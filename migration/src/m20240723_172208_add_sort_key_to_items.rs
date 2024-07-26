use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Contents::Table)
                    .add_column_if_not_exists(big_unsigned(Contents::SortKey).default(0u64))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Libraries::Table)
                    .add_column_if_not_exists(big_unsigned(Libraries::SortKey).default(0u64))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Contents::Table)
                    .drop_column(Contents::SortKey)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Libraries::Table)
                    .drop_column(Libraries::SortKey)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Contents {
    Table,
    SortKey,
}

#[derive(DeriveIden)]
enum Libraries {
    Table,
    SortKey,
}
