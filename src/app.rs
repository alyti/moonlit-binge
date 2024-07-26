use std::{net::SocketAddr, path::Path};

use async_trait::async_trait;
use axum::{
    body::Body,
    http::Request,
    middleware::{from_fn, Next},
    response::Response,
    routing::get_service,
    Router,
};
use loco_rs::{
    app::{AppContext, Hooks, Initializer},
    boot::{create_app, BootResult, ServeParams, StartMode},
    config::Config,
    controller::AppRoutes,
    db::{self, truncate_table},
    environment::Environment,
    storage::Storage,
    task::Tasks,
    worker::{AppWorker, Processor},
    Result,
};
use migration::Migrator;
use sea_orm::DatabaseConnection;
use tower_http::services::ServeDir;
use tracing::warn;

use crate::{
    common::settings::{self, Settings, SETTINGS},
    controllers::{self, middlewares},
    initializers,
    models::_entities::users,
    tasks,
    workers::downloader::DownloadWorker,
};

pub struct App;
#[async_trait]
impl Hooks for App {
    fn app_name() -> &'static str {
        env!("CARGO_CRATE_NAME")
    }

    fn app_version() -> String {
        format!(
            "{} ({})",
            env!("CARGO_PKG_VERSION"),
            option_env!("BUILD_SHA")
                .or(option_env!("GITHUB_SHA"))
                .unwrap_or("dev")
        )
    }

    async fn serve(app: Router, server_config: ServeParams) -> Result<()> {
        let listener = match listenfd::ListenFd::from_env().take_tcp_listener(0)? {
            // if we are given a tcp listener on listen fd 0, we use that one
            Some(listener) => {
                warn!(listener = ?listener, "using listener fd, ignore above binding config");
                listener.set_nonblocking(true)?;
                tokio::net::TcpListener::from_std(listener)?
            }
            // otherwise fall back to configured binding
            None => {
                tokio::net::TcpListener::bind(&format!(
                    "{}:{}",
                    server_config.binding, server_config.port
                ))
                .await?
            }
        };

        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await?;

        Ok(())
    }

    async fn boot(mode: StartMode, environment: &Environment) -> Result<BootResult> {
        create_app::<Self, Migrator>(mode, environment).await
    }

    async fn before_run(ctx: &AppContext) -> Result<()> {
        match &ctx.environment {
            Environment::Any(a) if a.starts_with(".suffering") => {
                tokio::fs::remove_file(format!("config/{a}.yaml")).await?;
            }
            Environment::Production | Environment::Development => {
                let config = ctx.config.clone();
                tokio::spawn(async move {
                    serve_streams(config).await.unwrap();
                });
            }
            _ => {}
        }
        Ok(())
    }

    async fn after_context(ctx: AppContext) -> Result<AppContext> {
        settings::Settings::to_cell(&ctx).await?;
        Ok(AppContext {
            storage: Storage::single(loco_rs::storage::drivers::local::new_with_prefix(
                &SETTINGS.get().unwrap().transcoding_dir,
            )?)
            .into(),
            ..ctx
        })
    }

    async fn initializers(_ctx: &AppContext) -> Result<Vec<Box<dyn Initializer>>> {
        Ok(vec![
            Box::new(initializers::view_engine::ViewEngineInitializer),
            Box::new(initializers::media_provider::MediaProviderInitializer),
            Box::new(initializers::layers::LayersInitializer),
        ])
    }

    fn routes(ctx: &AppContext) -> AppRoutes {
        AppRoutes::with_default_routes()
            .add_route(controllers::player_connections::routes())
            .add_route(controllers::auth::routes().layer(
                middlewares::view_auth_ext::ViewEngineAuthExt::new(ctx.clone()),
            ))
            .add_route(controllers::user::routes())
            .add_route(controllers::dashboard::routes())
            .add_route(controllers::playlist::routes())
    }

    fn connect_workers<'a>(p: &'a mut Processor, ctx: &'a AppContext) {
        p.register(DownloadWorker::build(ctx));
    }

    fn register_tasks(tasks: &mut Tasks) {
        tasks.register(tasks::seed::SeedData);
    }

    async fn truncate(db: &DatabaseConnection) -> Result<()> {
        truncate_table(db, users::Entity).await?;
        Ok(())
    }

    async fn seed(db: &DatabaseConnection, base: &Path) -> Result<()> {
        db::seed::<users::ActiveModel>(db, &base.join("users.yaml").display().to_string()).await?;
        Ok(())
    }
}

pub async fn files_mw(request: Request<Body>, next: Next) -> Response {
    let uri = request.uri().to_owned();
    let path = uri.path();

    let splited = path.split(".").collect::<Vec<_>>();

    let content_type = if let Some(ext) = splited.last() {
        let extension = ext.to_owned().to_lowercase();

        match extension.as_str() {
            "mp4" => "video/mp4",
            "m3u8" => "application/vnd.apple.mpegurl",
            "ts" => "video/mp2t",
            _ => "application/octet-stream",
        }
    } else {
        "unknown"
    };

    let mut response = next.run(request).await;
    let headers_mut = response.headers_mut();
    headers_mut.insert("Cache-Control", "public, max-age=31536000".parse().unwrap());
    headers_mut.insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    headers_mut.insert(
        "Access-Control-Allow-Methods",
        "GET, OPTIONS".parse().unwrap(),
    );

    if let Ok(content_type) = content_type.parse() {
        headers_mut.insert("Content-Type", content_type);
    }

    response
}

async fn serve_streams(config: Config) -> Result<()> {
    let settings: Settings = (&config).try_into().unwrap();
    let listener = tokio::net::TcpListener::bind(if let Some(addr) = &settings.file_server_addr {
        addr
    } else {
        "0.0.0.0:3000"
    })
    .await?;
    let serve_dir = ServeDir::new(settings.transcoding_dir);
    let router = Router::new()
        .fallback_service(get_service(serve_dir))
        .layer(from_fn(files_mw));

    axum::serve(listener, router).await?;

    Ok(())
}
