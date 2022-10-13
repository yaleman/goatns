use diesel::prelude::*;
use serde::Serialize;

use super::schema::zones;

#[derive(Serialize, Queryable)]
pub struct Zone {
    pub id: i32,
    pub name: String,
}

#[derive(Insertable)]
#[diesel(table_name=zones)]
pub struct NewZone<'a> {
    // pub id: u32,
    pub name: &'a str,
}
