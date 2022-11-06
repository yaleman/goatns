use super::*;
use tide::http::mime;
use serde::Serialize;

#[derive(Serialize)]
struct NotImplemented{
    response: String
}
impl Default for NotImplemented {
    fn default() -> Self {
        Self { response: "This endpoint is not yet implemented".to_string() }
    }
}

pub async fn zone_delete(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}
pub async fn zone_get(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}
pub async fn zone_patch(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}
pub async fn zone_post(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}

pub async fn record_delete(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}
pub async fn record_get(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}
pub async fn record_patch(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}
pub async fn record_post(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}

pub async fn ownership_delete(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}
pub async fn ownership_get(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}
pub async fn ownership_patch(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}
pub async fn ownership_post(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}

pub async fn user_delete(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}
pub async fn user_get(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}
pub async fn user_patch(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}
pub async fn user_post(_req: tide::Request<State>) -> tide::Result {
    let response = serde_json::to_string(&NotImplemented::default()).unwrap();
    tide_result_json!(response, 403)
}

pub fn new(tx: Sender<datastore::Command>) -> tide::Server<State> {
    let mut api = tide::with_state(State { tx });
    api.at("/zone/:id")
        .get(zone_get)
        .post(zone_post)
        .delete(zone_delete)
        .patch(zone_patch);
    api.at("/record/:id")
        .get(record_get)
        .post(record_post)
        .delete(record_delete)
        .patch(record_patch);
    api.at("/ownership/:id")
        .get(ownership_get)
        .post(ownership_post)
        .delete(ownership_delete)
        .patch(ownership_patch);
    api.at("/user/:id")
        .get(user_get)
        .post(user_post)
        .delete(user_delete)
        .patch(user_patch);
    api
}
