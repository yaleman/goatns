//! `SeaORM` Entity. Generated by sea-orm-codegen 0.12.15

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "records")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: u64,
    pub zoneid: i32,
    pub name: Option<String>,
    pub ttl: Option<i32>,
    pub rrtype: i32,
    pub rclass: i32,
    pub rdata: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::zones::Entity",
        from = "Column::Zoneid",
        to = "super::zones::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Zones,
}

impl Related<super::zones::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Zones.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
