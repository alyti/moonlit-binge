use std::collections::BTreeMap;

use axum::{async_trait, Extension, Router as AxumRouter};
use eyre::ContextCompat;
use loco_rs::{
    app::{AppContext, Initializer},
    Error, Result,
};
use players::types::{Content, Item, Library, MediaStream, TranscodeJob};
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;

use crate::models::player_connections;

pub static CELL: OnceCell<Box<MediaProviders>> = OnceCell::const_new();

pub type MediaProviders = BTreeMap<String, MediaProvider>;
pub type MediaProviderList = Vec<MediaProvider>;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaProvider {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub type_field: MediaProviderType,
    pub profiles: Vec<Profile>,
    pub exclude_library_ids: Vec<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub description: String,
    pub playback_settings: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaProviderType {
    Jellyfin,
    // Add your own media server type here, make sure to implement it in players crate too.
}

impl Default for MediaProviderType {
    fn default() -> Self {
        MediaProviderType::Jellyfin
    }
}

pub struct MediaProviderInitializer;
#[async_trait]
impl Initializer for MediaProviderInitializer {
    fn name(&self) -> String {
        "media-providers".to_string()
    }

    async fn before_run(&self, ctx: &AppContext) -> Result<()> {
        CELL.get_or_try_init(|| async {
            let media_providers_config =
                ctx.config.initializers.clone().ok_or_else(|| {
                    Error::Message("initializers config not configured".to_string())
                })?;

            let media_providers =
                media_providers_config
                    .get("media_providers")
                    .ok_or_else(|| {
                        Error::Message(
                            "initializers.media_provider config not configured".to_string(),
                        )
                    })?;

            let media_providers: MediaProviderList =
                serde_json::from_value(media_providers.clone())?;

            let mut map = BTreeMap::new();
            for provider in media_providers {
                let id = provider.id.clone();
                match map.insert(id.clone(), provider) {
                    Some(_) => {
                        return Err(Error::Message(format!(
                            "Duplicate media provider id: {}",
                            id
                        )));
                    }
                    None => {}
                }
            }
            Ok(Box::new(map))
        })
        .await?;
        Ok(())
    }

    async fn after_routes(&self, router: AxumRouter, _ctx: &AppContext) -> Result<AxumRouter> {
        Ok(router.layer(Extension(
            CELL.get()
                .wrap_err(Error::Message(
                    "initializers.media_provider config not configured".to_string(),
                ))?
                .clone(),
        )))
    }
}

impl MediaProvider {
    pub async fn setup(
        &self,
        _ctx: &AppContext,
        setup: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        match self.type_field {
            MediaProviderType::Jellyfin => {
                let jellyfin = players::jellyfin::Jellyfin::new(&self.url, &None);
                jellyfin.setup(setup).await.map_err(|e| e.into())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectedMediaProvider {
    pub provider: MediaProvider,
    pub identity: serde_json::Value,
    pub preferences: Option<serde_json::Value>,
    pub preferred_profile: Option<String>,
}

impl TryFrom<player_connections::Model> for ConnectedMediaProvider {
    type Error = Error;

    fn try_from(value: player_connections::Model) -> Result<Self> {
        let provider = CELL
            .get()
            .wrap_err(Error::Message("Media Providers not configured".to_string()))?
            .get(&value.media_provider_id)
            .ok_or_else(|| Error::NotFound)?;
        Ok(Self {
            provider: provider.clone(),
            identity: value.identity.ok_or_else(|| {
                Error::BadRequest("Player connection does not have an identity".to_string())
            })?,
            preferences: value.preferences,
            preferred_profile: value.preferred_profile,
        })
    }
}

impl ConnectedMediaProvider {
    // mostly used in setup before we have a real connection model
    pub fn from_provider_and_connection(
        provider: MediaProvider,
        identity: serde_json::Value,
    ) -> Self {
        Self {
            provider,
            identity,
            preferences: None,
            preferred_profile: None,
        }
    }

    pub async fn preferences(
        &self,
        _ctx: &AppContext,
        new_preferences: Option<serde_json::Value>,
    ) -> Result<Option<serde_json::Value>> {
        match self.provider.type_field {
            MediaProviderType::Jellyfin => {
                let jellyfin =
                    players::jellyfin::Jellyfin::new(&self.provider.url, &self.preferences);
                jellyfin
                    .preferences(&self.identity, new_preferences)
                    .await
                    .map_err(|e| e.into())
            }
        }
    }

    pub async fn test(&self, _ctx: &AppContext) -> Result<()> {
        match self.provider.type_field {
            MediaProviderType::Jellyfin => {
                let jellyfin =
                    players::jellyfin::Jellyfin::new(&self.provider.url, &self.preferences);
                jellyfin.test(&self.identity).await.map_err(|e| e.into())
            }
        }
    }

    pub async fn items(&self, _ctx: &AppContext, library: Option<Library>) -> Result<Vec<Item>> {
        let items = match self.provider.type_field {
            MediaProviderType::Jellyfin => {
                let jellyfin =
                    players::jellyfin::Jellyfin::new(&self.provider.url, &self.preferences);
                let user = jellyfin
                    .user_from_identity(&self.identity)
                    .await
                    .map_err(|e| Error::Anyhow(e.into()))?;
                user.items(library).await.map_err(|e| eyre::eyre!(e).into())
            }
        };
        match items {
            Ok(items) => Ok(items
                .into_iter()
                .filter(|item| {
                    if let Item::Library(library) = &item {
                        !self.provider.exclude_library_ids.contains(&library.id)
                    } else {
                        true
                    }
                })
                .collect()),
            Err(e) => {
                tracing::error!("{:?}", e);
                Err(Error::Anyhow(e))
            }
        }
    }

    pub async fn item(&self, _ctx: &AppContext, id: &str) -> Result<Item> {
        match self.provider.type_field {
            MediaProviderType::Jellyfin => {
                let jellyfin =
                    players::jellyfin::Jellyfin::new(&self.provider.url, &self.preferences);
                let user = jellyfin
                    .user_from_identity(&self.identity)
                    .await
                    .map_err(|e| Error::Anyhow(e.into()))?;
                user.item(id).await.map_err(|e| eyre::eyre!(e).into())
            }
        }
    }

    pub async fn transcode(
        &self,
        _ctx: &AppContext,
        content: &Content,
        profile: Option<&str>,
        preferred_media_streams: &[MediaStream],
    ) -> Result<TranscodeJob> {
        let preferred_profile = profile
            .map(|p| p.to_string())
            .or(self.preferred_profile.clone())
            .or_else(|| {
                Some(
                    self.provider
                        .profiles
                        .first()
                        .map(|p| p.name.clone())
                        .ok_or(loco_rs::Error::Message("No profiles available".to_string()))
                        .unwrap(),
                )
            })
            .ok_or_else(|| loco_rs::Error::string("Unknown profile"))?;
        let profile: &serde_json::Value = &self
            .provider
            .profiles
            .iter()
            .find(|p| p.name == preferred_profile)
            .ok_or_else(|| Error::BadRequest("Invalid profile".to_string()))?
            .playback_settings;
        match self.provider.type_field {
            MediaProviderType::Jellyfin => {
                let jellyfin =
                    players::jellyfin::Jellyfin::new(&self.provider.url, &self.preferences);
                let user = jellyfin
                    .user_from_identity(&self.identity)
                    .await
                    .map_err(|e| Error::Anyhow(e.into()))?;
                user.transcode(content, profile.clone(), preferred_media_streams)
                    .await
                    .map_err(|e| Error::Anyhow(e.into()))
            }
        }
    }
}
