use loco_rs::{app::AppContext, config::Config, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;

pub static SETTINGS: OnceCell<Box<Settings>> = OnceCell::const_new();

#[derive(Serialize, Deserialize, Debug)]
pub struct Settings {
    pub transcoding_dir: std::path::PathBuf,
    pub ip_source: axum_client_ip::SecureClientIpSource,
    pub file_server_addr: Option<String>,
}

impl Settings {
    pub async fn to_cell(ctx: &AppContext) -> Result<()> {
        SETTINGS
            .get_or_init(|| async {
                let settings = (&ctx.config).try_into().unwrap();
                Box::new(settings)
            })
            .await;
        Ok(())
    }
}

impl TryInto<Settings> for &Config {
    type Error = serde_json::Error;

    fn try_into(self) -> Result<Settings, Self::Error> {
        serde_json::from_value(self.settings.clone().unwrap_or_default())
    }
}
