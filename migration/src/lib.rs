#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;

mod m20220101_000001_users;
mod m20240623_111250_player_connections;

mod m20240707_110722_add_preferences_to_player_connection;
mod m20240709_154725_add_preferred_profile_to_player_connection;
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_users::Migration),
            Box::new(m20240623_111250_player_connections::Migration),
            Box::new(m20240707_110722_add_preferences_to_player_connection::Migration),
            Box::new(m20240709_154725_add_preferred_profile_to_player_connection::Migration),
        ]
    }
}