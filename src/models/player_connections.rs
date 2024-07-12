use loco_rs::model::{self, ModelError, ModelResult};
use sea_orm::entity::prelude::*;

pub use super::_entities::player_connections::{self, ActiveModel, Entity, Model};

impl ActiveModelBehavior for ActiveModel {
    // extend activemodel below (keep comment for generators)
}

impl super::_entities::player_connections::Model {
    /// finds a player connection by the provided user and id
    ///
    /// # Errors
    ///
    /// When could not find player connection by the given ids or DB query error
    pub async fn find_by_user_and_id(
        db: &DatabaseConnection,
        user: i32,
        id: i32,
    ) -> ModelResult<Self> {
        let user = player_connections::Entity::find()
            .filter(
                model::query::condition()
                    .eq(player_connections::Column::UserId, user)
                    .eq(player_connections::Column::Id, id)
                    .build(),
            )
            .one(db)
            .await?;
        user.ok_or_else(|| ModelError::EntityNotFound)
    }
}
