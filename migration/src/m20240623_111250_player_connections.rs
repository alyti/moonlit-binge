use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table_auto(PlayerConnections::Table)
                    .col(pk_auto(PlayerConnections::Id))
                    .col(string(PlayerConnections::MediaProviderId))
                    .col(integer(PlayerConnections::UserId))
                    .col(json_binary_null(PlayerConnections::Identity))
                    .col(json_null(PlayerConnections::Status))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-player_connections-users")
                            .from(PlayerConnections::Table, PlayerConnections::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PlayerConnections::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PlayerConnections {
    Table,
    Id,
    MediaProviderId,
    UserId,
    Identity,
    Status,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
