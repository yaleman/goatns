use std::str::from_utf8;

use base64::{engine::general_purpose, Engine as _};

use axum::body::{Bytes, Full};
use axum::response::Response;
use axum::routing::{get, post};
// use axum::{Json, Router};
use crate::db::get_all_fzr_by_name;
use crate::enums::{Rcode, RecordType};
use crate::servers::parse_query;
use crate::{Question, HEADER_BYTES};
use axum::extract::{Query, State};
use axum::Router;
use http::{HeaderMap, StatusCode};
use packed_struct::PackedStruct;
use serde::{Deserialize, Serialize};
// use crate::reply::{reply_nxdomain, reply_builder};

use super::GoatState;
// use super::api::ErrorResult;

#[derive(Debug, Serialize)]
pub struct JSONRecord {
    name: String,
    #[serde(rename = "type")]
    qtype: u16,
    ttl: u32,
    data: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JSONResponse {
    status: u32,
    tc: bool,
    rd: bool,
    ra: bool,
    ad: bool,
    cd: bool,
    question: Vec<JSONRecord>,
    answer: Vec<JSONRecord>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct GetQueryString {
    /// Base64-encoded raw question bytes
    dns: Option<String>,
    /// QNAME field
    name: Option<String>,
    /// Query type, defaults to A
    #[serde(alias = "type", default)]
    rrtype: Option<String>,
    /// DO bit - whether the client wants DNSSEC data (either empty or one of 0, false, 1, or true).
    #[serde(alias = "do", default)]
    dnssec: bool,
    /// CD bit - disable validation (either empty or one of 0, false, 1, or true).
    #[serde(default)]
    cd: bool,
}

impl Default for GetQueryString {
    fn default() -> Self {
        Self {
            dns: None,
            name: None,
            rrtype: Some("A".to_string()),
            dnssec: false,
            cd: false,
        }
    }
}

#[derive(Debug)]
enum ResponseType {
    Json,
    Raw,
}

async fn parse_raw_http(bytes: Vec<u8>) -> Result<GetQueryString, String> {
    let mut split_header: [u8; HEADER_BYTES] = [0; HEADER_BYTES];
    split_header.copy_from_slice(&bytes[0..HEADER_BYTES]);
    // unpack the header for great justice
    let header = match crate::Header::unpack(&split_header) {
        Ok(value) => value,
        Err(error) => {
            // can't return a servfail if we can't unpack the header, they're probably doing something bad.
            return Err(format!("Failed to parse header: {:?}", error));
        }
    };
    // log::trace!("Buffer length: {}", len);
    log::trace!("Parsed header: {:?}", header);

    let question = Question::from_packets(&bytes[HEADER_BYTES..]);
    log::debug!("Question: {question:?}");

    let name = match from_utf8(&question.clone().unwrap().qname) {
        Ok(value) => value.to_string(),
        Err(_) => {
            format!("{:?}", question.clone().unwrap().qname)
        }
    };

    Ok(GetQueryString {
        dns: None,
        name: Some(name),
        rrtype: Some(question.unwrap().qtype.to_string()),
        ..Default::default()
    })

    // Err("".to_string())
}

pub async fn handle_get(
    State(state): State<GoatState>,
    headers: HeaderMap,
    Query(query): Query<GetQueryString>,
) -> Response<Full<Bytes>> {
    let state_reader = state.read().await;
    // let datastore = state_reader.tx.clone();
    // let (tx_oneshot, rx_oneshot) = oneshot::channel();

    let response_type: ResponseType = match headers.get("accept") {
        Some(value) => match value.to_str().unwrap_or("") {
            "application/dns-json" => ResponseType::Json,
            "application/dns-message" => ResponseType::Raw,
            _ => ResponseType::Raw,
        },
        None => ResponseType::Raw,
    };
    log::debug!("Response type: {response_type:?}");

    let mut qname: String = "".to_string();
    let mut rrtype: String = "A".to_string();

    if let Some(dns) = query.dns {
        log::debug!("Raw query: {:?}", dns);
        let bytes = general_purpose::STANDARD.decode(dns).unwrap(); // TODO: error handling on parsing
        log::debug!("Packets: {:?}", bytes);

        let query = parse_raw_http(bytes.clone()).await.unwrap();
        qname = query.name.unwrap();
        rrtype = query.rrtype.unwrap();
    } else if query.name.is_some() {
        log::debug!("QueryString query: {query:?}");

        qname = query.name.unwrap();
        rrtype = query.rrtype.unwrap_or("A".to_string());
    }

    log::debug!("getting records...");

    let rrtype_u16: u16 = RecordType::from(rrtype.clone()) as u16;
    let records = match get_all_fzr_by_name(
        &mut state_reader.connpool.begin().await.unwrap(),
        &qname.clone(),
        &rrtype_u16,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            log::error!("Failed to query {qname}/{}: {error:?}", rrtype);
            panic!();
        }
    };

    log::debug!("done getting records...");

    // log::debug!("Query Type: {rrtype:?}");
    // let ds_req: Command = Command::GetRecord {
    //     name: qname.as_bytes().to_owned(),
    //     rrtype: rrtype.clone().into(),
    //     rclass: crate::enums::RecordClass::Internet,
    //     resp: tx_oneshot,
    // };

    // match state_reader.tx.send(ds_req).await {
    //     Ok(_) => log::trace!("Sent a request to the datastore!"),
    //     // TODO: handle errors sending to the DS properly
    //     Err(error) => {
    //         log::error!("Error sending to datastore: {error:?}");
    //         panic!("Error sending to datastore: {error:?}")
    //     },
    // };

    // let record = match rx_oneshot.await {
    //     Ok(value) => match value {
    //         Some(zr) => {
    //             log::debug!("DS Response: {}", zr);
    //             Some(zr)
    //         }
    //         None => {
    //             log::debug!("No response from datastore");
    //             // reply_nxdomain(0);
    //             None
    //         }
    //     },
    //     Err(error) => {
    //         log::error!("Failed to get response from datastore: {:?}", error);
    //         // reply_builder(0, Rcode::ServFail);
    //         None
    //     }
    // };

    match response_type {
        ResponseType::Json => {
            let answer = records
                .iter()
                .map(|rec| JSONRecord {
                    name: rec.name.clone(),
                    qtype: RecordType::from(rec.rrtype.clone()) as u16,
                    ttl: rec.ttl.to_owned(),
                    data: Some(rec.rdata.clone()),
                })
                .collect();

            let reply = JSONResponse {
                answer,
                status: Rcode::NoError as u32,
                tc: false,
                rd: false,
                ra: false,
                ad: false,
                cd: false,
                question: vec![JSONRecord {
                    name: qname,
                    qtype: RecordType::from(rrtype) as u16,
                    ttl: 1,
                    data: None,
                }], //record.unwrap().typerecords
            };

            let response = serde_json::to_string(&reply).unwrap();

            log::debug!("JSON RESPONSE: {response:?}");

            axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-type", "application/dns-json")
                .body(Full::from(response))
                .unwrap()
        }
        ResponseType::Raw => {
            todo!()
        } //     let reply = match reply {
          //         Some(reply) => reply,
          //         None => Reply {
          //         header: Header {
          //             id: 0, // TODO: Check this in ... headers?
          //             qr: crate::enums::PacketType::Answer,
          //             opcode: crate::enums::OpCode::Query,
          //             authoritative: true,
          //             truncated: false,
          //             recursion_desired: false,
          //             recursion_available: false,
          //             z: false,
          //             ad: false,
          //             cd: false,
          //             rcode: crate::enums::Rcode::NoError,
          //             qdcount: 1,
          //             ancount: record.unwrap().typerecords.len() as u16,
          //             nscount: 0,
          //             arcount: 0,
          //         },
          //         question,
          //         answers: vec![],
          //         authorities: vec![],
          //         additional: vec![],
          //     }
          // };

          //     match reply.as_bytes().await {
          //         Ok(value) => axum::response::Response::builder()
          //             .status(StatusCode::OK)
          //             .header("Content-type", "application/dns-message")
          //             .body(Full::from(value))
          //             .unwrap(),
          //         Err(err) => panic!("{err}"),
          //     }
          // }
    }
}

pub async fn handle_post(
    State(state): State<GoatState>,
    // _headers: HeaderMap,
    // Query(query): Query<GetQueryString>,
    body: Bytes,
) -> Response<Full<Bytes>> {
    log::debug!("body {body:?}");

    let state_reader = state.read().await;
    let datastore = state_reader.tx.clone();
    // let (tx_oneshot, rx_oneshot) = oneshot::channel();

    let res = parse_query(datastore, body.len(), &body, false).await;

    match res {
        Ok(reply) => {
            let bytes = match reply.as_bytes().await {
                Ok(value) => value,
                Err(error) => {
                    log::error!("Failed to turn reply into bytes! {error:?}");
                    panic!();
                }
            };
            axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-type", "application/dns-message")
                .body(Full::from(bytes))
                .unwrap()
        }
        Err(err) => panic!("{err:?}"),
    }
}

pub fn new() -> Router<GoatState> {
    Router::new()
        // just zone things
        .route("/", get(handle_get))
        .route("/", post(handle_post))
}
