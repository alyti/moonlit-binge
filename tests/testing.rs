use std::env::temp_dir;

use async_once_cell::OnceCell;
use axum_test::{TestServer, TestServerConfig};
use loco_rs::{
    app::{AppContext, Hooks},
    boot::{self, BootResult},
    environment::Environment,
    Result,
};
use players::{
    jellyfin::Jellyfin,
    testcontainers::{Jellyfin as JellyfinContainer, JELLYFIN_HTTP_PORT},
};
use serde_json::json;
use testcontainers::{runners::AsyncRunner, ContainerAsync, ImageExt};
use testcontainers_modules::{postgres::Postgres, redis::Redis};
use uuid::Uuid;

static ONCE_JELLYFIN: OnceCell<ContainerAsync<JellyfinContainer>> = OnceCell::new();

/// use Tera to template `config/test.yaml.tpl` with ctx and output it to `config/<uuid>.yaml`, return Uuid
/// cleanup is up to caller
async fn prepare_env_file(ctx: serde_json::Value) -> Result<Uuid> {
    let name = Uuid::new_v4();
    let result = tera::Tera::one_off(
        &tokio::fs::read_to_string("config/test.yaml.tera")
            .await
            .unwrap(),
        &tera::Context::from_value(ctx).unwrap(),
        false,
    )
    .unwrap();
    tokio::fs::write(format!("config/.suffering-{name}.yaml"), result).await?;
    Ok(name)
}

/// Incredibly cursed testcontainer setup that calls callback with a prepared `BootResult` or explodes, which is cool too
/// ```rust
/// #[tokio::main]
/// async fn main() {
///    // A
/// }
/// ```
/// # Panics
///
/// Panics if any containers fail to start
/// Panics if app fails to boot
/// Panics if env file can't be written to
pub async fn boot_with_testcontainers<H: Hooks, F, Fut>(callback: F)
where
    F: FnOnce(BootResult) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    // Oh look are those containarized hard dependencies that we absolutely require to even run tests? Yup
    // Oh neat they are all in one place and we don't need to POLLUTE THE ENVIRONMENT... right?...

    let pg = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .unwrap();
    let redis = Redis.start().await.unwrap();

    let jellyfin = Box::pin(ONCE_JELLYFIN.get_or_init(async {
        let container = JellyfinContainer::default()
            .with_media_mount(format!("{}/{}", env!("CARGO_MANIFEST_DIR"), "tests/media"))
            .start()
            .await
            .unwrap();
        let host = container.get_host().await.unwrap();
        let host_port = container
            .get_host_port_ipv4(JELLYFIN_HTTP_PORT)
            .await
            .unwrap();
        let url = format!("http://{host}:{host_port}");
        let client = Jellyfin::new(&url, &None);
        client
            .complete_startup("root", Some("/media"))
            .await
            .unwrap();
        container
    }))
    .await;

    let tmp_transcoding = temp_dir()
        .join("transcoding")
        .join(Uuid::new_v4().to_string());
    tokio::fs::create_dir_all(tmp_transcoding.clone())
        .await
        .unwrap();

    // Sike we sure have to, BECAUSE THANKS LOCO
    let env = prepare_env_file(json!({
        "database_url": format!(
            "postgres://postgres:postgres@127.0.0.1:{}/postgres?ssl-mode=disable",
            pg.get_host_port_ipv4(5432).await.unwrap()
        ),
        "redis_url": format!(
            "redis://{}:{}",
            redis.get_host().await.unwrap(),
            redis.get_host_port_ipv4(6379).await.unwrap()
        ),
        "jellyfin_url": format!("http://{}:{}", jellyfin.get_host().await.unwrap(), jellyfin.get_host_port_ipv4(JELLYFIN_HTTP_PORT).await.unwrap()),
        "port": 0, // Neat little thing about unix port 0: it will be randomly allocated :D
        "transcoding_folder": tmp_transcoding.to_str().unwrap(),
    }))
    .await
    .unwrap();
    // Honestly I am lost for words how fucking annoying loco_rs is with this config/env.yaml bullshit...
    // Like it didn't have to be like this, but I guess they just couldn't move past Rails way of thinking...

    // Use our new throwaway env to do neferious things
    let boot = H::boot(
        boot::StartMode::ServerOnly,
        &Environment::Any(format!(".suffering-{env}")),
    )
    .await
    .unwrap();

    // Actually run test
    callback(boot).await;

    // Clean up garbage
    // TODO: for now this is handled in App::before_run, since this point is sometimes not called if tests fail...
    // tokio::fs::remove_file(format!("config/.suffering-{}.yaml", env.to_string())).await.unwrap();
}

/// Functionally identical to `loco_rs::testing::request` but it can run in parallel and comes with it's own test env.
///
/// # Panics
///
/// Panics if router is unavailable
/// Panics if `TestServer` fails to start
#[allow(clippy::future_not_send)]
pub async fn request_with_testcontainers<H: Hooks, F, Fut>(callback: F)
where
    F: FnOnce(TestServer, AppContext) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    boot_with_testcontainers::<H, _, _>(|boot| async move {
        let config = TestServerConfig::builder()
            .default_content_type("application/json")
            .build();

        let server = TestServer::new_with_config(boot.router.unwrap(), config).unwrap();

        callback(server, boot.app_context.clone()).await;
    })
    .await;
}
