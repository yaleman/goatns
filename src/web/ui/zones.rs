use goatns_macros::get_logged_in;

use crate::zones::FileZoneRecord;

use super::prelude::*;

pub(crate) static VALID_ZONE_REGEX: &str = r"^[a-zA-Z0-9\-_]+\.[a-z]+$";

#[derive(Template)]
#[template(path = "view_zones.html")]
struct TemplateViewZones {
    zones: Vec<FileZone>,
    pub user_is_admin: bool,
}

#[derive(Template)]
#[template(path = "view_zone.html")]
struct TemplateViewZone {
    zone: FileZone,
    pub user_is_admin: bool,
}

#[derive(Template)]
#[template(path = "zone_create.html")]
struct TemplateCreateZone {
    user_is_admin: bool,
    zone: String,

    message: String,
    message_is_error: bool,
}

#[derive(Debug, Deserialize)]
pub struct FormCreateZone {
    zone: String,
    csrftoken: Option<String>,
}

#[derive(Template)]
#[template(path = "zone_delete.html")]
struct TemplateDeleteZone {
    user_is_admin: bool,
    zone: String,
}

#[derive(Debug, Deserialize)]
pub struct FormDeleteZone {
    csrftoken: Option<String>,
}

#[debug_handler]
pub async fn zones_create_post(
    State(state): State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
    Form(zoneform): Form<FormCreateZone>,
) -> impl IntoResponse {
    let user = get_logged_in!();
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    if let Some(csrf_token) = &zoneform.csrftoken {
        if !validate_csrf_expiry(csrf_token, &mut session) {
            error!(
                "CSRF validation failed while trying to create a zone: {:?}",
                zoneform
            );
            // TODO: this should throw an error
            return redirect_to_login().into_response();
        };
    }

    let message = if zoneform.zone.trim().is_empty() {
        "Zone name cannot be empty".to_string()
    } else if Regex::new(VALID_ZONE_REGEX)
        .unwrap()
        .is_match(zoneform.zone.trim())
    {
        // zone name is valid, send off a request to create it
        let command = Command::CreateZone {
            resp: os_tx,
            user: user.clone(),
            rname: user.email.clone(),
            zone_name: zoneform.zone.trim().to_string(),
        };
        match state.read().await.tx.send(command).await {
            Err(err) => {
                log::error!("Failed to send message to backend: {:?}", err);

                "Failed to send message to backend, try again please.".to_string()
            }
            Ok(_) => {
                let result: DataStoreResponse = os_rx.await.expect("Failed to get response");
                match result {
                    DataStoreResponse::Created(id) => {
                        if id > 0 {
                            return redirect_to_zone(id).into_response();
                        } else {
                            return redirect_to_zones_list().into_response();
                        }
                    }
                    DataStoreResponse::Failure(err) => {
                        log::error!("Failed to create zone: {:?}", err);
                        format!("Failed to create zone, try again please: {}", err)
                    }
                    DataStoreResponse::AlreadyExists => "Zone already exists".to_string(),
                    _ => "Unknown error".to_string(),
                }
            }
        }
    } else {
        "Zone name is invalid".to_string()
    };

    let context = TemplateCreateZone {
        // zones,
        user_is_admin: user.admin,
        zone: zoneform.zone.trim().to_string(),

        message,
        message_is_error: true,
    };
    Response::builder()
        .status(200)
        .body(context.render().unwrap())
        .unwrap()
        .into_response()
}

// #[debug_handler]
pub async fn zones_create_get(
    State(_state): State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
) -> impl IntoResponse {
    let user = get_logged_in!();

    if let Err(err) = store_api_csrf_token(&mut session, None) {
        error!("Failed to store CSRF token! {}", err);
        return redirect_to_zones_list().into_response();
    };

    let context = TemplateCreateZone {
        user_is_admin: user.admin,
        zone: "".to_string(),
        message: "".to_string(),
        message_is_error: false,
    };
    Response::builder()
        .status(200)
        .body(context.render().unwrap())
        .unwrap()
        .into_response()
}

// #[debug_handler]
pub async fn zones_list(
    State(state): State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
) -> impl IntoResponse {
    let user = get_logged_in!();
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    let offset = 0;
    let limit = 20;

    log::trace!("Sending request for zones");
    if let Err(err) = state
        .read()
        .await
        .tx
        .send(Command::GetZoneNames {
            resp: os_tx,
            user: user.clone(),
            offset,
            limit,
        })
        .await
    {
        eprintln!("failed to send GetZoneNames command to datastore: {err:?}");
        log::error!("failed to send GetZoneNames command to datastore: {err:?}");
        return redirect_to_dashboard().into_response();
    };

    let zones = os_rx.await.expect("Failed to get response: {res:?}");

    log::debug!("about to return zone list... found {} zones", zones.len());
    let context = TemplateViewZones {
        zones,
        user_is_admin: user.admin,
    };
    Response::builder()
        .status(200)
        .body(context.render().unwrap())
        .unwrap()
        .into_response()
}

#[debug_handler]
pub(crate) async fn zone_view(
    Path(name_or_id): Path<i64>,
    axum::extract::State(state): axum::extract::State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
) -> impl IntoResponse {
    let user = get_logged_in!();

    let (os_tx, os_rx) = tokio::sync::oneshot::channel();
    let cmd = Command::GetZone {
        resp: os_tx,
        id: Some(name_or_id),
        name: None,
    };
    log::debug!("{cmd:?}");
    if let Err(err) = state.read().await.tx.send(cmd).await {
        eprintln!("failed to send GetZone command to datastore: {err:?}");
        log::error!("failed to send GetZone command to datastore: {err:?}");
        return redirect_to_zones_list().into_response();
    };

    let zone = match os_rx.await.expect("Failed to get response: {res:?}") {
        Some(value) => value,
        None => {
            info!("Zone not found {} for user {}", name_or_id, user.username);
            return redirect_to_zones_list().into_response();
        }
    };

    log::trace!("Returning zone: {zone:?}");
    let context = TemplateViewZone {
        zone,
        user_is_admin: user.admin,
    };
    Response::new(context.render().unwrap()).into_response()
}

#[debug_handler]
pub async fn zone_delete_post(
    State(state): State<GoatState>,
    Path(id): Path<i64>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
    Form(zoneform): Form<FormDeleteZone>,
) -> impl IntoResponse {
    let user = get_logged_in!();

    if let Some(csrf_token) = &zoneform.csrftoken {
        if !validate_csrf_expiry(csrf_token, &mut session) {
            error!(
                "CSRF validation failed while trying to delete a zone: {:?}",
                zoneform
            );
            // TODO: this should throw an error
            return redirect_to_login().into_response();
        };
    }

    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    let msg = Command::DeleteZone {
        resp: os_tx,
        user,
        id,
    };

    if let Err(err) = state.read().await.tx.send(msg).await {
        log::error!("Failed to send GetZone command to datastore: {err:?}");
        return redirect_to_zone(id).into_response();
    };

    match os_rx.await.expect("Failed to get response: {res:?}") {
        DataStoreResponse::Deleted | DataStoreResponse::NotFound => {
            redirect_to_zones_list().into_response()
        }
        DataStoreResponse::Failure(error) => {
            todo!("{}", error);
        }
        _ => {
            todo!();
        }
    }
}

#[debug_handler]
pub async fn zone_delete_get(
    State(state): State<GoatState>,
    Path(id): Path<i64>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
    // Form(zoneform): Form<FormCreateZone>,
) -> impl IntoResponse {
    let user = get_logged_in!();

    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    let msg = Command::GetZone {
        resp: os_tx,
        id: Some(id),
        name: None,
    };

    if let Err(err) = state.read().await.tx.send(msg).await {
        log::error!("Failed to send GetZone command to datastore: {err:?}");
        return redirect_to_zone(id).into_response();
    };

    let zone = match os_rx.await.expect("Failed to get response: {res:?}") {
        Some(val) => val,
        None => todo!("Send a not found"),
    };

    let context = TemplateDeleteZone {
        zone: zone.name,
        user_is_admin: user.admin,
    };
    Response::new(context.render().unwrap()).into_response()
    // TODO: show the "are you sure" button, add csrf things
}

#[derive(Deserialize)]
pub struct FormCreateRecord {
    pub name: String,
    pub rrtype: String,
    pub ttl: u32,
    pub rdata: String,
    pub csrftoken: Option<String>,
}

#[debug_handler]
pub async fn zone_create_record_get(
    State(state): State<GoatState>,
    Path(id): Path<i64>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
    // Form(zoneform): Form<FormCreateZone>,
) -> impl IntoResponse {
    let user = get_logged_in!();

    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    let msg = Command::GetOwnership {
        resp: os_tx,
        zoneid: Some(id),
        userid: user.id,
    };
    let userid = match user.id {
        None => {
            log::error!("User {} does not have an ID... how?", user.username);
            return redirect_to_zone(id).into_response();
        }
        Some(id) => id,
    };

    if let Err(err) = state.read().await.tx.send(msg).await {
        log::error!("Failed to send GetOwnership command to datastore: {err:?}");
        return redirect_to_zone(id).into_response();
    };

    let ownership = match os_rx.await {
        Ok(val) => val,
        Err(err) => {
            log::error!("Failed to get response: {err:?}");
            return redirect_to_zone(id).into_response();
        }
    };
    if ownership.userid == userid && !user.admin {
        log::error!(
            "User {} does not own zone {} and user isn't admin",
            user.username,
            id
        );
        log::error!("Ownership data: {:?}", ownership);
        return redirect_to_zone(id).into_response();
    };

    let csrftoken = store_api_csrf_token(&mut session, Some(900)).unwrap_or("".to_string());

    let form = FormCreateRecord {
        name: "".to_string(),
        rrtype: "A".to_string(),
        ttl: 300, // TODO this should be the zone min TTL?
        rdata: "".to_string(),
        csrftoken: Some(csrftoken.clone()),
    };

    let context = TemplateCreateRecord {
        form,
        user_is_admin: user.admin,
        message: "".to_string(),
        message_is_error: false,
        csrftoken,
    };

    Response::new(context.render().unwrap()).into_response()
}

#[derive(Template)]
#[template(path = "record_create.html")]
struct TemplateCreateRecord {
    form: FormCreateRecord,
    user_is_admin: bool,
    csrftoken: String,
    message: String,
    message_is_error: bool,
}

#[debug_handler]
pub async fn zone_create_record_post(
    State(state): State<GoatState>,
    Path(id): Path<i64>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
    Form(form): Form<FormCreateRecord>,
) -> impl IntoResponse {
    let user = get_logged_in!();

    if form.csrftoken.is_none() {
        error!("CSRF token is missing");
        return redirect_to_zone(id).into_response();
    } else if !validate_csrf_expiry(
        &form.csrftoken.clone().unwrap_or("".to_string()),
        &mut session,
    ) {
        return redirect_to_zone(id).into_response();
    }
    info!("CSRF validation passed");

    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    let msg = Command::GetOwnership {
        resp: os_tx,
        zoneid: Some(id),
        userid: user.id,
    };
    let userid = match user.id {
        None => {
            log::error!("User {} does not have an ID... how?", user.username);
            return redirect_to_zone(id).into_response();
        }
        Some(id) => id,
    };

    if let Err(err) = state.read().await.tx.send(msg).await {
        log::error!("Failed to send GetOwnership command to datastore: {err:?}");
        return redirect_to_zone(id).into_response();
    };

    let ownership = match os_rx.await {
        Ok(val) => val,
        Err(err) => {
            log::error!("Failed to get response: {err:?}");
            return redirect_to_zone(id).into_response();
        }
    };
    if ownership.userid != userid && !user.admin {
        log::error!("User {} does not own zone {}", user.username, id);
        log::error!("Ownership data: {:?}", ownership);
        return redirect_to_zone(id).into_response();
    }

    // TODO: we should do some validation of input here, maybe? :D

    info!("about to create fzr");
    let record = FileZoneRecord {
        zoneid: Some(id),
        id: None,
        name: form.name.clone(),
        rrtype: form.rrtype.clone(),
        class: "IN".into(),
        rdata: form.rdata.clone(),
        ttl: form.ttl,
    };

    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    let msg = Command::CreateZoneRecord {
        zoneid: id,
        userid,
        record: record.clone(),
        resp: os_tx,
    };

    let (message, message_is_error) = match state.read().await.tx.send(msg).await {
        Err(err) => {
            log::error!("Failed to send CreateZoneRecord command to datastore: {err:?}");
            (
                "Failed to send backend message, please try again!".to_string(),
                true,
            )
        }
        Ok(_) => match os_rx.await {
            Ok(DataStoreResponse::Created(_)) => (String::new(), false),
            Ok(DataStoreResponse::Failure(err)) => {
                log::error!(
                    "Failed to create zone (id: {} record ({:?}): {:?}",
                    id,
                    record,
                    err
                );
                (
                    "Failed to create zone record, try again or contact an admin if it keeeps happening!".to_string(),
                    true,
                )
            }
            Ok(DataStoreResponse::AlreadyExists) => {
                ("Zone record already exists".to_string(), true)
            }
            _ => ("Unknown error".to_string(), true),
        },
    };

    let context = TemplateCreateRecord {
        form,
        user_is_admin: user.admin,
        csrftoken: store_api_csrf_token(&mut session, Some(900)).unwrap_or("".to_string()),
        message,
        message_is_error,
    };

    if message_is_error {
        context.render().unwrap().into_response()
    } else {
        redirect_to_zone(id).into_response()
    }
}

pub async fn zone_edit_get(
    State(_state): State<GoatState>,
    Path(_id): Path<i64>,
    mut _session: Session,
    OriginalUri(_path): OriginalUri,
) -> impl IntoResponse {
    todo!()
}
pub async fn zone_edit_post(
    State(_state): State<GoatState>,
    Path(_id): Path<i64>,
    mut _session: Session,
    OriginalUri(_path): OriginalUri,
) -> impl IntoResponse {
    // let user = get_logged_in!();

    // if !form.csrftoken.is_some() {
    //     error!("CSRF token is missing");
    //     return redirect_to_zone(id).into_response();
    // } else {
    //     if !validate_csrf_expiry(
    //         &form.csrftoken.clone().unwrap_or("".to_string()),
    //         &mut session,
    //     ) {
    //         return redirect_to_zone(id).into_response();
    //     }
    // }
    // info!("CSRF validation passed");
    todo!()
}
pub async fn zone_edit_record_get(
    State(_state): State<GoatState>,
    Path(_id): Path<i64>,
    mut _session: Session,
    OriginalUri(_path): OriginalUri,
) -> impl IntoResponse {
    todo!()
}
pub async fn zone_edit_record_post(
    State(_state): State<GoatState>,
    Path(_id): Path<i64>,
    mut _session: Session,
    OriginalUri(_path): OriginalUri,
) -> impl IntoResponse {
    // let user = get_logged_in!();

    // if !form.csrftoken.is_some() {
    //     error!("CSRF token is missing");
    //     return redirect_to_zone(id).into_response();
    // } else {
    //     if !validate_csrf_expiry(
    //         &form.csrftoken.clone().unwrap_or("".to_string()),
    //         &mut session,
    //     ) {
    //         return redirect_to_zone(id).into_response();
    //     }
    // }
    // info!("CSRF validation passed");
    todo!()
}
pub async fn zone_delete_record_get(
    State(_state): State<GoatState>,
    Path(_id): Path<i64>,
    mut _session: Session,
    OriginalUri(_path): OriginalUri,
) -> impl IntoResponse {
    todo!()
}
pub async fn zone_delete_record_post(
    State(_state): State<GoatState>,
    Path(_id): Path<i64>,
    mut _session: Session,
    OriginalUri(_path): OriginalUri,
) -> impl IntoResponse {
    // let user = get_logged_in!();

    // if !form.csrftoken.is_some() {
    //     error!("CSRF token is missing");
    //     return redirect_to_zone(id).into_response();
    // } else {
    //     if !validate_csrf_expiry(
    //         &form.csrftoken.clone().unwrap_or("".to_string()),
    //         &mut session,
    //     ) {
    //         return redirect_to_zone(id).into_response();
    //     }
    // }
    // info!("CSRF validation passed");
    todo!()
}
