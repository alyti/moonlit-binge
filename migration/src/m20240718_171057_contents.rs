use extension::postgres::Type;
use sea_orm::{DbBackend, DeriveActiveEnum, EnumIter, Iterable, Schema};
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let schema = Schema::new(DbBackend::Postgres);
        manager
            .create_type(schema.create_enum_from_active_enum::<Status>())
            .await?;
        manager
            .create_table(
                table_auto(Contents::Table)
                    .col(integer(Contents::PlayerConnectionId))
                    .col(string(Contents::ContentId))
                    .col(string_null(Contents::ParentId))
                    .col(json_null(Contents::CachedData))
                    .primary_key(
                        Index::create()
                            .col(Contents::PlayerConnectionId)
                            .col(Contents::ContentId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-contents-player_connections")
                            .from(Contents::Table, Contents::PlayerConnectionId)
                            .to(PlayerConnections::Table, PlayerConnections::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .col(
                        ColumnDef::new(Contents::Status)
                            .custom(Alias::new("status_name"))
                            .null()
                            .to_owned(),
                    )
                    .col(
                        timestamp(Contents::StatusLastUpdatedAt).default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx-contents-parent-id")
                    .if_not_exists()
                    .table(Contents::Table)
                    .col(Contents::PlayerConnectionId)
                    .col(Contents::ParentId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx-player-connection-user-id")
                    .if_not_exists()
                    .table(PlayerConnections::Table)
                    .col(PlayerConnections::UserId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_type(
                Type::drop()
                    .if_exists()
                    .name(StatusEnum)
                    .restrict()
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-contents-parent-id")
                    .table(Contents::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx-player-connection-user-id")
                    .table(PlayerConnections::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(Contents::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Contents {
    Table,
    PlayerConnectionId,
    ContentId,
    ParentId,
    CachedData,
    Status,
    StatusLastUpdatedAt,
}

#[derive(DeriveIden)]
enum PlayerConnections {
    Table,
    Id,
    UserId,
}

#[derive(EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "status_name")]
pub enum Status {
    #[sea_orm(string_value = "in-progress")]
    InProgress,
    #[sea_orm(string_value = "success")]
    Success,
    #[sea_orm(string_value = "error")]
    Error,
}
