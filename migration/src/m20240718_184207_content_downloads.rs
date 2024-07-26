use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table_auto(ContentDownloads::Table)
                    .col(
                        uuid(ContentDownloads::Id)
                            .extra("DEFAULT gen_random_uuid()")
                            .primary_key(),
                    )
                    .col(integer(ContentDownloads::PlayerConnectionId))
                    .col(string(ContentDownloads::ContentId))
                    .col(json_null(ContentDownloads::StatusInfo))
                    .col(
                        ColumnDef::new(ContentDownloads::Status)
                            .custom(Alias::new("status_name"))
                            .not_null()
                            .to_owned(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-content_downloads-contents")
                            .from(
                                ContentDownloads::Table,
                                (
                                    ContentDownloads::PlayerConnectionId,
                                    ContentDownloads::ContentId,
                                ),
                            )
                            .to(
                                Contents::Table,
                                (Contents::PlayerConnectionId, Contents::ContentId),
                            )
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ContentDownloads::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ContentDownloads {
    Table,
    Id,
    PlayerConnectionId,
    ContentId,
    StatusInfo,
    Status,
}

#[derive(DeriveIden)]
enum Contents {
    Table,
    PlayerConnectionId,
    ContentId,
}
