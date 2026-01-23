mod m_20260115_users_table;
mod m_20260115_zones_tables;
mod m_20260201_users_id_uuid;
mod m_20260203_zones_ids_uuid;

use async_trait::async_trait;
use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m_20260115_users_table::Migration),
            Box::new(m_20260115_zones_tables::Migration),
            Box::new(m_20260201_users_id_uuid::Migration),
            Box::new(m_20260203_zones_ids_uuid::Migration),
        ]
    }
}
