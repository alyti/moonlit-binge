use loco_rs::prelude::*;
use serde::Serialize;
use serde_json::json;

use crate::models::_entities::player_connections;

use super::Format;

/// Home view
pub fn home<V: ViewRenderer>(
    f: Format<V>,
    connections: &[player_connections::Model],
) -> Result<Response> {
    f.render(
        None,
        "dashboard",
        "home",
        &json!({"connections": &connections}),
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
        &format!("dashboard/{}.html", if partial { action } else { "index" }),
        HtmxPartial { action, ctx },
    )
}

#[derive(serde::Serialize)]
pub struct HtmxPartial<'a, T: Serialize> {
    pub action: &'a str,
    #[serde(flatten)]
    pub ctx: &'a T,
}
