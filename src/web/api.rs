use super::*;

pub async fn zone_delete(_req: tide::Request<State>) -> tide::Result {
    todo!();
}
pub async fn zone_get(_req: tide::Request<State>) -> tide::Result {
    todo!();
}
pub async fn zone_patch(_req: tide::Request<State>) -> tide::Result {
    todo!();
}
pub async fn zone_post(_req: tide::Request<State>) -> tide::Result {
    todo!();
}

pub async fn record_delete(_req: tide::Request<State>) -> tide::Result {
    todo!();
}
pub async fn record_get(_req: tide::Request<State>) -> tide::Result {
    todo!();
}
pub async fn record_patch(_req: tide::Request<State>) -> tide::Result {
    todo!();
}
pub async fn record_post(_req: tide::Request<State>) -> tide::Result {
    todo!();
}

pub async fn ownership_delete(_req: tide::Request<State>) -> tide::Result {
    todo!();
}
pub async fn ownership_get(_req: tide::Request<State>) -> tide::Result {
    todo!();
}
pub async fn ownership_patch(_req: tide::Request<State>) -> tide::Result {
    todo!();
}
pub async fn ownership_post(_req: tide::Request<State>) -> tide::Result {
    todo!();
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
    api
}
