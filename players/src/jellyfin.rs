use self::types::{BaseItemKind, ResponseProfile, SubtitleProfile, TranscodingProfile};
use crate::types::{
    Content, ContentKind, Item, Library, LibraryKind, M3U8Playlist, MediaStream, TranscodeJob,
};
use chrono::Utc;
use progenitor::generate_api;
use reqwest::StatusCode;
use serde::Serialize;
use std::collections::HashMap;
use types::{AuthenticateUserByName, BaseItemDto};
use uuid::Uuid;

generate_api!("schemas/jellyfin-openapi-stable-models-only.json");

#[derive(Clone)]
pub struct JellyfinConfig {
    pub base_url: String,
}

impl JellyfinConfig {
    pub fn new(base_url: String) -> Self {
        Self { base_url }
    }
}

fn emby_authorization(token: Option<&str>) -> String {
    format!(
        r#"MediaBrowser Client="moonlit-binge", Device="Unknown VR HMD", DeviceId="placeholder", Version="0.0.1"{}"#,
        token.map_or("".to_string(), |t| format!(r#", Token="{}""#, t))
    )
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SetupStep {
    QcPoll { secret: String, code: String },
    Auth { id: String, token: String },
    Failed { cause: String },
}

#[derive(Clone, Debug)]
pub struct Jellyfin {
    pub base_url: String,
    pub preferences: Option<serde_json::Value>,
    client: reqwest::Client,
}

impl Jellyfin {
    pub fn new(base_url: &str, preferences: &Option<serde_json::Value>) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: reqwest::ClientBuilder::new()
                .connection_verbose(true)
                .build()
                .unwrap(),
            preferences: preferences.clone(),
        }
    }

    pub async fn ping(&self) -> Result<(), eyre::Error> {
        let url = format!("{}/health", self.base_url);
        let response = self.client.get(&url).send().await?;
        if response.status() == StatusCode::OK {
            Ok(())
        } else {
            Err(eyre::eyre!("Invalid status code: {}", response.status()))
        }
    }

    pub async fn setup(
        &self,
        setup: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, eyre::Error> {
        let setup: SetupStep = if let Some(setup) = setup {
            serde_json::from_value(setup)?
        } else {
            let qc = self.new_quick_connect().await?;
            return Ok(serde_json::to_value(qc.to_step())?);
        };
        match setup {
            SetupStep::Failed { .. } => Ok(serde_json::to_value(setup)?),
            SetupStep::QcPoll { secret, code } => {
                let qc = self.resume_quick_connect(&secret, &code);
                match qc.poll().await {
                    Ok(true) => Ok(serde_json::to_value(qc.auth().await?.to_step())?),
                    Ok(false) => Ok(serde_json::to_value(qc.to_step())?),
                    Err(err) => {
                        if let Some(code) = err.status() {
                            if code.is_client_error() {
                                return Ok(serde_json::to_value(SetupStep::Failed {
                                    cause: "Code Expired".to_string(),
                                })?);
                            }
                        }
                        Err(err.into())
                    }
                }
            }
            SetupStep::Auth { id, token } => Ok(serde_json::to_value(
                self.resume_user(&id, &token).to_step(),
            )?),
        }
    }

    pub async fn user_from_identity(
        &self,
        identity: &serde_json::Value,
    ) -> Result<JellyfinUser, eyre::Error> {
        let identity: SetupStep = serde_json::from_value(identity.clone())?;
        match identity {
            SetupStep::Auth { id, token } => Ok(self.resume_user(&id, &token)),
            _ => Err(eyre::eyre!("Invalid identity")),
        }
    }

    pub async fn test(&self, identity: &serde_json::Value) -> Result<(), eyre::Error> {
        let identity: SetupStep = serde_json::from_value(identity.clone())?;
        match identity {
            SetupStep::Auth { id, token } => {
                let user = self.resume_user(&id, &token);
                user.whoami().await?;
                Ok(())
            }
            _ => Err(eyre::eyre!("Invalid identity")),
        }
    }

    pub async fn new_quick_connect(&self) -> Result<QuickConnectSession, reqwest::Error> {
        let url = format!("{}/QuickConnect/Initiate", self.base_url);
        let response: types::QuickConnectResult = self
            .client
            .get(&url)
            .header("X-Emby-Authorization", emby_authorization(None))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(QuickConnectSession {
            client: self.clone(),
            secret: response.secret.expect("No secret in QuickConnectResult"),
            code: response.code.expect("No code in QuickConnectResult"),
        })
    }

    // POST /Users/authenticatebyname
    // "{\"Username\":\"root\",\"Pw\":\"root\"}"
    // Must be 200
    // "{\"User\":{\"Name\":\"root\",\"ServerId\":\"3dd289f817a84b2cb14aaf7613315acf\",\"Id\":\"e7334e423e4f42b1a7cae972dd869ef3\",\"HasPassword\":true,\"HasConfiguredPassword\":true,\"HasConfiguredEasyPassword\":false,\"EnableAutoLogin\":false,\"LastLoginDate\":\"2024-06-30T14:05:51.6232672Z\",\"LastActivityDate\":\"2024-06-30T14:05:51.6232672Z\",\"Configuration\":{\"PlayDefaultAudioTrack\":true,\"SubtitleLanguagePreference\":\"\",\"DisplayMissingEpisodes\":false,\"GroupedFolders\":[],\"SubtitleMode\":\"Default\",\"DisplayCollectionsView\":false,\"EnableLocalPassword\":false,\"OrderedViews\":[],\"LatestItemsExcludes\":[],\"MyMediaExcludes\":[],\"HidePlayedInLatest\":true,\"RememberAudioSelections\":true,\"RememberSubtitleSelections\":true,\"EnableNextEpisodeAutoPlay\":true,\"CastReceiverId\":\"F007D354\"},\"Policy\":{\"IsAdministrator\":true,\"IsHidden\":true,\"EnableCollectionManagement\":false,\"EnableSubtitleManagement\":false,\"EnableLyricManagement\":false,\"IsDisabled\":false,\"BlockedTags\":[],\"AllowedTags\":[],\"EnableUserPreferenceAccess\":true,\"AccessSchedules\":[],\"BlockUnratedItems\":[],\"EnableRemoteControlOfOtherUsers\":true,\"EnableSharedDeviceControl\":true,\"EnableRemoteAccess\":true,\"EnableLiveTvManagement\":true,\"EnableLiveTvAccess\":true,\"EnableMediaPlayback\":true,\"EnableAudioPlaybackTranscoding\":true,\"EnableVideoPlaybackTranscoding\":true,\"EnablePlaybackRemuxing\":true,\"ForceRemoteSourceTranscoding\":false,\"EnableContentDeletion\":true,\"EnableContentDeletionFromFolders\":[],\"EnableContentDownloading\":true,\"EnableSyncTranscoding\":true,\"EnableMediaConversion\":true,\"EnabledDevices\":[],\"EnableAllDevices\":true,\"EnabledChannels\":[],\"EnableAllChannels\":true,\"EnabledFolders\":[],\"EnableAllFolders\":true,\"InvalidLoginAttemptCount\":0,\"LoginAttemptsBeforeLockout\":-1,\"MaxActiveSessions\":0,\"EnablePublicSharing\":true,\"BlockedMediaFolders\":[],\"BlockedChannels\":[],\"RemoteClientBitrateLimit\":0,\"AuthenticationProviderId\":\"Jellyfin.Server.Implementations.Users.DefaultAuthenticationProvider\",\"PasswordResetProviderId\":\"Jellyfin.Server.Implementations.Users.DefaultPasswordResetProvider\",\"SyncPlayAccess\":\"CreateAndJoinGroups\"}},\"SessionInfo\":{\"PlayState\":{\"CanSeek\":false,\"IsPaused\":false,\"IsMuted\":false,\"RepeatMode\":\"RepeatNone\",\"PlaybackOrder\":\"Default\"},\"AdditionalUsers\":[],\"Capabilities\":{\"PlayableMediaTypes\":[],\"SupportedCommands\":[],\"SupportsMediaControl\":false,\"SupportsPersistentIdentifier\":true,\"SupportsContentUploading\":false,\"SupportsSync\":false},\"RemoteEndPoint\":\"172.17.0.1\",\"PlayableMediaTypes\":[],\"Id\":\"457d7db0ba577de797fa3abeec732e2c\",\"UserId\":\"e7334e423e4f42b1a7cae972dd869ef3\",\"UserName\":\"root\",\"Client\":\"Jellyfin Web\",\"LastActivityDate\":\"2024-06-30T14:05:51.7015338Z\",\"LastPlaybackCheckIn\":\"0001-01-01T00:00:00.0000000Z\",\"DeviceName\":\"Firefox\",\"DeviceId\":\"TW96aWxsYS81LjAgKFgxMTsgTGludXggeDg2XzY0OyBydjoxMjcuMCkgR2Vja28vMjAxMDAxMDEgRmlyZWZveC8xMjcuMHwxNzE5NzU2MjE4MDUy\",\"ApplicationVersion\":\"10.9.7\",\"IsActive\":true,\"SupportsMediaControl\":false,\"SupportsRemoteControl\":false,\"NowPlayingQueue\":[],\"NowPlayingQueueFullItems\":[],\"HasCustomDeviceName\":false,\"ServerId\":\"3dd289f817a84b2cb14aaf7613315acf\",\"SupportedCommands\":[]},\"AccessToken\":\"a9229457f4304b45852813e4d5803d20\",\"ServerId\":\"3dd289f817a84b2cb14aaf7613315acf\"}"
    pub async fn authenticate(
        &self,
        user: &str,
        pass: &str,
    ) -> Result<JellyfinUser, reqwest::Error> {
        let url = format!("{}/Users/authenticatebyname", self.base_url);
        let response: types::AuthenticationResult = self
            .client
            .post(&url)
            .json(&AuthenticateUserByName {
                username: Some(user.to_string()),
                pw: Some(pass.to_string()),
                password: None,
            })
            .header("X-Emby-Authorization", emby_authorization(None))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let user = JellyfinUser {
            client: self.clone(),
            id: response
                .user
                .as_ref()
                .expect("No user_id in AuthenticationResult")
                .id
                .expect("No id in User")
                .to_string(),
            token: response
                .access_token
                .expect("No access_token in AuthenticationResult"),
        };
        Ok(user)
    }

    // Authorization: "MediaBrowser Client=\"Jellyfin Web\", Device=\"Firefox\", DeviceId=\"TW96aWxsYS81LjAgKFgxMTsgTGludXggeDg2XzY0OyBydjoxMjcuMCkgR2Vja28vMjAxMDAxMDEgRmlyZWZveC8xMjcuMHwxNzE5NzU2MjE4MDUy\", Version=\"10.9.7\", Token=\"a9229457f4304b45852813e4d5803d20\""

    pub fn resume_quick_connect(&self, secret: &str, code: &str) -> QuickConnectSession {
        QuickConnectSession {
            client: self.clone(),
            secret: secret.to_string(),
            code: code.to_string(),
        }
    }

    pub fn resume_user(&self, id: &str, token: &str) -> JellyfinUser {
        JellyfinUser {
            client: self.clone(),
            id: id.to_string(),
            token: token.to_string(),
        }
    }

    pub async fn complete_startup(
        &self,
        pass: &str,
        media: Option<&str>,
    ) -> Result<(), eyre::Error> {
        let startup = Startup::new(self.clone(), pass.to_string());
        startup.configuration().await?;
        startup.user().await?;
        if let Some(media) = media {
            startup
                .add_tvshow_library("Test Shows Library", media)
                .await?;
        }
        startup.remote_access().await?;
        startup.complete().await?;
        Ok(())
    }

    pub async fn preferences(
        &self,
        _identity: &serde_json::Value,
        _new_preferences: Option<serde_json::Value>,
    ) -> Result<Option<serde_json::Value>, eyre::Error> {
        todo!()
    }
}

pub struct Startup {
    client: Jellyfin,
    pub pass: String,
}

impl Startup {
    pub fn new(client: Jellyfin, pass: String) -> Self {
        Self { client, pass }
    }

    // POST /Startup/Configuration
    // "{\"UICulture\":\"en-US\",\"MetadataCountryCode\":\"US\",\"PreferredMetadataLanguage\":\"en\"}"
    // Must be 204
    pub async fn configuration(&self) -> Result<(), eyre::Error> {
        let url = format!("{}/Startup/Configuration", self.client.base_url);
        let response = self
            .client
            .client
            .post(&url)
            .json(&serde_json::json!({
                "UICulture": "en-US",
                "MetadataCountryCode": "US",
                "PreferredMetadataLanguage": "en",
            }))
            .header("X-Emby-Authorization", emby_authorization(None))
            .send()
            .await?;
        if response.status() == StatusCode::NO_CONTENT {
            Ok(())
        } else {
            Err(eyre::eyre!("Invalid status code: {}", response.status()))
        }
    }

    // GET /Startup/User
    // POST /Startup/User
    // "{\"Name\":\"root\",\"Password\":\"root\"}"
    // Must be 204
    pub async fn user(&self) -> Result<(), eyre::Error> {
        let url = format!("{}/Startup/User", self.client.base_url);
        let response = self
            .client
            .client
            .get(&url)
            .header("X-Emby-Authorization", emby_authorization(None))
            .send()
            .await?;
        let startup_user: types::StartupUserDto = response.json().await?;

        let response = self
            .client
            .client
            .post(&url)
            .json(&serde_json::json!({
                "Name": startup_user.name.unwrap(),
                "Password": self.pass,
            }))
            .header("X-Emby-Authorization", emby_authorization(None))
            .send()
            .await?;
        if response.status() == StatusCode::NO_CONTENT {
            Ok(())
        } else {
            Err(eyre::eyre!(
                "Invalid status code: {}\n{}",
                response.status(),
                response.text().await?
            ))
        }
    }

    // POST /Library/VirtualFolders?collectionType=tvshows&refreshLibrary=false&name=Shows
    // a
    // Must be 204
    pub async fn add_tvshow_library(&self, name: &str, media: &str) -> Result<(), eyre::Error> {
        let url = format!(
            "{}/Library/VirtualFolders?collectionType=tvshows&refreshLibrary=true&name={}",
            self.client.base_url, name,
        );
        let response = self.client.client.post(&url).json(&serde_json::json!({
            "LibraryOptions": {
                "Enabled": true,
                "EnableArchiveMediaFiles": false,
                "EnablePhotos": true,
                "EnableRealtimeMonitor": true,
                "EnableLUFSScan": true,
                "ExtractTrickplayImagesDuringLibraryScan": false,
                "EnableTrickplayImageExtraction": false,
                "ExtractChapterImagesDuringLibraryScan": false,
                "EnableChapterImageExtraction": false,
                "EnableInternetProviders": true,
                "SaveLocalMetadata": false,
                "EnableAutomaticSeriesGrouping": false,
                "PreferredMetadataLanguage": "",
                "MetadataCountryCode": "",
                "SeasonZeroDisplayName": "Specials",
                "AutomaticRefreshIntervalDays": 0,
                "EnableEmbeddedTitles": false,
                "EnableEmbeddedExtrasTitles": false,
                "EnableEmbeddedEpisodeInfos": false,
                "AllowEmbeddedSubtitles": "AllowAll",
                "SkipSubtitlesIfEmbeddedSubtitlesPresent": false,
                "SkipSubtitlesIfAudioTrackMatches": false,
                "SaveSubtitlesWithMedia": true,
                "SaveLyricsWithMedia": false,
                "RequirePerfectSubtitleMatch": true,
                "AutomaticallyAddToCollection": false,
                "MetadataSavers": [],
                "TypeOptions": [
                    {
                        "Type": "Series",
                        "MetadataFetchers": [],
                        "MetadataFetcherOrder": ["TheMovieDb", "The Open Movie Database"],
                        "ImageFetchers": [],
                        "ImageFetcherOrder": ["TheMovieDb"]
                    },
                    {
                        "Type": "Season",
                        "MetadataFetchers": [],
                        "MetadataFetcherOrder": ["TheMovieDb"],
                        "ImageFetchers": [],
                        "ImageFetcherOrder": ["TheMovieDb"]
                    },
                    {
                        "Type": "Episode",
                        "MetadataFetchers": [],
                        "MetadataFetcherOrder": ["TheMovieDb", "The Open Movie Database"],
                        "ImageFetchers": [],
                        "ImageFetcherOrder": ["TheMovieDb", "The Open Movie Database", "Embedded Image Extractor", "Screen Grabber"]
                    }
                ],
                "LocalMetadataReaderOrder": ["Nfo"],
                "SubtitleDownloadLanguages": [],
                "DisabledSubtitleFetchers": [],
                "SubtitleFetcherOrder": [],
                "PathInfos": [
                    {
                        "Path": media,
                    }
                ],
            }
        })).header("X-Emby-Authorization", emby_authorization(None)).send().await?;
        if response.status() == StatusCode::NO_CONTENT {
            Ok(())
        } else {
            Err(eyre::eyre!("Invalid status code: {}", response.status()))
        }
    }

    // POST /Startup/RemoteAccess
    // "{\"EnableRemoteAccess\":true,\"EnableAutomaticPortMapping\":false}"
    // Must be 204
    pub async fn remote_access(&self) -> Result<(), eyre::Error> {
        let url = format!("{}/Startup/RemoteAccess", self.client.base_url);
        let response = self
            .client
            .client
            .post(&url)
            .json(&serde_json::json!({
                "EnableRemoteAccess": true,
                "EnableAutomaticPortMapping": false,
            }))
            .header("X-Emby-Authorization", emby_authorization(None))
            .send()
            .await?;
        if response.status() == StatusCode::NO_CONTENT {
            Ok(())
        } else {
            Err(eyre::eyre!("Invalid status code: {}", response.status()))
        }
    }

    // POST /Startup/Complete
    // (Empty Request body)
    // Must be 204
    pub async fn complete(&self) -> Result<(), eyre::Error> {
        let url = format!("{}/Startup/Complete", self.client.base_url);
        let response = self
            .client
            .client
            .post(&url)
            .header("X-Emby-Authorization", emby_authorization(None))
            .send()
            .await?;
        if response.status() == StatusCode::NO_CONTENT {
            Ok(())
        } else {
            Err(eyre::eyre!("Invalid status code: {}", response.status()))
        }
    }
}

#[derive(Clone)]
pub struct QuickConnectSession {
    client: Jellyfin,
    pub secret: String,
    pub code: String,
}

impl QuickConnectSession {
    pub async fn poll(&self) -> Result<bool, reqwest::Error> {
        let url = format!(
            "{}/QuickConnect/Connect?Secret={}",
            self.client.base_url, self.secret
        );
        let response: types::QuickConnectResult = self
            .client
            .client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response.authenticated.unwrap_or_default())
    }

    pub async fn auth(&self) -> Result<JellyfinUser, reqwest::Error> {
        let url = format!(
            "{}/Users/AuthenticateWithQuickConnect",
            self.client.base_url
        );
        let response: types::AuthenticationResult = self
            .client
            .client
            .post(&url)
            .json(&types::QuickConnectDto {
                secret: self.secret.clone(),
            })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let user = JellyfinUser {
            client: self.client.clone(),
            id: response
                .user
                .as_ref()
                .expect("No user_id in AuthenticationResult")
                .id
                .expect("No id in User")
                .to_string(),
            token: response
                .access_token
                .expect("No access_token in AuthenticationResult"),
        };
        let caps_url = format!("{}/Sessions/Capabilities/Full", self.client.base_url);
        self.client.client.post(&caps_url).json(&types::ClientCapabilitiesDto{
            // These don't actually seem to do anything at all...
            app_store_url: Some("https://github.com/alyti/jellyvr/".to_string()),
            icon_url: Some("https://raw.githubusercontent.com/alyti/jellyvr/main/assets/images/jellyfin-jellyvr-logo.svg".to_string()),
            device_profile: None, //Some(DeviceProfile{}),
            message_callback_url: None,
            playable_media_types: vec!["Video".to_string()],
            supported_commands: vec![],
            supports_content_uploading: Some(false),
            supports_media_control: Some(false),
            supports_persistent_identifier: Some(false),
            supports_sync: Some(false),
        }).header("X-Emby-Authorization", emby_authorization(Some(&user.token))).send().await?.error_for_status()?;
        Ok(user)
    }

    fn to_step(&self) -> SetupStep {
        SetupStep::QcPoll {
            secret: self.secret.clone(),
            code: self.code.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct JellyfinUser {
    client: Jellyfin,
    pub id: String,
    pub token: String,
}

impl JellyfinUser {
    fn to_step(&self) -> SetupStep {
        SetupStep::Auth {
            id: self.id.clone(),
            token: self.token.clone(),
        }
    }

    // POST /QuickConnect/Authorize?code=000000&userId=e7334e423e4f42b1a7cae972dd869ef3
    pub async fn authorize(&self, code: &str) -> Result<(), reqwest::Error> {
        let url = format!(
            "{}/QuickConnect/Authorize?code={}&userId={}",
            self.client.base_url, code, self.id
        );
        self.client
            .client
            .post(&url)
            .header(
                "X-Emby-Authorization",
                emby_authorization(Some(&self.token)),
            )
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn whoami(&self) -> Result<types::UserDto, reqwest::Error> {
        let url = format!("{}/Users/{}", self.client.base_url, self.id);
        let response: types::UserDto = self
            .client
            .client
            .get(&url)
            .header(
                "X-Emby-Authorization",
                emby_authorization(Some(&self.token)),
            )
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response)
    }

    pub async fn views(&self) -> Result<Vec<Library>, reqwest::Error> {
        let url = format!("{}/Users/{}/Views", self.client.base_url, self.id);
        let response: types::BaseItemDtoQueryResult = self
            .client
            .client
            .get(&url)
            .header(
                "X-Emby-Authorization",
                emby_authorization(Some(&self.token)),
            )
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response
            .items
            .expect("No items in BaseItemDtoQueryResult")
            .into_iter()
            .map(|view| view.into())
            .collect())
    }

    pub async fn items(&self, library: Option<Library>) -> Result<Vec<Item>, reqwest::Error> {
        match library {
            None => self
                .views()
                .await
                .map(|views| views.into_iter().map(Item::Library).collect()),
            Some(lib) => {
                let url = format!("{}/Users/{}/Items", self.client.base_url, self.id);
                let query: &[(&str, &str)] = &[
                    ("SortBy", "IsFolder,SortName,ProductionYear"),
                    ("SortOrder", "Ascending"),
                    // ("IncludeItemTypes", "Movie,Episode".into()),
                    // ("Recursive", "true".into()),
                    ("ParentId", &lib.id),
                    ("Fields", "ParentId,DateCreated,MediaSources,MediaStreams,BasicSyncInfo,Genres,Tags,Studios,SeriesStudio,People,Chapters,ChildCount,MediaSourceCount,Overview"),
                    ("ImageTypeLimit", "1"),
                    ("EnableImageTypes", "Primary,Backdrop"),
                    ("StartIndex", "0"),
                    ("IsMissing", "false")
                ];
                let response: types::BaseItemDtoQueryResult = self
                    .client
                    .client
                    .get(&url)
                    .query(query)
                    .header(
                        "X-Emby-Authorization",
                        emby_authorization(Some(&self.token)),
                    )
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                Ok(response
                    .items
                    .unwrap_or_default()
                    .into_iter()
                    .map(|item| item.into())
                    .collect())
            }
        }
    }

    pub async fn item(&self, id: &str) -> Result<Item, reqwest::Error> {
        let url = format!("{}/Users/{}/Items/{}", self.client.base_url, self.id, id);
        let query: &[(&str, &str)] = &[
            ("Fields", "ParentId,DateCreated,MediaSources,MediaStreams,BasicSyncInfo,Genres,Tags,Studios,SeriesStudio,People,Chapters,ChildCount,MediaSourceCount,Overview"),
            ("ImageTypeLimit", "1"),
            ("EnableImageTypes", "Primary,Backdrop"),
        ];
        let response: types::BaseItemDto = self
            .client
            .client
            .get(&url)
            .query(query)
            .header(
                "X-Emby-Authorization",
                emby_authorization(Some(&self.token)),
            )
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response.into())
    }

    pub async fn transcode(
        &self,
        content: &Content,
        profile: serde_json::Value,
        preferred_media_streams: &[MediaStream],
    ) -> Result<TranscodeJob, eyre::Error> {
        let audio_index = preferred_media_streams
            .iter()
            .filter_map(|stream| match stream {
                MediaStream::Audio { index, .. } => Some(index),
                _ => None,
            })
            .next();
        let subtitle_index = preferred_media_streams
            .iter()
            .filter_map(|stream| match stream {
                MediaStream::Subtitle { index, .. } => Some(index),
                _ => None,
            })
            .next();
        let url = format!(
            "{}/Items/{}/PlaybackInfo",
            self.client.base_url, &content.id
        );
        let pretty = serde_json::to_string_pretty(&profile)
            .map_err(|e| eyre::eyre!("Failed to serialize profile: {}", e))?;
        let query = PlaybackQuery::new(&self.id, audio_index.copied(), subtitle_index.copied());
        tracing::info!(
            "Transcoding with profile: {} and query: {:?}",
            pretty,
            query
        );
        let response: types::PlaybackInfoResponse = self
            .client
            .client
            .post(&url)
            .query(&query)
            .json(&profile)
            .header(
                "X-Emby-Authorization",
                emby_authorization(Some(&self.token)),
            )
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        tracing::info!(name: "jellyfin_resp", ?response);

        let path = response
            .media_sources
            .into_iter()
            .filter_map(|source| source.transcoding_url)
            .next()
            .expect("No transcoding url in PlaybackInfoResponse");
        let path = format!("{}{}", self.client.base_url, path);

        let manifest = self
            .client
            .client
            .get(&path)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let mut master = m3u8_rs::parse_master_playlist_res(&manifest)
            .map_err(|e| eyre::eyre!("Failed to parse master playlist: {}", e))?;
        let mut media = HashMap::new();
        for variant in master.variants.iter_mut() {
            let name = if let Some(res) = variant.resolution.as_ref() {
                format!("{}p", res)
            } else {
                "unknown".to_string()
            };
            let manifest_media = self
                .client
                .client
                .get(format!(
                    "{}/videos/{}/{}",
                    self.client.base_url, &content.id, variant.uri
                ))
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            variant.uri = format!("{}.m3u8", name);
            let mut media_playlist = m3u8_rs::parse_media_playlist_res(&manifest_media)
                .map_err(|e| eyre::eyre!("Failed to parse media playlist: {}", e))?;

            media_playlist.segments.iter_mut().for_each(|variant| {
                variant.uri = format!(
                    "{}/videos/{}/{}",
                    self.client.base_url, &content.id, variant.uri
                );
            });
            media.insert(name, media_playlist);
        }

        Ok(TranscodeJob::M3U8(M3U8Playlist {
            media,
            main: master,
        }))
    }
}

#[derive(Serialize, Debug, Clone)]
struct PlaybackQuery {
    #[serde(rename = "UserId")]
    user_id: String,
    #[serde(rename = "AudioStreamIndex")]
    audio_stream_index: Option<i32>,
    #[serde(rename = "SubtitleStreamIndex")]
    subtitle_stream_index: Option<i32>,

    #[serde(rename = "StartTimeTicks")]
    start_time_ticks: Option<i64>,
    #[serde(rename = "IsPlayback")]
    is_playback: Option<bool>,
    #[serde(rename = "AutoOpenLiveStream")]
    auto_open_live_stream: Option<bool>,
    #[serde(rename = "MediaSourceId")]
    media_source_id: Option<String>,
    #[serde(rename = "MaxStreamingBitrate")]
    max_streaming_bitrate: Option<i64>,
}

impl PlaybackQuery {
    fn new(user: &str, audio: Option<i32>, subtitle: Option<i32>) -> Self {
        Self {
            user_id: user.to_string(),
            audio_stream_index: audio,
            subtitle_stream_index: subtitle,
            start_time_ticks: None,
            is_playback: Some(true),
            auto_open_live_stream: Some(true),
            max_streaming_bitrate: Some(140_000_000),
            media_source_id: None,
        }
    }
}

impl From<BaseItemDto> for Item {
    fn from(item: BaseItemDto) -> Self {
        if item.is_folder.unwrap_or_default() {
            Item::Library(item.into())
        } else {
            Item::Content(item.into())
        }
    }
}

impl From<BaseItemDto> for Library {
    fn from(item: BaseItemDto) -> Self {
        Library {
            id: item.id.expect("No id in ViewDto").to_string(),
            parent_id: item.parent_id.map(|id| id.to_string()),
            name: item.name.expect("No name in ViewDto"),
            description: None,
            icon_url: Some(match item.type_.unwrap() {
                BaseItemKind::Season | BaseItemKind::CollectionFolder | BaseItemKind::Folder => {
                    format!(
                        "/Items/{}/Images/Primary?maxHeight=300&maxWidth=300&quality=90",
                        item.id.expect("No id in ViewDto")
                    )
                }
                _ => format!(
                    "/Items/{}/Images/Backdrop?maxHeight=300&maxWidth=300&quality=90",
                    item.id.expect("No id in ViewDto")
                ),
            }),
            kind: match item.type_.unwrap() {
                BaseItemKind::Folder => LibraryKind::Folder,
                BaseItemKind::Season => LibraryKind::Season {
                    season: item.index_number.unwrap_or_default(),
                },
                BaseItemKind::Series => LibraryKind::Show,
                BaseItemKind::CollectionFolder => LibraryKind::Collection,
                x => LibraryKind::Other {
                    name: Some(x.to_string()),
                },
            },
        }
    }
}

impl From<BaseItemDto> for Content {
    fn from(item: BaseItemDto) -> Self {
        let id = item.id.expect("No id in BaseItemDto").to_string();
        let name = item.name.expect("No name in BaseItemDto");
        let description = item.overview;
        let icon_url = Some(match item.type_.unwrap() {
            BaseItemKind::Movie => {
                format!("/Items/{id}/Images/Backdrop?maxHeight=300&maxWidth=300&quality=90",)
            }
            _ => format!("/Items/{id}/Images/Primary?maxHeight=300&maxWidth=300&quality=90",),
        });
        Self {
            id,
            parent_id: item.parent_id.map(|id| id.to_string()),
            name,
            description,
            icon_url,
            media_streams: item
                .media_streams
                .expect("no media streams")
                .into_iter()
                .filter_map(|stream| match &stream.type_.expect("no media type") {
                    types::MediaStreamType::Video => Some(MediaStream::Video {
                        index: stream.index.expect("no index"),
                        codec: stream.codec.expect("no codec"),
                    }),
                    types::MediaStreamType::Audio => Some(MediaStream::Audio {
                        index: stream.index.expect("no index"),
                        codec: stream.codec.expect("no codec"),
                        language: stream.language,
                        name: stream.title,
                    }),
                    types::MediaStreamType::Subtitle => Some(MediaStream::Subtitle {
                        index: stream.index.expect("no index"),
                        codec: stream.codec.expect("no codec"),
                        language: stream.language,
                        name: stream.title,
                    }),
                    _ => None,
                })
                .collect(),
            kind: match item.type_.expect("no type") {
                BaseItemKind::Movie => ContentKind::Movie,
                BaseItemKind::Episode => ContentKind::Episode {
                    season: item.parent_index_number,
                    episode: item.index_number.unwrap_or_default(),
                },
                x => ContentKind::Other {
                    name: Some(x.to_string()),
                },
            },
        }
    }
}
