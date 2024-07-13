#![allow(clippy::unused_async)]
use axum::Extension;
use axum_htmx::HxRequest;
use loco_rs::prelude::*;
use sea_orm::{Order, QueryOrder};

use crate::{
    initializers::{media_provider::MediaProviders, view_engine::BetterTeraView},
    models::_entities::{player_connections, users},
    views,
};

use super::extractors::auth::JWTWithUser;

/// Renders the dashboard home page
///
/// # Errors
///
/// This function will return an error if render fails
pub async fn render_home(
    State(ctx): State<AppContext>,
    ViewEngine(v): ViewEngine<BetterTeraView>,
    HxRequest(boosted): HxRequest,
    Extension(_media_providers): Extension<Box<MediaProviders>>,
    auth: JWTWithUser<users::Model>,
) -> Result<Response> {
    let connections = player_connections::Entity::find()
        .order_by(player_connections::Column::Id, Order::Desc)
        .filter(player_connections::Column::UserId.eq(auth.user.id))
        .all(&ctx.db)
        .await?;
    views::dashboard::base_view(
        &v,
        boosted,
        "home",
        &serde_json::json!({"connections": &connections}),
    )
}

pub fn routes() -> Routes {
    Routes::new().add("/", get(render_home))
}
