use std::{
    convert::Infallible,
    task::{Context, Poll},
};

use axum::{
    body::Body,
    extract::{FromRequestParts, Request},
    response::Response,
    Extension,
};
use futures_util::future::BoxFuture;
use loco_rs::prelude::{auth::JWTWithUser, *};
use serde_json::json;
use tower::{Layer, Service};

use crate::{initializers, models::users};

#[derive(Clone)]
pub struct ViewEngineAuthExt {
    state: AppContext,
}

impl ViewEngineAuthExt {
    #[must_use] pub fn new(state: AppContext) -> Self {
        Self { state }
    }
}

impl<S> Layer<S> for ViewEngineAuthExt {
    type Service = ViewEngineAuthExtService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Self::Service {
            inner,
            state: self.state.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ViewEngineAuthExtService<S> {
    inner: S,
    state: AppContext,
}

impl<S, B> Service<Request<B>> for ViewEngineAuthExtService<S>
where
    S: Service<Request<B>, Response = Response<Body>, Error = Infallible> + Clone + Send + 'static, /* Inner Service must return Response<Body> and never error */
    S::Future: Send + 'static,
    B: Send + 'static,
{
    // Response type is the same as the inner service / handler
    type Response = S::Response;
    // Error type is the same as the inner service / handler
    type Error = S::Error;
    // Future type is the same as the inner service / handler
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let state = self.state.clone();
        let clone = self.inner.clone();
        // take the service that was ready
        let mut inner = std::mem::replace(&mut self.inner, clone);
        Box::pin(async move {
            // Example of extracting JWT and checking roles
            let (mut parts, body) = req.into_parts();
            let auth = JWTWithUser::<users::Model>::from_request_parts(&mut parts, &state).await;

            let authed = match auth {
                Ok(auth) => {
                    json!({"logged_in": true, "user": auth.user})
                }
                Err(_) => {
                    json!({"logged_in": false})
                }
            };

            let view = Extension::<ViewEngine::<initializers::view_engine::BetterTeraView>>::from_request_parts(&mut parts, &state).await;
            let view = match view {
                Ok(mut view) => {
                    view.0 .0.default_context.insert("auth", &authed);
                    view.0
                }
                Err(_) => {
                    return Ok(Response::builder()
                        .status(500)
                        .body(Body::empty())
                        .unwrap()
                        .into_response())
                }
            };

            parts.extensions.insert(view);
            let req = Request::from_parts(parts, body);
            inner.call(req).await
        })
    }
}
