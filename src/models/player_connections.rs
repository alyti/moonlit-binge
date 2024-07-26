use loco_rs::{
    db,
    model::{self, ModelError, ModelResult},
};
use players::types::{Content, Item, Library};
use sea_orm::{entity::prelude::*, ActiveValue, TransactionTrait};

use crate::{initializers::media_provider::ConnectedMediaProvider, models::_entities::libraries};

pub use super::_entities::player_connections::{self, ActiveModel, Entity, Model};
use super::{contents::ContentWithModel, libraries::LibraryWithModel};

impl ActiveModelBehavior for ActiveModel {
    // extend activemodel below (keep comment for generators)
}
impl ActiveModel {
    // extend activemodel below (keep comment for generators)
    pub fn cache_data(mut self, content: &[&Library]) -> ModelResult<Self> {
        let data = serde_json::to_value(content).map_err(|e| ModelError::Any(e.into()))?;
        self.root_libraries = ActiveValue::Set(Some(data));
        Ok(self)
    }
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

    pub async fn items(
        db: &DatabaseConnection,
        connection_id: i32,
        parent_id: &str,
    ) -> ModelResult<Vec<WrappedItem>> {
        let libraries = super::_entities::libraries::Model::find_by_connection_and_parent_id(
            db,
            connection_id,
            Some(parent_id),
        )
        .await?;
        let contents = super::_entities::contents::Model::by_connection_and_parent_id(
            db,
            connection_id,
            Some(parent_id),
        )
        .await?;
        Ok(libraries
            .into_iter()
            .map(|library| WrappedItem::Library(library.try_into().unwrap()))
            .chain(
                contents
                    .into_iter()
                    .map(|content| WrappedItem::Content(content.try_into().unwrap())),
            )
            .collect())
    }

    pub async fn library_and_items(
        db: &DatabaseConnection,
        user_id: i32,
        connection_id: i32,
        library_id: Option<&str>,
        force_update: bool,
    ) -> ModelResult<(
        Model,
        ConnectedMediaProvider,
        Option<WrappedItem>,
        Vec<WrappedItem>,
    )> {
        // find the player connection, this also makes sure the user owns the connection
        let connection = Self::find_by_user_and_id(db, user_id, connection_id).await?;
        let provider: ConnectedMediaProvider = connection
            .clone()
            .try_into()
            .map_err(|e: loco_rs::Error| ModelError::Any(e.into()))?;

        match library_id {
            Some(library_id) => {
                // resolve parent
                let library: WrappedItem =
                    match super::_entities::libraries::Model::find_by_connection_and_id(
                        db,
                        connection_id,
                        library_id,
                    )
                    .await
                    {
                        Ok(library) if !force_update => library,
                        Ok(_) | Err(ModelError::EntityNotFound) => {
                            let item = provider
                                .item(&library_id)
                                .await
                                .map_err(|e| ModelError::Any(e.into()))?;
                            if let Item::Library(library) = item {
                                super::_entities::libraries::Model::upsert_cache_data(
                                    db,
                                    connection_id,
                                    &[&library],
                                    None,
                                )
                                .await?;
                                super::_entities::libraries::Model::find_by_connection_and_id(
                                    db,
                                    connection_id,
                                    library_id,
                                )
                                .await?
                            } else {
                                return Err(ModelError::EntityNotFound);
                            }
                        }
                        Err(e) => return Err(e),
                    }
                    .try_into()?;

                let items: Vec<WrappedItem> =
                    match Self::items(
                        db,
                        connection_id,
                        library_id,
                    )
                    .await
                    {
                        Ok(items) if !force_update && !items.is_empty() => items,
                        Ok(_) | Err(ModelError::EntityNotFound) => {
                            let start = std::time::Instant::now();
                            let items = provider.items(Some(Library::from_path(library_id)))
                                .await
                                .map_err(|e: loco_rs::Error| ModelError::Any(e.into()))?;
                            tracing::debug!(elapsed = ?start.elapsed(), items = items.len(), "fetched fresh items from provider");
                            let mut libraries = vec![];
                            let mut contents = vec![];
                            for item in &items {
                                match item {
                                    Item::Library(library) => {
                                        libraries.push(library);
                                    }
                                    Item::Content(content) => {
                                        contents.push(content);
                                    }
                                }
                            }
                            super::_entities::libraries::Model::upsert_cache_data(
                                db,
                                connection_id,
                                &libraries,
                                Some(library_id),
                            )
                            .await?;
                            super::_entities::contents::Model::upsert_cache_data(
                                db,
                                connection_id,
                                &contents,
                                Some(library_id),
                            ).await?;
                            Self::items(
                                db,
                                connection_id,
                                library_id,
                            ).await?
                        }
                        Err(e) => return Err(e),
                    }
                    .into_iter()
                    .collect();

                Ok((connection, provider, Some(library), items))
            }
            None => match &connection.root_libraries {
                Some(_) if !force_update => {
                    let libraries: Vec<WrappedItem> = connection.clone().try_into()?;
                    Ok((connection, provider, None, libraries))
                }
                _ => {
                    let items = provider
                        .items(None)
                        .await
                        .map_err(|e| ModelError::Any(e.into()))?;
                    let libraries: Vec<&Library> = items
                        .iter()
                        .filter_map(|item| {
                            if let Item::Library(library) = item {
                                Some(library)
                            } else {
                                None
                            }
                        })
                        .collect();
                    super::_entities::player_connections::Model::upsert_root_libraries(
                        db,
                        connection_id,
                        &libraries,
                    )
                    .await?;
                    let libraries: Vec<WrappedItem> = libraries
                        .into_iter()
                        .map(|library| {
                            WrappedItem::Library(LibraryWithModel {
                                library: library.clone(),
                                parent_id: None,
                            })
                        })
                        .collect();
                    Ok((connection, provider, None, libraries))
                }
            },
        }
    }

    pub async fn upsert_root_libraries(
        db: &DatabaseConnection,
        connection_id: i32,
        libraries: &[&Library],
    ) -> ModelResult<()> {
        ActiveModel {
            id: ActiveValue::Set(connection_id),
            ..Default::default()
        }
        .cache_data(libraries)?
        .update(db)
        .await?;

        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum WrappedItem {
    Content(ContentWithModel),
    Library(LibraryWithModel),
}

impl TryFrom<super::_entities::contents::Model> for WrappedItem {
    type Error = ModelError;

    fn try_from(content: super::_entities::contents::Model) -> Result<Self, Self::Error> {
        Ok(WrappedItem::Content(content.try_into()?))
    }
}

impl TryFrom<super::_entities::libraries::Model> for WrappedItem {
    type Error = ModelError;

    fn try_from(library: super::_entities::libraries::Model) -> Result<Self, Self::Error> {
        Ok(WrappedItem::Library(library.try_into()?))
    }
}

impl TryFrom<super::_entities::player_connections::Model> for Vec<WrappedItem> {
    type Error = ModelError;

    fn try_from(value: super::_entities::player_connections::Model) -> Result<Self, Self::Error> {
        let libraries: Vec<Library> = serde_json::from_value(
            value
                .root_libraries
                .clone()
                .ok_or(ModelError::EntityNotFound)?,
        )
        .map_err(|e| ModelError::Any(e.into()))?;

        Ok(libraries
            .into_iter()
            .map(|library| {
                WrappedItem::Library(LibraryWithModel {
                    library,
                    parent_id: None,
                })
            })
            .collect())
    }
}
