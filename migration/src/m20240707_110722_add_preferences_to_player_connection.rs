use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;


#[derive(DeriveIden)]
enum PlayerConnections {
    Table,
    Preferences,    
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(PlayerConnections::Table)
                    .add_column_if_not_exists(json_null(PlayerConnections::Preferences))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(PlayerConnections::Table)
                    .drop_column(PlayerConnections::Preferences)
                    .to_owned(),
            )
            .await
    }
}

