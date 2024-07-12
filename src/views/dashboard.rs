use loco_rs::prelude::*;
use serde::Serialize;
use serde_json::json;

/// Home view
///
/// # Errors
///
/// This function will return an error if render fails
pub fn home(v: &impl ViewRenderer) -> Result<Response> {
    format::render().view(v, "dashboard/home.html", json!({}))
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
