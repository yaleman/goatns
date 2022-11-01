use crate::config::ConfigFile;
use crate::datastore;
use crate::enums::RecordType;
use rocket;
use rocket::State;
use tokio::sync::mpsc::Sender;

#[derive(Responder)]
enum ResponseError {
    #[response(status = 400)]
    Failed(String),
    #[response(status = 400)]
    BadRequest(String),
    #[response(status = 404)]
    NotFound(()),
}

#[get("/")]
async fn index() -> &'static str {
    "Hello, world!"
}

#[get("/status")]
async fn status() -> &'static str {
    "OK"
}

#[get("/query/<qname>/<qtype>")]
async fn api_query(
    qname: &str,
    qtype: &str,
    ds: &State<Datastore>,
) -> Result<String, ResponseError> {
    use tokio::sync::oneshot;

    let rrtype: RecordType = qtype.into();
    if let RecordType::InvalidType = rrtype {
        return Err(ResponseError::BadRequest(format!(
            "Invalid RRTYPE requested: {qtype:?}"
        )));
    }

    let (tx_oneshot, rx_oneshot) = oneshot::channel();
    let ds_req: datastore::Command = datastore::Command::Get {
        name: qname.into(),
        rrtype,
        rclass: crate::RecordClass::Internet,
        resp: tx_oneshot,
    };

    // here we talk to the datastore to pull the result
    match ds.tx.send(ds_req).await {
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
                return Err(ResponseError::NotFound(()));
            }
        },
        Err(error) => {
            log::error!("Failed to get response from datastore: {:?}", error);
            // return reply_builder(header.id, Rcode::ServFail);
            // TODO: return 500
            return Err(ResponseError::Failed("".to_string()));
        }
    };

    match record {
        None => Err(ResponseError::NotFound(())),
        Some(value) => match serde_json::to_string(&value.typerecords) {
            Ok(value) => Ok(value),
            Err(error) => {
                error!("Failed to serialize response: {error:?}");
                Err(ResponseError::Failed("".to_string()))
            }
        },
    }
}

/// Internal State handler for the datastore object within rocket
struct Datastore {
    tx: Sender<datastore::Command>,
}

pub async fn build(
    goatns_config: ConfigFile,
    tx: Sender<datastore::Command>,
) -> Result<rocket::Rocket<rocket::Ignite>, rocket::Error> {
    let shutdown = rocket::config::Shutdown {
        ctrlc: false,
        ..rocket::config::Shutdown::default()
    };

    let tls = rocket::config::TlsConfig::from_paths(
        goatns_config.api_tls_cert,
        goatns_config.api_tls_key,
    );

    let config = rocket::Config {
        port: goatns_config.api_port,
        shutdown,
        log_level: rocket::log::LogLevel::Normal,
        tls: Some(tls),
        ..rocket::Config::debug_default()
    };
    rocket::custom(&config)
        .manage(Datastore { tx })
        .mount("/", routes![index, status])
        .mount("/api", routes![api_query])
        .ignite()
        .await
}
