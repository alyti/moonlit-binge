use std::{net::SocketAddr, path::Path};

use async_trait::async_trait;
use axum::Router;
use chrono::format;
use loco_rs::{
    app::{AppContext, Hooks, Initializer},
    boot::{create_app, BootResult, ServeParams, StartMode},
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
use tracing::warn;

use crate::{
    common::settings::{self, SETTINGS},
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
                tokio::fs::remove_file(format!("config/{}.yaml", a)).await?;
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
