use extension::postgres::Type;
use sea_orm::{ActiveEnum, DeriveActiveEnum, EnumIter};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        use sea_orm::{DbBackend, Schema};
        let schema = Schema::new(DbBackend::Postgres);

        manager.create_type(schema.create_enum_from_active_enum::<AuthProvider>()).await?;

        manager
            .create_table(
                Table::create()
                    .table(User::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(User::Id).integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(User::Username).string().not_null().unique_key())
                    .col(ColumnDef::new(User::DisplayName).string().not_null().default(""))
                    .col(ColumnDef::new(User::ShowDisplayName).boolean().not_null().default(false))
                    .col(ColumnDef::new(User::Bio).string().not_null().default(""))
                    .col(ColumnDef::new(User::CreatedAt).timestamp().not_null().default(Expr::current_timestamp()))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(ExternalUser::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(ExternalUser::Id).integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(ExternalUser::UserId).integer().not_null())
                    .col(ColumnDef::new(ExternalUser::ExternalId).string().not_null())
                    .col(
                        ColumnDef::new(ExternalUser::AuthProvider)
                            .enumeration(AuthProvider::name(), AuthProvider::iden_values())
                            .not_null(),
                    )
                    .col(ColumnDef::new(ExternalUser::Data).json_binary().not_null())
                    .col(ColumnDef::new(ExternalUser::AccessToken).string().not_null())
                    .col(
                        ColumnDef::new(ExternalUser::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(ForeignKey::create().from(ExternalUser::Table, ExternalUser::UserId).to(User::Table, User::Id))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(ExternalUser::Table)
                    .col(ExternalUser::ExternalId)
                    .col(ExternalUser::AuthProvider)
                    .unique()
                    .take(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(AuthenticatedClient::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(AuthenticatedClient::Id).integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(AuthenticatedClient::UserId).integer().not_null())
                    .col(ColumnDef::new(AuthenticatedClient::Token).uuid().not_null())
                    .col(
                        ColumnDef::new(AuthenticatedClient::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(AuthenticatedClient::Table, AuthenticatedClient::UserId)
                            .to(User::Table, User::Id),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(AuthenticatedClient::Table).to_owned()).await?;
        manager.drop_table(Table::drop().table(ExternalUser::Table).to_owned()).await?;
        manager.drop_table(Table::drop().table(User::Table).to_owned()).await?;
        manager.drop_type(Type::drop().name(AuthProvider::name()).to_owned()).await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
    Username,
    DisplayName,
    ShowDisplayName,
    Bio,
    CreatedAt,
}

#[derive(DeriveIden)]
enum ExternalUser {
    Table,
    Id,
    UserId,
    ExternalId,
    AuthProvider,
    Data,
    AccessToken,
    CreatedAt,
}

#[derive(DeriveIden)]
enum AuthenticatedClient {
    Table,
    Id,
    UserId,
    Token,
    CreatedAt,
}

#[derive(EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "auth_provider")]
pub enum AuthProvider {
    #[sea_orm(string_value = "discord")]
    Discord,
    #[sea_orm(string_value = "facebook")]
    Facebook,
    #[sea_orm(string_value = "github")]
    Github,
    #[sea_orm(string_value = "google")]
    Google,
    #[sea_orm(string_value = "kicrosoft")]
    Microsoft,
    #[sea_orm(string_value = "spotify")]
    Spotify,
    #[sea_orm(string_value = "twitter")]
    Twitter,
}
