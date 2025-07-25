use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use base64::{engine::general_purpose, Engine as _};
use packed_struct::PackedStruct;
use serde::{Deserialize, Serialize};
use std::str::from_utf8;
use tracing::{debug, error, trace};

use crate::db::get_all_fzr_by_name;
use crate::enums::{Rcode, RecordClass, RecordType};
use crate::reply::Reply;
use crate::resourcerecord::InternalResourceRecord;
use crate::servers::{parse_query, QueryProtocol};
use crate::web::GoatState;
use crate::{Header, Question, HEADER_BYTES};

// TODO: when responding to requests and have an empty response, if we can find the root zone, include the SOA minimum

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
    /// id of the request, should normally be 0 for caching, but sometimes...
    #[serde(default)]
    id: u16,
}

impl Default for GetQueryString {
    fn default() -> Self {
        Self {
            dns: None,
            name: None,
            rrtype: Some("A".to_string()),
            dnssec: false,
            cd: false,
            id: 0,
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
            return Err(format!("Failed to parse header: {error:?}"));
        }
    };

    let question = Question::from_packets(&bytes[HEADER_BYTES..])?;
    debug!("Question: {question:?}");

    let name = match from_utf8(&question.qname) {
        Ok(value) => value.to_string(),
        Err(_) => {
            format!("{:?}", question.qname)
        }
    };

    Ok(GetQueryString {
        dns: None,
        name: Some(name),
        rrtype: Some(question.qtype.to_string()),
        id: header.id,
        cd: header.cd,
        ..Default::default()
    })
}

fn get_response_type_from_headers(headers: &HeaderMap) -> ResponseType {
    match headers.get("accept") {
        Some(value) => match value.to_str().unwrap_or("") {
            "application/dns-json" => ResponseType::Json,
            "application/dns-message" => ResponseType::Raw,
            _ => ResponseType::Invalid,
        },
        None => ResponseType::Invalid,
    }
}

pub(crate) fn response_406() -> Response {
    (
        StatusCode::NOT_ACCEPTABLE,
        [(axum::http::header::CACHE_CONTROL, "max-age=3600")],
        "",
    )
        .into_response()
}
pub(crate) fn response_500() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        [(axum::http::header::CACHE_CONTROL, "max-age=1")],
        "",
    )
        .into_response()
}

pub async fn handle_get(
    State(state): State<GoatState>,
    headers: HeaderMap,
    Query(query): Query<GetQueryString>,
) -> Result<Response, Response> {
    // TODO: accept header filtering probably should be a middleware since it applies to the whole /doh route but those things are annoying as heck
    let response_type: ResponseType = get_response_type_from_headers(&headers);
    if let ResponseType::Invalid = response_type {
        return Err(response_406());
    }

    let mut qname: String = query.name.unwrap_or(String::new());
    let mut rrtype: String = query.rrtype.unwrap_or("A".to_string());
    let mut id: u16 = 0;

    if let Some(dns) = query.dns {
        let bytes = match general_purpose::STANDARD.decode(dns) {
            Ok(val) => val,
            Err(err) => {
                debug!("Failed to parse DoH GET RAW: {err:?}");
                return Err(response_500()); // TODO: this could probably be a SERVFAIL?
            }
        };

        let query = parse_raw_http(bytes).await.map_err(|err| {
            error!("Failed to parse DoH GET RAW: {err:?}");
            response_500() // TODO: should this be a SERVFAIL?
        })?;
        if let Some(name) = query.name {
            qname = name;
        }
        rrtype = query.rrtype.unwrap_or("A".to_string());
        id = query.id;
    }

    let mut read_txn = state.read().await.connpool.begin().await.map_err(|err| {
        error!("Failed to get DB connection: {err:?}");
        response_500()
    })?;

    let records = match get_all_fzr_by_name(
        &mut read_txn,
        &qname,
        RecordType::from(rrtype.as_str()) as u16,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            error!("Failed to query {qname}/{}: {error:?}", rrtype);
            return Err(response_500()); // TODO: This should probably be a SERVFAIL
        }
    };

    trace!("Completed record request...");

    let ttl = records.iter().map(|r| r.ttl).min();
    let ttl = match ttl {
        Some(val) => val.to_owned(),
        None => {
            trace!("Failed to get minimum TTL from query, using 1");
            1
        }
    };

    trace!("Returned records: {records:?}");

    match response_type {
        ResponseType::Invalid => Err(response_500()),
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

            let response = serde_json::to_string(&reply).map_err(|err| {
                error!("Failed to parse DoH GET request into JSON: {err:?}");
                response_500()
            })?;

            let response_builder = axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-type", "application/dns-json")
                .header("Cache-Control", format!("max-age={ttl}"));
            // TODO: add handler for DNSSEC responses
            response_builder.body(Body::from(response)).map_err(|err| {
                error!("Failed to turn DoH GET request into JSON: {err:?}");
                response_500()
            })
        }
        ResponseType::Raw => {
            let answers: Vec<InternalResourceRecord> = records
                .iter()
                .filter_map(|r| InternalResourceRecord::try_from(r.to_owned()).ok())
                .collect();

            let reply = Reply {
                header: Header {
                    id,
                    qr: crate::enums::PacketType::Answer,
                    opcode: crate::enums::OpCode::Query,
                    authoritative: true, // we're always authoritative
                    truncated: false,
                    recursion_desired: false,
                    recursion_available: false,
                    z: false,
                    ad: false, // TODO: ad handling
                    cd: false, // TODO: cd handling
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
                answers,
                authorities: vec![], // TODO: authorities in handle_get raw response
                additional: vec![],  // TODO: additional fields in handle_get raw response
            };

            match reply.as_bytes().await {
                Ok(value) => axum::response::Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-type", "application/dns-message")
                    .header("Cache-Control", format!("max-age={ttl}"))
                    .body(Body::from(value))
                    .map_err(|err| {
                        error!("Failed to turn DoH GET request into bytes: {err:?}");
                        response_500()
                    }),
                Err(err) => {
                    error!("Failed to turn DoH GET request into bytes: {err:?}");
                    Err(response_500())
                }
            }
        }
    }
}

pub async fn handle_post(
    State(state): State<GoatState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Response> {
    // TODO: accept header filtering probably should be a middleware since it applies to the whole /doh route but those things are annoying as heck
    let response_type: ResponseType = get_response_type_from_headers(&headers);
    if let ResponseType::Invalid = response_type {
        return Err(response_406());
    };
    if let ResponseType::Json = response_type {
        // TODO: maybe support JSON responses to DoH POST requests
        return Err(response_406());
    };

    let state_reader = state.read().await;
    let datastore = state_reader.tx.clone();

    let res = parse_query(
        datastore,
        body.len(),
        &body,
        state_reader.config.capture_packets,
        QueryProtocol::DoH,
    )
    .await;

    match res {
        Ok(mut reply) => {
            let bytes = match reply.as_bytes().await {
                Ok(value) => {
                    // we need to truncate the response
                    if value.len() > 65535 {
                        reply.header.truncated = true;
                        let mut bytes: Vec<u8> = reply.as_bytes().await.map_err(|err| {
                            error!("Failed to turn DoH POST response into bytes: {err:?}");
                            response_500()
                        })?;
                        bytes.resize(65535, 0);
                        bytes
                    } else {
                        value
                    }
                }
                Err(error) => {
                    error!("Failed to turn DoH POST response into bytes! {error:?}");
                    return Err(response_500());
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
                .body(Body::from(bytes))
                .map_err(|err| {
                    error!("Failed to turn DoH POST response into bytes: {err:?}");
                    response_500()
                })
        }
        Err(err) => {
            error!("Failed to parse DoH POST query: {err:?}");
            Err(response_500())
        }
    }
}

pub fn new() -> Router<GoatState> {
    Router::new()
        // just zone things
        .route("/", get(handle_get))
        .route("/", post(handle_post))
}
