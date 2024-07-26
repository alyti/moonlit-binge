use super::_entities::{
    content_downloads,
    contents::{self, ActiveModel, Model},
    sea_orm_active_enums::StatusName,
};
use futures_util::TryFutureExt;
use loco_rs::model::{self, ModelError, ModelResult};
use migration::OnConflict;
use players::types::Content;
use sea_orm::{entity::prelude::*, ActiveValue, IntoActiveModel, QueryOrder, TransactionTrait};

impl ActiveModelBehavior for ActiveModel {
    // extend activemodel below (keep comment for generators)
}

impl ActiveModel {
    // extend activemodel below (keep comment for generators)
    pub fn cache_data(mut self, content: &Content) -> ModelResult<Self> {
        let data = serde_json::to_value(content).map_err(|e| ModelError::Any(e.into()))?;
        self.cached_data = ActiveValue::Set(Some(data));
        Ok(self)
    }

    pub fn status(mut self, status: Option<StatusName>) -> Self {
        self.status = ActiveValue::Set(status);
        self.status_last_updated_at = ActiveValue::Set(chrono::Utc::now().naive_utc());
        self
    }

    pub fn parent_id(mut self, parent_id: Option<&str>) -> Self {
        if let Some(parent_id) = parent_id {
            self.parent_id = ActiveValue::Set(Some(parent_id.to_string()));
        }
        self
    }
}

impl super::_entities::contents::Model {
    pub async fn upsert_cache_data(
        db: &DatabaseConnection,
        connection_id: i32,
        contents: &[&Content],
        true_parent_id: Option<&str>,
    ) -> ModelResult<()> {
        if contents.is_empty() {
            return Ok(());
        }

        let txn = db.begin().await?;

        let models = contents
            .iter()
            .map(|content| {
                ActiveModel {
                    player_connection_id: ActiveValue::Set(connection_id),
                    content_id: ActiveValue::Set(content.id.clone()),
                    sort_key: ActiveValue::Set(content.sort_key() as i64),
                    ..Default::default()
                }
                .parent_id(true_parent_id.or(content.parent_id.as_deref()).as_deref())
                .cache_data(content)
            })
            .collect::<ModelResult<Vec<_>>>()?;

        contents::Entity::insert_many(models)
            .on_conflict(
                OnConflict::columns([
                    contents::Column::PlayerConnectionId,
                    contents::Column::ContentId,
                ])
                .update_columns([
                    contents::Column::CachedData,
                    contents::Column::SortKey,
                    contents::Column::ParentId,
                ])
                .to_owned(),
            )
            .exec(&txn)
            .await?;

        txn.commit().await?;

        Ok(())
    }

    pub async fn start_download(
        db: &DatabaseConnection,
        connection_id: i32,
        content_id: &str,
    ) -> ModelResult<(Self, content_downloads::Model)> {
        let txn = db.begin().await?;

        let content_db = contents::Entity::find()
            .filter(contents::Column::PlayerConnectionId.eq(connection_id))
            .filter(contents::Column::ContentId.eq(content_id))
            .one(&txn)
            .await?
            .ok_or(ModelError::EntityNotFound)?;

        let download = content_downloads::ActiveModel {
            status: ActiveValue::Set(StatusName::InProgress),
            ..Default::default()
        }
        .content(&content_db)
        .insert(&txn)
        .await?;

        let content_db = content_db
            .into_active_model()
            .status(Some(StatusName::InProgress))
            .update(&txn)
            .await?;

        txn.commit().await?;

        Ok((content_db, download))
    }

    pub async fn by_connection_and_parent_id(
        db: &DatabaseConnection,
        connection_id: i32,
        parent_id: Option<&str>,
    ) -> ModelResult<Vec<Model>> {
        let contents = contents::Entity::find()
            .filter(
                model::query::condition()
                    .eq(contents::Column::PlayerConnectionId, connection_id)
                    .eq(contents::Column::ParentId, parent_id)
                    .build(),
            )
            .order_by_asc(contents::Column::SortKey)
            .all(db)
            .await?;
        Ok(contents)
    }

    pub async fn by_connection_and_id(
        db: &DatabaseConnection,
        connection_id: i32,
        content_id: &str,
    ) -> ModelResult<Model> {
        let content = contents::Entity::find()
            .filter(
                model::query::condition()
                    .eq(contents::Column::PlayerConnectionId, connection_id)
                    .eq(contents::Column::ContentId, content_id)
                    .build(),
            )
            .one(db)
            .await?;
        content.ok_or_else(|| ModelError::EntityNotFound)
    }

    pub async fn content_by_connection_and_id(
        db: &DatabaseConnection,
        connection_id: i32,
        content_id: &str,
    ) -> ModelResult<ContentWithModel> {
        Self::by_connection_and_id(db, connection_id, content_id)
            .await
            .and_then(ContentWithModel::try_from)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ContentWithModel {
    #[serde(flatten)]
    pub content: Content,
    pub status: Option<StatusName>,
}

impl TryFrom<Model> for ContentWithModel {
    type Error = ModelError;

    fn try_from(value: Model) -> Result<Self, Self::Error> {
        let content = serde_json::from_value(
            value
                .cached_data
                .clone()
                .ok_or(ModelError::EntityNotFound)?,
        )
        .map_err(|e| ModelError::Any(e.into()))?;
        Ok(Self {
            content,
            status: value.status,
        })
    }
}
