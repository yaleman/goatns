use super::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Deserialize, Serialize)]
#[sea_orm(table_name = "ownership")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub zoneid: Uuid,
    pub userid: Uuid,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::Userid",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Users,
    #[sea_orm(
        belongs_to = "super::zones::Entity",
        from = "Column::Zoneid",
        to = "super::zones::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Zones,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
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

impl Entity {
    pub(crate) async fn find_by_user_and_zone<C>(
        db: &C,
        user_id: Uuid,
        zone_id: Uuid,
    ) -> Result<Option<Model>, DbErr>
    where
        C: ConnectionTrait,
    {
        Self::find()
            .filter(Column::Userid.eq(user_id).and(Column::Zoneid.eq(zone_id)))
            .one(db)
            .await
    }
}
