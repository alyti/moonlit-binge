pub mod auth;

use async_trait::async_trait;
use axum::{
    body::Body,
    extract::{FromRef, FromRequestParts, Host, Query},
    http::{request::Parts, HeaderMap, Uri},
    response::{IntoResponse, Response},
};
use loco_rs::prelude::{ViewEngine, ViewRenderer};
use reqwest::{header, StatusCode};

pub struct ProtoHost(pub String);

#[async_trait]
impl<S> FromRequestParts<S> for ProtoHost
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let host = Host::from_request_parts(parts, state)
            .await
            .map(|host| Self(host.0))
            .map_err(IntoResponse::into_response);

        // If the service is running behind a reverse proxy we need to
        // use the `x-forwarded-proto` header to get the real proto of the request
        // so our references to the service are correct for the client
        let scheme = match parts.headers.get("x-forwarded-proto") {
            Some(scheme) => scheme.to_str().map_err(|_| {
                (StatusCode::BAD_REQUEST, "Invalid x-forwarded-proto header").into_response()
            })?,
            None => "http",
        };

        Ok(Self(format!("{}://{}", scheme, host?.0)))
    }
}

pub struct Format<V: ViewRenderer + Clone + Send + Sync>(pub crate::views::Format<V>);

#[async_trait]
impl<S, V: ViewRenderer + Clone + Send + Sync + 'static> FromRequestParts<S> for Format<V>
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let header = None
            .or_else(|| parts.headers.get(axum::http::header::ACCEPT))
            .or_else(|| parts.headers.get(axum::http::header::CONTENT_TYPE));

        let is_partial = parts.headers.get(axum_htmx::HX_REQUEST).is_some();

        let format = match header {
            Some(h) if h.to_str().unwrap().contains("text/html") || is_partial => {
                let engine = ViewEngine::<V>::from_request_parts(parts, state)
                    .await
                    .unwrap()
                    .0;

                if is_partial {
                    crate::views::Format::HtmxPartial(engine)
                } else {
                    crate::views::Format::HtmxFull(engine)
                }
            }
            Some(h) if h.to_str().unwrap().contains("application/json") => {
                crate::views::Format::Json
            }
            Some(_) | None => {
                tracing::warn!("Unsupported format requested, defaulting to JSON");
                crate::views::Format::Json
            }
        };

        Ok(Self(format))
    }
}
