use axum::response::Response;
use loco_rs::{controller::format::RenderBuilder, prelude::ViewRenderer, Result};
use serde::Serialize;

pub mod auth;
pub mod dashboard;
pub mod user;

pub mod player_connections;

pub enum Format<V: ViewRenderer> {
    Json,
    HtmxFull(V),
    HtmxPartial(V),
}

impl<V: ViewRenderer> Format<V> {
    /// Render the response based on the format
    /// If the format is JSON, it will render the context as JSON.
    /// Otherwise it will render the context using the view engine.
    ///
    /// # Arguments
    ///
    /// `render_builder` - The render builder to use, if none is provided, a default one will be used,
    /// this is useful for setting headers, status codes, etc.
    /// `ctx` - The context to render, this will be serialized and passed to the view engine or rendered as JSON.
    /// `namespace` - The namespace of the view to render (e.g. `auth`, `dashboard`, etc.)
    /// `action` - The view to render (e.g. `home`, `login`, etc.)
    ///
    /// # Errors
    ///
    /// This function will return an error if the view engine fails to render the view
    pub fn render<T: Serialize>(
        &self,
        render_builder: Option<RenderBuilder>,
        namespace: &str,
        action: &str,
        ctx: &T,
    ) -> Result<Response> {
        match self {
            Format::Json => render_builder.unwrap_or_default().json(ctx),
            Format::HtmxFull(v) => render_builder.unwrap_or_default().view(
                v,
                &format!("{namespace}/index.html"),
                HtmxPartial {
                    action: &action,
                    ctx,
                },
            ),
            Format::HtmxPartial(v) => render_builder.unwrap_or_default().view(
                v,
                &format!("{namespace}/{action}.html"),
                ctx,
            ),
        }
    }
}

#[derive(serde::Serialize)]
pub struct HtmxPartial<'a, T: Serialize> {
    pub action: &'a str,
    #[serde(flatten)]
    pub ctx: &'a T,
}
