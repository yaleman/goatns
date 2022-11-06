use crate::config::ConfigFile;
use crate::datastore;
use crate::enums::RecordType;
use tide::{self, Response, StatusCode};

use tokio::sync::mpsc::Sender;

pub mod api;
#[macro_use]
pub mod macros;
pub mod ui;

pub const STATUS_OK: &str = "Ok";

async fn status(_req: tide::Request<State>) -> tide::Result {
    tide::Result::Ok(Response::builder(200).body(STATUS_OK).build())
}

async fn api_query(req: tide::Request<State>) -> tide::Result {
    let qname = req.param("qname")?;
    let qtype = req.param("qtype")?;

    use tokio::sync::oneshot;

    let rrtype: RecordType = qtype.into();
    if let RecordType::InvalidType = rrtype {
        // return Err(tide::BadRequest(
        // ));
        return Err(tide::Error::from_str(
            StatusCode::BadRequest,
            format!("Invalid RRTYPE requested: {qtype:?}"),
        ));
    }

    let (tx_oneshot, rx_oneshot) = oneshot::channel();
    let ds_req: datastore::Command = datastore::Command::GetRecord {
        name: qname.into(),
        rrtype,
        rclass: crate::RecordClass::Internet,
        resp: tx_oneshot,
    };

    // here we talk to the datastore to pull the result
    match req.state().tx.send(ds_req).await {
        Ok(_) => log::trace!("Sent a request to the datastore!"),
        // TODO: handle this properly
        Err(error) => log::error!("Error sending to datastore: {:?}", error),
    };

    let record: Option<crate::zones::ZoneRecord> = match rx_oneshot.await {
        Ok(value) => match value {
            Some(zr) => {
                log::debug!("DS Response: {}", zr);
                Some(zr)
            }
            None => {
                log::debug!("No response from datastore");
                return Err(tide::Error::from_str(
                    StatusCode::NotFound,
                    "No response from datastore",
                ));
            }
        },
        Err(error) => {
            log::error!("Failed to get response from datastore: {:?}", error);
            return Err(tide::Error::from_str(
                StatusCode::InternalServerError,
                "Sorry, something went wrong.",
            ));
        }
    };

    match record {
        None => Err(tide::Error::from_str(tide::StatusCode::NotFound, "")),
        Some(value) => match serde_json::to_string(&value.typerecords) {
            Ok(value) => Ok(tide::Response::from(value)),
            Err(error) => {
                log::error!("Failed to serialize response: {error:?}");
                Err(tide::Error::from_str(
                    tide::StatusCode::InternalServerError,
                    "Failed to serialize response.",
                ))
            }
        },
    }
}

/// Internal State handler for the datastore object within the API
#[derive(Debug, Clone)]
pub struct State {
    tx: Sender<datastore::Command>,
}

pub async fn build(
    tx: Sender<datastore::Command>,
    config: &ConfigFile,
) -> Result<tide::Server<State>, tide::Error> {
    let mut app = tide::with_state(State { tx: tx.clone() });

    app.with(
        tide::sessions::SessionMiddleware::new(
            tide::sessions::MemoryStore::new(),
            "supersekretasdf1234asdfasdfdsfasdf".as_bytes(),
        )
        .with_same_site_policy(tide::http::cookies::SameSite::Strict),
    );

    app.at("/").get(ui::index);
    app.at("/query/:qname/:qtype").get(api_query);
    app.at("/status").get(status);
    app.at("/ui/zones/list").get(ui::zones_list);
    app.at("/ui/zones/:id").get(ui::zone_view);
    if let Err(error) = app.at("/ui/static").serve_dir(&config.api_static_dir) {
        log::error!(
            "Failed to find static file dir ({})! {error:?}",
            &config.api_static_dir.to_string_lossy()
        )
    };

    app.at("/api").nest(api::new(tx));

    app.with(driftwood::ApacheCombinedLogger);

    Ok(app)
}
