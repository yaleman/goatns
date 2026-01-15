use std::collections::HashMap;

use crate::datastore::Command;
use crate::db::entities;
use crate::enums::{RecordClass, RecordType};
use crate::web::constants::{SESSION_REDIRECT_KEY, SESSION_USER_KEY};
use crate::web::utils::Urls;
use crate::web::GoatStateTrait;
use askama::Template;
use askama_web::WebTemplate;
use axum::Router;
use axum::extract::{OriginalUri, Path, Query, State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use serde::Deserialize;
use sea_orm::ModelTrait;
use tower_sessions::Session;
use tracing::{debug, error, instrument, trace};
use uuid::Uuid;

use super::GoatState;

mod admin_ui;
mod profile;
mod user_settings;
mod zones;

#[derive(Template, WebTemplate)]
#[template(path = "view_zones.html")]
pub(crate) struct TemplateViewZones {
    zones: Vec<entities::zones::Model>,
    pub user_is_admin: bool,
    message: Option<String>,
    error: Option<String>,
}

#[derive(Template, WebTemplate)]
#[template(path = "view_zone.html")]
pub(crate) struct TemplateViewZone {
    zone: entities::zones::Model,
    records: Vec<UiZoneRecord>,
    record_type_options: Vec<RecordTypeOption>,
    record_class_options: Vec<RecordClassOption>,
    pub user_is_admin: bool,
}

pub(crate) struct UiZoneRecord {
    id: Uuid,
    name: String,
    rrtype: u16,
    rrtype_text: String,
    rclass: u16,
    rclass_text: String,
    ttl: Option<u32>,
    rdata: String,
}

pub(crate) struct RecordTypeOption {
    value: u16,
    label: String,
}

pub(crate) struct RecordClassOption {
    value: u16,
    label: String,
}

#[derive(Deserialize, Debug)]
pub(crate) struct ViewZonesQueryString {
    message: Option<String>,
    error: Option<String>,
}

#[instrument(level = "info", skip(state, session))]
pub(crate) async fn zones_list(
    State(state): State<GoatState>,
    mut session: Session,
    OriginalUri(path): OriginalUri,
    Query(query): Query<ViewZonesQueryString>,
) -> Result<TemplateViewZones, impl IntoResponse> {
    let user = check_logged_in(&mut session, path)
        .await
        .map_err(|err| err.into_response())?;
    let (os_tx, os_rx) = tokio::sync::oneshot::channel();

    let offset = 0;
    let limit = 20;

    trace!("Sending request for zones");
    if let Err(err) = state
        .read()
        .await
        .tx
        .send(Command::GetZoneNames {
            resp: os_tx,
            user_id: user.id,
            offset,
            limit,
        })
        .await
    {
        eprintln!("failed to send GetZoneNames command to datastore: {err:?}");
        error!("failed to send GetZoneNames command to datastore: {err:?}");
        return Err(Urls::Dashboard.redirect().into_response());
    };

    let zones = os_rx.await.map_err(|err| {
        error!("Failed to get zones for user={:?} error={:?}", user.id, err);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error getting zones: {err:?}"),
        )
            .into_response()
    })?;

    Ok(TemplateViewZones {
        zones,
        user_is_admin: user.admin,
        message: query.message,
        error: query.error,
    })
}

#[instrument(level = "debug", skip(state))]
pub(crate) async fn zone_view(
    OriginalUri(path): OriginalUri,
    Path(id): Path<Uuid>,
    State(state): State<GoatState>,
    mut session: Session,
) -> Result<TemplateViewZone, impl IntoResponse> {
    let user = check_logged_in(&mut session, path)
        .await
        .map_err(|err| err.into_response())?;

    let (os_tx, os_rx) = tokio::sync::oneshot::channel();
    let cmd = Command::GetZone {
        resp: os_tx,
        id: Some(id),
        name: None,
    };
    debug!("{cmd:?}");
    if let Err(err) = state.read().await.tx.send(cmd).await {
        eprintln!("failed to send GetZone command to datastore: {err:?}");
        error!("failed to send GetZone command to datastore: {err:?}");
        return Err(Urls::ZonesList.redirect().into_response());
    };

    let zone = match os_rx.await {
        Ok(zone) => match zone {
            Some(value) => value,
            None => {
                return Err((
                    axum::http::StatusCode::NOT_FOUND,
                    format!("Zone '{id}' not found"),
                )
                    .into_response());
            }
        },
        Err(err) => {
            error!("failed to get response from datastore: {err:?}");
            return Err(Urls::ZonesList.redirect().into_response());
        }
    };

    let txn = state.get_db_txn().await.map_err(|err| {
        error!("Failed to begin DB transaction for zone records: {err:?}");
        Urls::ZonesList.redirect().into_response()
    })?;
    let records = zone
        .find_related(entities::records::Entity)
        .all(&txn)
        .await
        .map_err(|err| {
            error!("Error getting records for zone {}: {err:?}", zone.name);
            Urls::ZonesList.redirect().into_response()
        })?;

    let records = records
        .into_iter()
        .map(|record| {
            let rrtype = RecordType::from(&record.rrtype);
            let rrtype_text = if rrtype == RecordType::InvalidType {
                record.rrtype.to_string()
            } else {
                rrtype.to_string()
            };
            let rclass = RecordClass::from(&record.rclass);
            let rclass_text = if rclass == RecordClass::InvalidType {
                record.rclass.to_string()
            } else {
                rclass.to_string()
            };

            UiZoneRecord {
                id: record.id,
                name: record.name,
                rrtype: record.rrtype,
                rrtype_text,
                rclass: record.rclass,
                rclass_text,
                ttl: record.ttl,
                rdata: record.rdata,
            }
        })
        .collect();

    let record_type_options = vec![
        RecordType::A,
        RecordType::AAAA,
        RecordType::CAA,
        RecordType::CNAME,
        RecordType::HINFO,
        RecordType::LOC,
        RecordType::MX,
        RecordType::NAPTR,
        RecordType::NS,
        RecordType::PTR,
        RecordType::SOA,
        RecordType::TXT,
        RecordType::URI,
    ]
    .into_iter()
    .map(|record_type| RecordTypeOption {
        value: record_type as u16,
        label: record_type.to_string(),
    })
    .collect();

    let record_class_options = vec![
        RecordClass::Internet,
        RecordClass::CsNet,
        RecordClass::Chaos,
        RecordClass::Hesiod,
    ]
    .into_iter()
    .map(|record_class| RecordClassOption {
        value: record_class as u16,
        label: record_class.to_string(),
    })
    .collect();

    trace!("Returning zone: {zone:?}");
    Ok(TemplateViewZone {
        zone,
        records,
        record_type_options,
        record_class_options,
        user_is_admin: user.admin,
    })
}

pub async fn check_logged_in(
    session: &mut Session,
    path: Uri,
) -> Result<entities::users::Model, Redirect> {
    let user: Option<entities::users::Model> = session
        .get(SESSION_USER_KEY)
        .await
        .map_err(|_e| Urls::Login.redirect())?;

    let redirect_path = Some(
        path.path_and_query()
            .map(|v| v.to_string())
            .unwrap_or("/".to_string()),
    );

    // Check the database to make sure they're actually legit and not disabled
    match user {
        Some(user) => {
            if user.disabled {
                debug!("User {} is disabled, clearing session", user.id);
                session.clear().await;
                Err(Urls::Login.redirect())
            } else {
                Ok(user)
            }
        }
        None => {
            session.clear().await;

            session
                .insert(SESSION_REDIRECT_KEY, redirect_path)
                .await
                .map_err(|e| {
                    debug!("Couldn't store redirect for user: {e:?}");
                    Urls::Home.redirect_with_query(HashMap::from([(
                        "error",
                        "An error storing your session occurred!",
                    )]))
                })?;
            debug!("Not-logged-in-user tried to log in, how rude!");
            Err(Urls::Login.redirect())
        }
    }
}

pub(crate) async fn dashboard_redirect(
    mut session: Session,
    OriginalUri(path): OriginalUri,
) -> Result<Redirect, Redirect> {
    check_logged_in(&mut session, path).await?;
    Ok(Urls::ZonesList.redirect())
}

pub fn new() -> Router<GoatState> {
    Router::new()
        .route("/", get(dashboard_redirect))
        .route("/zones/{id}", get(zone_view))
        .route("/zones/list", get(zones_list))
        .route("/zones/new", post(zones::zones_new_post))
        .route("/profile", get(profile::user_profile_get))
        .nest("/settings", user_settings::router())
        .nest("/admin", admin_ui::router())
}
