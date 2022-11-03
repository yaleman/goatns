use crate::datastore;
use crate::enums::RecordType;
use tide::{self, Response, StatusCode};

use tokio::sync::mpsc::Sender;

// #[get("/")]
async fn index(_req: tide::Request<State>) -> tide::Result {
    tide::Result::Ok(Response::builder(200).body("Hello world").build())
}

// #[get("/status")]
async fn status(_req: tide::Request<State>) -> tide::Result {
    tide::Result::Ok(Response::builder(200).body("Ok").build())
}

// #[get("/query/<qname>/<qtype>")]
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
    let ds_req: datastore::Command = datastore::Command::Get {
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

pub async fn build(tx: Sender<datastore::Command>) -> Result<tide::Server<State>, tide::Error> {
    let mut app = tide::with_state(State { tx });

    app.at("/").get(index);
    app.at("/status").get(status);
    app.at("/query/:qname/:qtype").get(api_query);
    app.with(driftwood::ApacheCombinedLogger);

    Ok(app)
}
