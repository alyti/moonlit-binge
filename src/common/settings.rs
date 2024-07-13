use loco_rs::{app::AppContext, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;

pub static SETTINGS: OnceCell<Box<Settings>> = OnceCell::const_new();

#[derive(Serialize, Deserialize, Debug)]
pub struct Settings {
    pub transcoding_dir: std::path::PathBuf,
    pub ip_source: axum_client_ip::SecureClientIpSource,
}

impl Settings {
    /// Deserialize a strongly typed settings
    ///
    /// # Errors
    ///
    /// This function will return an error if deserialization fails
    pub fn from_json(value: &serde_json::Value) -> Result<Self> {
        Ok(serde_json::from_value(value.clone())?)
    }

    pub async fn to_cell(ctx: &AppContext) -> Result<()> {
        SETTINGS
            .get_or_init(|| async {
                let settings = Self::from_json(ctx.config.settings.as_ref().unwrap()).unwrap();
                Box::new(settings)
            })
            .await;
        Ok(())
    }
}
