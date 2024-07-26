use super::_entities::libraries::{self, ActiveModel, Model};
use loco_rs::model::{self, ModelError, ModelResult};
use migration::OnConflict;
use players::types::Library;
use sea_orm::{entity::prelude::*, ActiveValue, QueryTrait, TransactionTrait};
use sqlx_postgres::Postgres;

impl ActiveModelBehavior for ActiveModel {
    // extend activemodel below (keep comment for generators)
}

impl ActiveModel {
    // extend activemodel below (keep comment for generators)
    pub fn cache_data(mut self, content: &Library) -> ModelResult<Self> {
        let data = serde_json::to_value(content).map_err(|e| ModelError::Any(e.into()))?;
        self.cached_data = ActiveValue::Set(Some(data));
        Ok(self)
    }
    pub fn parent_id(mut self, parent_id: Option<&str>) -> Self {
        if let Some(parent_id) = parent_id {
            self.parent_id = ActiveValue::Set(Some(parent_id.to_string()));
        }
        self
    }
}

impl Model {
    pub async fn upsert_cache_data(
        db: &DatabaseConnection,
        connection_id: i32,
        libraries: &[&Library],
        true_parent_id: Option<&str>,
    ) -> ModelResult<()> {
        if libraries.is_empty() {
            return Ok(());
        }

        let txn = db.begin().await?;

        let models = libraries
            .iter()
            .map(|library| {
                ActiveModel {
                    player_connection_id: ActiveValue::Set(connection_id),
                    library_id: ActiveValue::Set(library.id.clone()),
                    ..Default::default()
                }
                .parent_id(true_parent_id.or(library.parent_id.as_deref()).as_deref())
                .cache_data(library)
            })
            .collect::<ModelResult<Vec<_>>>()?;

        libraries::Entity::insert_many(models)
            .on_conflict(
                OnConflict::columns([
                    libraries::Column::PlayerConnectionId,
                    libraries::Column::LibraryId,
                ])
                .update_column(libraries::Column::CachedData)
                .to_owned(),
            )
            .exec(&txn)
            .await?;

        txn.commit().await?;

        Ok(())
    }

    pub async fn find_by_connection_and_parent_id(
        db: &DatabaseConnection,
        connection_id: i32,
        parent_id: Option<&str>,
    ) -> ModelResult<Vec<Model>> {
        let libraries = libraries::Entity::find()
            .filter(
                model::query::condition()
                    .eq(libraries::Column::PlayerConnectionId, connection_id)
                    .eq(libraries::Column::ParentId, parent_id)
                    .build(),
            )
            .all(db)
            .await?;
        Ok(libraries)
    }

    pub async fn find_by_connection_and_id(
        db: &DatabaseConnection,
        connection_id: i32,
        library_id: &str,
    ) -> ModelResult<Model> {
        let library = libraries::Entity::find()
            .filter(
                model::query::condition()
                    .eq(libraries::Column::PlayerConnectionId, connection_id)
                    .eq(libraries::Column::LibraryId, library_id)
                    .build(),
            )
            .one(db)
            .await?;
        library.ok_or_else(|| ModelError::EntityNotFound)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct LibraryWithModel {
    // Only library for now but can be extended to include more fields from model later.
    #[serde(flatten)]
    pub library: Library,
    pub parent_id: Option<String>,
}

impl TryFrom<Model> for LibraryWithModel {
    type Error = ModelError;

    fn try_from(value: Model) -> Result<Self, Self::Error> {
        let library = serde_json::from_value(
            value
                .cached_data
                .clone()
                .ok_or(ModelError::EntityNotFound)?,
        )
        .map_err(|e| ModelError::Any(e.into()))?;
        Ok(Self {
            library,
            parent_id: value.parent_id,
        })
    }
}
