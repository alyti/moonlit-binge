use loco_rs::prelude::*;
use players::types::Item;
use serde::Serialize;

use crate::{
    initializers::media_provider::{MediaProvider, MediaProviderType, MediaProviders},
    models::_entities::player_connections,
};

/// Render a list view of `player_connections`.
///
/// # Errors
///
/// When there is an issue with rendering the view.
pub fn list(v: &impl ViewRenderer, items: &Vec<player_connections::Model>) -> Result<Response> {
    format::render().view(
        v,
        "player_connections/list.html",
        serde_json::json!({"items": items}),
    )
}

/// Render a single `player_connections` view.
///
/// # Errors
///
/// When there is an issue with rendering the view.
pub fn show(
    v: &impl ViewRenderer,
    provider: &MediaProvider,
    item: &player_connections::Model,
    items: Vec<Item>,
) -> Result<Response> {
    format::render().view(
        v,
        "player_connections/show.html",
        serde_json::json!({"item": item, "items": items, "provider": provider}),
    )
}

/// Render a `player_connections` create form.
///
/// # Errors
///
/// When there is an issue with rendering the view.
pub fn create(v: &impl ViewRenderer, providers: Box<MediaProviders>) -> Result<Response> {
    format::render().view(
        v,
        "player_connections/create.html",
        serde_json::json!({"providers": providers}),
    )
}

/// Render a `player_connections` create form.
///
/// # Errors
///
/// When there is an issue with rendering the view.
pub fn setup(
    v: &impl ViewRenderer,
    provider: &MediaProvider,
    provider_setup: serde_json::Value,
) -> Result<Response> {
    format::render().view(
        v,
        match &provider.type_field {
            MediaProviderType::Jellyfin => "player_connections/setup/jellyfin.html",
        },
        serde_json::json!({"setup": provider_setup, "provider": provider}),
    )
}

/// Render a `player_connections` edit form.
///
/// # Errors
///
/// When there is an issue with rendering the view.
pub fn edit(v: &impl ViewRenderer, item: &player_connections::Model) -> Result<Response> {
    format::render().view(
        v,
        "player_connections/edit.html",
        serde_json::json!({"item": item}),
    )
}

pub fn base_view<T: Serialize>(
    v: &impl ViewRenderer,
    partial: bool,
    action: &str,
    ctx: &T,
) -> Result<Response> {
    format::render().view(
        v,
        &format!(
            "player_connections/{}.html",
            if partial { action } else { "index" }
        ),
        HtmxPartial { action, ctx },
    )
}

#[derive(serde::Serialize)]
pub struct HtmxPartial<'a, T: Serialize> {
    pub action: &'a str,
    #[serde(flatten)]
    pub ctx: &'a T,
}
