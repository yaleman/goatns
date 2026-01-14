use async_trait::async_trait;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Users::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Users::Displayname).string().not_null())
                    .col(ColumnDef::new(Users::Username).string().not_null())
                    .col(ColumnDef::new(Users::Email).string().not_null())
                    .col(ColumnDef::new(Users::Disabled).boolean().not_null())
                    .col(ColumnDef::new(Users::Authref).string())
                    .col(ColumnDef::new(Users::Admin).boolean().not_null())
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(UserTokens::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserTokens::Id)
                            .big_integer()
                            .not_null()
                            .primary_key()
                            .auto_increment(),
                    )
                    .col(ColumnDef::new(UserTokens::Name).string().not_null())
                    .col(ColumnDef::new(UserTokens::Issued).date_time().not_null())
                    .col(ColumnDef::new(UserTokens::Expiry).date_time())
                    .col(ColumnDef::new(UserTokens::Key).string().not_null())
                    .col(ColumnDef::new(UserTokens::Hash).string().not_null())
                    .col(ColumnDef::new(UserTokens::Userid).uuid().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-user_tokens-userid")
                            .from(UserTokens::Table, UserTokens::Userid)
                            .to(Users::Table, Users::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

#[derive(DeriveIden)]
pub(crate) enum Users {
    Table,
    Id,
    Displayname,
    Username,
    Email,
    Disabled,
    Authref,
    Admin,
}

#[derive(DeriveIden)]
pub(crate) enum UserTokens {
    Table,
    Id,
    Name,
    Issued,
    Expiry,
    Key,
    Hash,
    Userid,
}
