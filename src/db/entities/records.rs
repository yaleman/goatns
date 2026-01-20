use super::prelude::*;

use crate::web::api::records::RecordForm;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Deserialize, Serialize, ToSchema)]
#[sea_orm(table_name = "records")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub zoneid: Uuid,
    pub name: String,
    pub ttl: Option<u32>,
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

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, _insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let mut me = self;
        if me.id.is_not_set() {
            me.id.set_if_not_equals(Uuid::now_v7());
        }
        Ok(me)
    }
}

impl From<RecordForm> for ActiveModel {
    fn from(form: RecordForm) -> Self {
        let mut am = ActiveModel {
            id: NotSet,
            zoneid: Set(form.zoneid),
            name: Set(form.name),
            ttl: Set(form.ttl),
            rrtype: Set(form.rrtype as u16),
            rclass: Set(form.rclass as u16),
            rdata: Set(form.rdata),
        };
        if let Some(id) = form.id {
            am.id = Set(id);
        }
        am
    }
}
