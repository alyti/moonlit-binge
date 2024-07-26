use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table_auto(Libraries::Table)
                    .col(integer(Libraries::PlayerConnectionId))
                    .col(string(Libraries::LibraryId))
                    .primary_key(
                        Index::create()
                            .col(Libraries::PlayerConnectionId)
                            .col(Libraries::LibraryId),
                    )
                    .col(string_null(Libraries::ParentId))
                    .col(json_null(Libraries::CachedData))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-libraries-player_connections")
                            .from(Libraries::Table, Libraries::PlayerConnectionId)
                            .to(PlayerConnections::Table, PlayerConnections::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx-libraries-parent-id")
                    .if_not_exists()
                    .table(Libraries::Table)
                    .col(Libraries::PlayerConnectionId)
                    .col(Libraries::ParentId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Libraries::Table).to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-libraries-parent-id")
                    .table(Libraries::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Libraries {
    Table,
    PlayerConnectionId,
    LibraryId,
    ParentId,
    CachedData,
}

#[derive(DeriveIden)]
enum PlayerConnections {
    Table,
    Id,
}
