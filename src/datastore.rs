use crate::db::{cron_db_cleanup, entities};
use crate::enums::{RecordClass, RecordType};
use crate::error::GoatNsError;
use crate::resourcerecord::InternalResourceRecord;
use crate::zones::{ZoneFile, ZoneRecord};
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, ModelTrait,
    QueryFilter, QuerySelect, TransactionTrait,
};
use std::str::from_utf8;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

type Responder<T> = oneshot::Sender<T>;

#[derive(Debug)]
/// Commands that can be sent to the datastore
pub enum Command {
    /// Query a record from the database
    GetRecord {
        /// Reversed vec of the name
        name: Vec<u8>,
        /// The type of record to get
        rrtype: RecordType,
        /// The class of record to get
        rclass: RecordClass,
        /// The response channel
        resp: Responder<Option<ZoneRecord>>,
    },
    /// Query a zone from the database
    GetZone {
        /// If you know the ID supply it
        id: Option<Uuid>,
        /// If you know the name supply it
        name: Option<String>,
        /// The response channel
        resp: Responder<Option<entities::zones::Model>>,
    },
    /// Query a list of zones from the database
    GetZoneNames {
        /// Filter by user
        user_id: Uuid,
        /// The offset to start at
        offset: u64,
        /// The number of records to return
        limit: u64,
        /// The response channel
        resp: Responder<Vec<entities::zones::Model>>,
    },
    /// Import a file directly into the database
    ImportFile {
        /// Filename to load
        filename: String,
        /// If you only want to import a single zone, specify the name
        zone_name: Option<String>,
        /// The response channel
        resp: Responder<()>,
    },
    /// Shutdown the datastore
    Shutdown,
    /// Create a new zone
    CreateZone {
        /// Zone data
        zone: entities::zones::ActiveModel,

        /// Zone ownership
        userid: Uuid,

        /// The response channel
        resp: Responder<entities::zones::Model>,
    },
    /// Delete a zone
    DeleteZone,
    /// update a zone
    UpdateZone,
    /// Delete user
    DeleteUser,
    /// Create a new user
    CreateUser {
        /// Username
        username: String,
        /// Reference from OIDC
        authref: String,
        /// Is this user an admin?
        admin: bool,
        /// Is this user disabled?
        disabled: bool,
        /// The response channel
        resp: Responder<bool>,
    },
    /// Get a user
    GetUser {
        /// Database ID if you have it
        id: Option<Uuid>,
        /// username if you have it
        username: Option<String>,
        /// The response channel
        resp: Responder<()>,
    },
    /// Update a user
    UpdateUser,
    /// Remove zone ownership (optionally include a user)
    DeleteOwnership {
        /// Zone ID
        zoneid: i64,
        /// User ID (db ID)
        userid: Option<Uuid>,
        /// The response channel
        resp: Responder<()>,
    },
    /// Get ownership of a zone, or zones for a user
    GetOwnership {
        /// Zone ID
        zoneid: Option<Uuid>,
        /// User ID
        userid: Option<Uuid>,
        /// The response Channel
        resp: Responder<Vec<entities::zones::Model>>,
    },
    /// Create ownership of a zone
    PostOwnership {
        /// Zone ID
        zoneid: Uuid,
        /// User ID
        userid: Uuid,
        /// The response channel
        resp: Responder<entities::ownership::Model>,
    },
}

async fn handle_soa_query(
    server_hostname: &str,
    conn: &DatabaseConnection,
    name: &[u8],
) -> Result<Option<InternalResourceRecord>, GoatNsError> {
    let txn = conn.begin().await?;

    let name = from_utf8(name)?;

    // get the zone
    match entities::zones::Entity::find()
        .filter(entities::zones::Column::Name.eq(name.to_string()))
        .one(&txn)
        .await?
    {
        Some(zone) => {
            // get the SOA record
            let soa = zone.get_soa_record(server_hostname);
            Ok(Some(soa))
        }
        None => {
            debug!("Zone not found during SOA query: {name}");
            Ok(None)
        }
    }
}

async fn handle_get_command(
    server_hostname: &str,
    // database pool
    conn: &DatabaseConnection,
    // this is the result from the things in memory
    // zone_get: Option<&ZoneRecord>,
    name: Vec<u8>,
    rrtype: RecordType,
    rclass: RecordClass,
    resp: oneshot::Sender<Option<ZoneRecord>>,
) -> Result<(), String> {
    debug!(
        "query name={:?} rrtype={rrtype:?} rclass={rclass}",
        from_utf8(&name).unwrap_or("-"),
    );

    // query the database
    let db_name =
        from_utf8(&name).map_err(|e| format!("Failed to convert name to utf8 - {e:?}"))?;

    let mut zr = ZoneRecord {
        name: name.clone(),
        typerecords: vec![],
    };

    //  if it's an SOA record we don't go to the database directly, it's based on the zone, but currently we're only doing this for the specific zone.

    // TODO: need to get an SOA for any record we're authoritative for?
    if rrtype == RecordType::SOA {
        match handle_soa_query(server_hostname, conn, &name).await {
            Ok(Some(soa)) => zr.typerecords.push(soa),
            Ok(None) => {
                info!("SOA not found for {db_name}");
            }
            Err(err) => {
                error!("Failed to query db: {err:?}");
            }
        }
    } else {
        match entities::records_merged::Entity::get_records(conn, db_name, rrtype, rclass, true)
            .await
        {
            Ok(value) => zr.typerecords.extend(
                value
                    .into_iter()
                    .filter_map(|rec| InternalResourceRecord::try_from(rec).ok()),
            ),
            Err(err) => {
                error!("Failed to query db: {err:?}")
            }
        };
    }

    // if let Some(value) = zone_get {
    //     // check if the type we want is in there, and only return the matching records
    //     let res: Vec<InternalResourceRecord> = value
    //         .to_owned()
    //         .typerecords
    //         .into_iter()
    //         .filter(|r| r == &rrtype && r == &rclass)
    //         .collect();
    //     zr.typerecords.extend(res);
    // };

    let result = match zr.typerecords.is_empty() {
        true => None,
        false => Some(zr),
    };

    if let Err(error) = resp.send(result) {
        debug!("error sending response from data store: {:?}", error)
    };
    Ok(())
}

pub async fn import_zonefile<C: ConnectionTrait>(
    pool: &C,
    zonefile: ZoneFile,
) -> Result<(), GoatNsError> {
    let zone: entities::zones::ActiveModel = zonefile.zone.into();
    let zone_name = zone.name.clone();

    debug!("Importing zone: {}", zone_name.as_ref());
    let zone_id = if zone.id.is_not_set() {
        match zone.insert(pool).await {
            Ok(zone_res) => {
                info!("Created zone: {:?}", zone_res);
                zone_res.id
            }
            Err(err) => {
                error!("Failed to create zone {}: {:?}", zone_name.as_ref(), err);
                return Err(err.into());
            }
        }
    } else {
        match zone.update(pool).await {
            Ok(zone_res) => {
                info!("Updated zone: {:?}", zone_res);
                zone_res.id
            }
            Err(err) => {
                error!("Failed to update zone {}: {:?}", zone_name.as_ref(), err);
                return Err(err.into());
            }
        }
    };
    for record in zonefile.records {
        let record_am: entities::records::ActiveModel = record.to_activemodel(zone_id);

        let record_clone = record_am.clone();
        if record_am.id.is_unchanged() || record_am.id.is_not_set() {
            match record_am.insert(pool).await {
                Ok(record_res) => {
                    debug!("Inserted record: {:?}", record_res);
                }
                Err(err) => {
                    error!(
                        "Failed to insert record {:?} in zone {}: {:?}",
                        record_clone,
                        zone_name.as_ref(),
                        err
                    );
                }
            }
        } else {
            match record_am.update(pool).await {
                Ok(record_res) => {
                    debug!("Updated record: {:?}", record_res);
                }
                Err(err) => {
                    error!(
                        "Failed to update record {:?} in zone {}: {:?}",
                        record_clone,
                        zone_name.as_ref(),
                        err
                    );
                }
            }
        }
    }
    Ok(())
}

/// Import a file directly into the database. Normally, you shouldn't use this directly, call it through calls to the datastore.
#[instrument(level = "info", skip(pool))]
pub async fn handle_import_file(
    pool: &DatabaseConnection,
    filename: String,
    zone_name: Option<String>,
) -> Result<(), GoatNsError> {
    let zones: Vec<ZoneFile> = crate::zones::load_zones(&filename)?;

    if zones.is_empty() {
        warn!("No zones to import!");
        return Err(GoatNsError::EmptyFile);
    } else {
        debug!("Starting import process");
    }

    let zones = if let Some(zone_name) = zone_name {
        zones
            .into_iter()
            .filter(|z| z.zone.name == zone_name)
            .collect()
    } else {
        zones
    };

    let txn = pool.begin().await?;
    for zonefile in zones {
        import_zonefile(&txn, zonefile).await?;
    }
    txn.commit()
        .await
        .inspect_err(|err| error!("Failed to commit transaction! {:?}", err))?;
    info!("Completed import process");
    Ok(())
}

#[instrument(level = "debug", skip(tx, pool))]
/// find a zone by id or name
async fn handle_get_zone(
    tx: oneshot::Sender<Option<entities::zones::Model>>,
    pool: DatabaseConnection,
    id: Option<Uuid>,
    name: Option<String>,
) -> Result<(), GoatNsError> {
    let mut zone_query = match id {
        None => entities::zones::Entity::find(),
        Some(id) => entities::zones::Entity::find_by_id(id),
    };

    if let Some(name) = name {
        zone_query = zone_query.filter(entities::zones::Column::Name.eq(name));
    }
    let zone = zone_query.one(&pool).await?;

    tx.send(zone).map_err(|e| {
        GoatNsError::SendError(format!("Failed to send response on tokio channel: {e:?}"))
    })
}

async fn handle_get_zone_names(
    user_id: Uuid,
    tx: oneshot::Sender<Vec<entities::zones::Model>>,
    pool: DatabaseConnection,
    offset: u64,
    limit: u64,
) -> Result<(), GoatNsError> {
    let txn = pool.begin().await?;

    let results = entities::users::Entity::find_by_id(user_id)
        .one(&txn)
        .await?;

    let zones = match results {
        None => Vec::new(),
        Some(user) => {
            let ownerships = user
                .find_related(entities::ownership::Entity)
                .offset(Some(offset))
                .limit(Some(limit))
                .all(&txn)
                .await?;
            let ownership_ids = ownerships
                .into_iter()
                .map(|o| o.zoneid)
                .collect::<Vec<Uuid>>();
            entities::zones::Entity::find()
                .filter(entities::zones::Column::Id.is_in(ownership_ids))
                .all(&txn)
                .await?
        }
    };

    tx.send(zones).map_err(|e| {
        GoatNsError::SendError(format!("Failed to send response on tokio channel: {e:?}"))
    })
}

#[instrument(level = "info", skip(connpool))]
pub(crate) async fn handle_message(
    server_hostname: &str,
    cmd: Command,
    connpool: DatabaseConnection,
) -> Result<(), String> {
    match cmd {
        Command::GetZone { id, name, resp } => {
            let res = handle_get_zone(resp, connpool, id, name).await;
            if let Err(e) = res {
                error!("{e:?}")
            };
        }
        Command::GetZoneNames {
            resp,
            user_id,
            offset,
            limit,
        } => {
            let res = handle_get_zone_names(user_id, resp, connpool, offset, limit).await;
            if let Err(e) = res {
                error!("{e:?}")
            };
        }
        Command::Shutdown => {
            #[cfg(test)]
            println!("### Datastore was sent shutdown message, shutting down.");
            info!("Datastore was sent shutdown message, shutting down.");
            return Err("Datastore was sent shutdown message, shutting down.".to_string());
        }
        Command::ImportFile {
            filename,
            resp,
            zone_name,
        } => {
            handle_import_file(&connpool, filename, zone_name)
                .await
                .map_err(|e| format!("{e:?}"))?;
            match resp.send(()) {
                Ok(_) => info!("DS Sent Success"),
                Err(err) => {
                    let res = format!("Failed to send response: {err:?}");
                    info!("{res}");
                }
            }
        }
        Command::GetRecord {
            name,
            rrtype,
            rclass,
            resp,
        } => {
            let res =
                handle_get_command(server_hostname, &connpool, name, rrtype, rclass, resp).await;
            if let Err(e) = res {
                error!("{e:?}")
            };
        }
        Command::CreateZone {
            zone,
            userid: _,
            resp: _,
        } => {
            let zone_cloned = zone.clone();
            match zone.update(&connpool).await {
                Ok(_zone) => {
                    // TODO: create the ownership
                    todo!();
                    //     entities::ownership::ActiveModel {
                    //         id: NotSet,
                    //         userid: Set(userid),
                    //         zoneid: Set(userid),
                    //     }
                    //     .insert(connpool)
                    //     .await
                    //     .map_err(|e| format!("{e:?}"))?;
                    // }

                    // if let Err(err) = resp.send(*zone.clone()) {
                    //     error!(
                    //         "Failed to send message back to caller after creating zone {zone:?}: {err:?}"
                    //     );
                    // } else {
                    //     info!("Created zone: {:?}", zone);
                    // }
                }
                Err(err) => {
                    error!("Failed to create zone: {zone_cloned:?} {err:?}");
                }
            };
        }
        Command::DeleteZone => {
            error!("Unimplemented: Command::DeleteZone")
        }
        Command::UpdateZone => {
            error!("Unimplemented: Command::PatchZone")
        }
        Command::CreateUser {
            username,
            authref,
            admin,
            disabled,
            resp,
        } => {
            let new_user = entities::users::ActiveModel {
                id: NotSet,
                username: Set(username.clone()),
                authref: Set(Some(authref.clone())),
                admin: Set(admin),
                disabled: Set(disabled),
                ..Default::default()
            };
            debug!("Creating: {new_user:?}");
            let res = match new_user.insert(&connpool).await {
                Ok(_) => true,
                Err(error) => {
                    error!("Failed to create {username}: {error:?}");
                    false
                }
            };
            if let Err(error) = resp.send(res) {
                error!("Failed to send message back to caller: {error:?}");
            }
        }
        Command::DeleteUser => error!("Unimplemented: Command::DeleteUser"),
        Command::GetUser { .. } => error!("Unimplemented: Command::GetUser"),
        Command::UpdateUser => error!("Unimplemented: Command::PatchUser"),
        Command::DeleteOwnership { .. } => error!("Unimplemented: Command::DeleteOwnership"),
        Command::GetOwnership {
            zoneid,
            userid,
            resp,
        } => {
            let mut query = entities::ownership::Entity::find();
            if let Some(zoneid) = zoneid {
                query = query.filter(entities::ownership::Column::Zoneid.eq(zoneid));
            };
            if let Some(userid) = userid {
                query = query.filter(entities::ownership::Column::Userid.eq(userid));
            };
            match query
                .find_also_related(entities::zones::Entity)
                .all(&connpool)
                .await
            {
                Err(err) => {
                    error!("Failed to get ownership: {err:?}");
                    if let Err(err) = resp.send(Vec::new()) {
                        error!("Failed to send response: {err:?}");
                    }
                }
                Ok(values) => {
                    let zones: Vec<entities::zones::Model> = values
                        .into_iter()
                        .filter_map(|(_ownership, zone_opt)| zone_opt)
                        .collect();
                    if let Err(err) = resp.send(zones) {
                        error!("Failed to send response: {err:?}");
                    }
                }
            }
        }
        Command::PostOwnership { .. } => {
            error!("Unimplemented command: Command::PostOwnership")
        }
    }
    Ok(())
}

/// Manages the datastore, waits for signals from the server instances and responds with data
pub async fn manager(
    mut rx: mpsc::Receiver<crate::datastore::Command>,
    server_hostname: String,
    connpool: DatabaseConnection,
    cron_db_cleanup_timer: Option<Duration>,
) -> Result<(), String> {
    if let Some(timer) = cron_db_cleanup_timer {
        debug!("Spawning DB cron cleanup task");
        tokio::spawn(cron_db_cleanup(connpool.clone(), timer, None));
    }

    while let Some(cmd) = rx.recv().await {
        if handle_message(&server_hostname, cmd, connpool.clone())
            .await
            .is_err()
        {
            break;
        };
    }
    #[cfg(test)]
    println!("### manager is done!");
    Ok(())
}
