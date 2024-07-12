use loco_rs::cli;
use migration::Migrator;
use moonlit_binge::app::App;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    cli::main::<App, Migrator>().await
}
