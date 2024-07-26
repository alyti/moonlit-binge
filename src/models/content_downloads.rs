use super::_entities::{
    content_downloads::{ActiveModel, Model},
    contents,
    sea_orm_active_enums::StatusName,
};
use loco_rs::model::{ModelError, ModelResult};
use sea_orm::{entity::prelude::*, ActiveValue, Statement, TransactionTrait};
use serde::Serialize;

impl ActiveModelBehavior for ActiveModel {
    // extend activemodel below (keep comment for generators)
}

impl ActiveModel {
    pub fn content(mut self, content: &contents::Model) -> Self {
        self.content_id = ActiveValue::Set(content.content_id.clone());
        self.player_connection_id = ActiveValue::Set(content.player_connection_id.clone());
        self
    }

    pub fn status_info<T: Serialize>(mut self, status_info: &T) -> ModelResult<Self> {
        let status_info =
            serde_json::to_value(status_info).map_err(|e| ModelError::Any(e.into()))?;
        self.status_info = ActiveValue::Set(Some(status_info));
        Ok(self)
    }

    pub fn id(mut self, id: Uuid) -> Self {
        self.id = ActiveValue::Set(id);
        self
    }
}

impl Model {
    pub async fn notify_status<T: Serialize>(
        db: &DatabaseConnection,
        id: Uuid,
        content_id: &str,
        status: StatusName,
        status_info: &T,
    ) -> ModelResult<Self> {
        let txn = db.begin().await?;

        let content: Model = ActiveModel {
            status: ActiveValue::Set(status.clone()),
            ..Default::default()
        }
        .id(id)
        .status_info(status_info)?
        .update(&txn)
        .await?;
        let notify: Notification = content.clone().into();

        contents::ActiveModel {
            player_connection_id: ActiveValue::Set(content.player_connection_id),
            content_id: ActiveValue::Set(content_id.to_string()),
            ..Default::default()
        }
        .status(Some(status))
        .update(&txn)
        .await?;

        txn.query_one(Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Postgres,
            "SELECT pg_notify('provider-' || $1, $2::text)",
            vec![
                content.player_connection_id.into(),
                serde_json::to_value(&notify).unwrap().into(),
            ],
        ))
        .await?;

        txn.commit().await?;

        Ok(content)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Notification {
    pub player_connection_id: i32,
    pub download_id: Uuid,
    pub content_id: String,
    #[serde(flatten)]
    pub status: Json,
}

impl From<Model> for Notification {
    fn from(content: Model) -> Self {
        Self {
            player_connection_id: content.player_connection_id,
            download_id: content.id,
            content_id: content.content_id,
            status: content.status_info.unwrap_or_default(),
        }
    }
}
