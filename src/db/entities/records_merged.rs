//! `SeaORM` Entity based on the records_merged view

use sea_orm::QueryFilter;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use crate::enums::{RecordClass, RecordType};
use crate::error::GoatNsError;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Deserialize, Serialize)]
#[sea_orm(table_name = "records_merged")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub record_id: Uuid,
    pub zoneid: Uuid,
    pub name: String,
    pub ttl: u32,
    pub rrtype: u16,
    pub rclass: u16,
    pub rdata: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::zones::Entity",
        from = "Column::Zoneid",
        to = "super::zones::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Zones,
}

impl Related<super::zones::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Zones.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl Entity {
    pub(crate) async fn get_records(
        pool: &DatabaseConnection,
        name: &str,
        rrtype: RecordType,
        rclass: RecordClass,
        normalize_ttls: bool,
    ) -> Result<Vec<Model>, GoatNsError> {
        let query = Self::find()
            .filter(Column::Name.eq(name))
            .filter(Column::Rrtype.eq(rrtype as u16))
            .filter(Column::Rclass.eq(rclass as u16));

        let mut records = query.all(pool).await?;
        if normalize_ttls {
            let min_ttl = records.iter().map(|rec| rec.ttl).min().unwrap_or(0);
            for rec in records.iter_mut() {
                rec.ttl = min_ttl;
            }
        }

        Ok(records)
    }
}
