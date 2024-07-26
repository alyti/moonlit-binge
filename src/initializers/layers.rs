use async_trait::async_trait;
use axum::Router as AxumRouter;
use axum_client_ip::SecureClientIp;
use loco_rs::prelude::*;
use tower_http::{add_extension::AddExtensionLayer, trace::TraceLayer};

use crate::common::settings::SETTINGS;

pub struct LayersInitializer;

#[async_trait]
impl Initializer for LayersInitializer {
    fn name(&self) -> String {
        "some-layers".to_string()
    }

    async fn after_routes(&self, router: AxumRouter, ctx: &AppContext) -> Result<AxumRouter> {
        let router = router
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &axum::http::Request<_>| {
                // loco has its own logging middleware but it doesn't have origin_ip field
                let request_id = uuid::Uuid::new_v4();
                let user_agent = request
                    .headers()
                    .get(axum::http::header::USER_AGENT)
                    .map_or("", |h| h.to_str().unwrap_or(""));
                let uri = request.uri();
                let path = uri.path();
                if path.eq("/_health") || path.eq("/_ping") {
                    // Skip trying to resolve IP, etc for health checks
                    return tracing::error_span!(
                        "http-request",
                        "http.method" = tracing::field::display(request.method()),
                        "http.uri" = tracing::field::display(uri),
                        "http.version" = tracing::field::debug(request.version()),
                        "http.user_agent" = tracing::field::display(user_agent),
                        request_id = tracing::field::display(request_id),
                    );
                }

                let extensions = request
                    .extensions();

                let env: String = extensions
                    .get::<loco_rs::environment::Environment>()
                    .map(std::string::ToString::to_string)
                    .unwrap_or_default();

                let headers = request.headers();
                let ip: String = match SecureClientIp::from(&SETTINGS.get().unwrap().ip_source, headers, extensions) {
                    Ok(ip) => ip.0.to_string(),
                    Err(_) => {
                        tracing::error!("Could not get client ip using configured ip source, falling back to insecure ip source");
                        match axum_client_ip::InsecureClientIp::from(headers, extensions) {
                            Ok(ip) => ip.0.to_string(),
                            Err(_) => "unknown".to_string(),
                        }
                    }
                };

                tracing::error_span!(
                    "http-request",
                    "http.method" = tracing::field::display(request.method()),
                    "http.uri" = tracing::field::display(uri),
                    "http.version" = tracing::field::debug(request.version()),
                    "http.user_agent" = tracing::field::display(user_agent),
                    "http.origin_ip" = tracing::field::display(ip),
                    "environment" = tracing::field::display(env),
                    request_id = tracing::field::display(request_id),
                )
            }),
        ).layer(
            AddExtensionLayer::new(ctx.environment.clone()),
        );
        Ok(router)
    }
}
