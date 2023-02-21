use std::str::from_utf8;

use base64::{engine::general_purpose, Engine as _};

use axum::body::{Bytes, Full};
use axum::response::Response;
use axum::routing::{get, post};
// use axum::{Json, Router};
use crate::db::get_all_fzr_by_name;
use crate::enums::{Rcode, RecordClass, RecordType};
use crate::reply::Reply;
use crate::servers::parse_query;
use crate::{Header, Question, HEADER_BYTES};
use axum::extract::{Query, State};
use axum::Router;
use http::{HeaderMap, StatusCode};
use packed_struct::PackedStruct;
use serde::{Deserialize, Serialize};
// use crate::reply::{reply_nxdomain, reply_builder};

use super::GoatState;
// use super::api::ErrorResult;

#[derive(Debug, Serialize)]
pub struct JSONQuestion {
    name: String,
    #[serde(rename = "type")]
    qtype: u16,
}

#[derive(Debug, Serialize)]
pub struct JSONRecord {
    name: String,
    #[serde(rename = "type")]
    qtype: u16,
    #[serde(rename = "TTL")]
    ttl: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct JSONResponse {
    status: u32,
    /// Response was truncated
    #[serde(rename = "tc")]
    truncated: bool,
    /// Recursive desired was set
    #[serde(rename = "rd")]
    recursive_desired: bool,
    #[serde(rename = "ra")]
    /// If true, it means the Recursion Available bit was set.
    recursion_available: bool,
    ///If true, it means that every record in the answer was verified with DNSSEC.
    ad: bool,
    #[serde(rename = "cd")]
    /// If true, the client asked to disable DNSSEC validation.
    client_dnssec_disable: bool,
    #[serde(rename = "Question")]
    question: Vec<JSONQuestion>,
    #[serde(rename = "Answer")]
    answer: Vec<JSONRecord>,
    #[serde(rename = "Comment", skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
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
    Invalid,
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

fn get_response_type_from_headers(headers: HeaderMap) -> ResponseType {
    match headers.get("accept") {
        Some(value) => match value.to_str().unwrap_or("") {
            "application/dns-json" => ResponseType::Json,
            "application/dns-message" => ResponseType::Raw,
            _ => ResponseType::Invalid,
        },
        None => ResponseType::Invalid,
    }
}

fn response_406() -> Response<Full<Bytes>> {
    axum::response::Response::builder()
        .status(StatusCode::from_u16(406).unwrap())
        // .header("Content-type", "application/dns-json")
        .header("Cache-Control", "max-age=3600")
        .body(Full::new(Bytes::new()))
        .unwrap()
}

pub async fn handle_get(
    State(state): State<GoatState>,
    headers: HeaderMap,
    Query(query): Query<GetQueryString>,
) -> Response<Full<Bytes>> {
    let response_type: ResponseType = get_response_type_from_headers(headers);

    if let ResponseType::Invalid = response_type {
        return response_406();
    }

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

    let records = match get_all_fzr_by_name(
        &mut state.read().await.connpool.begin().await.unwrap(),
        &qname.clone(),
        RecordType::from(rrtype.clone()) as u16,
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

    let ttl = records.iter().map(|r| r.ttl).min();
    let ttl = match ttl {
        Some(val) => val.to_owned(),
        None => {
            log::trace!("Failed to get minimum TTL from query, using 1");
            1
        }
    };

    log::debug!("Returned records: {records:?}");

    match response_type {
        ResponseType::Invalid => {
            todo!("How did you get here?");
        }
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
                truncated: false,
                recursive_desired: false,
                recursion_available: false,
                ad: false,
                client_dnssec_disable: false,
                question: vec![JSONQuestion {
                    name: qname,
                    qtype: RecordType::from(rrtype) as u16,
                }],
                ..Default::default()
            };

            let response = serde_json::to_string(&reply).unwrap();

            let response_builder = axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-type", "application/dns-json")
                .header("Cache-Control", format!("max-age={ttl}"));
            // TODO: add handler for DNSSEC responses
            response_builder.body(Full::from(response)).unwrap()
        }
        ResponseType::Raw => {
            let reply = Reply {
                header: Header {
                    id: 0, // TODO: Check this in ... headers?
                    qr: crate::enums::PacketType::Answer,
                    opcode: crate::enums::OpCode::Query,
                    authoritative: true,
                    truncated: false,
                    recursion_desired: false,
                    recursion_available: false,
                    z: false,
                    ad: false,
                    cd: false,
                    rcode: crate::enums::Rcode::NoError,
                    qdcount: 1,
                    ancount: records.len() as u16,
                    nscount: 0,
                    arcount: 0,
                },
                question: Some(Question {
                    qname: qname.into(),
                    qtype: RecordType::from(rrtype),
                    qclass: RecordClass::Internet,
                }),
                answers: vec![], // TODO
                authorities: vec![],
                additional: vec![],
            };

            match reply.as_bytes().await {
                Ok(value) => axum::response::Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-type", "application/dns-message")
                    .header("Cache-Control", format!("max-age={ttl}"))
                    .body(Full::from(value))
                    .unwrap(),
                Err(err) => panic!("{err}"),
            }
        }
    }
}

pub async fn handle_post(
    State(state): State<GoatState>,
    headers: HeaderMap,
    // Query(query): Query<GetQueryString>,
    body: Bytes,
) -> Response<Full<Bytes>> {
    log::debug!("body {body:?}");

    let response_type: ResponseType = get_response_type_from_headers(headers);

    if let ResponseType::Invalid = response_type {
        return response_406();
    };

    let state_reader = state.read().await;
    let datastore = state_reader.tx.clone();

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

            let ttl = reply.answers.iter().map(|a| a.ttl()).min();
            let ttl = match ttl {
                Some(ttl) => ttl.to_owned(),
                None => 1,
            };
            axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-type", "application/dns-message")
                .header("Cache-Control", format!("max-age={ttl}"))
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
