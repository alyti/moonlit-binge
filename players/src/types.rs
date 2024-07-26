use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Library {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub kind: LibraryKind,
}

impl Library {
    #[must_use]
    pub fn from_path(id: &str) -> Self {
        Self {
            id: id.to_string(),
            parent_id: None,
            name: "From Path".to_string(),
            description: None,
            icon_url: None,
            kind: LibraryKind::Collection,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Content {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub media_streams: Vec<MediaStream>,
    pub kind: ContentKind,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum MediaStream {
    Video {
        index: i32,
        codec: String,
    },
    Audio {
        index: i32,
        codec: String,
        language: Option<String>,
        name: Option<String>,
    },
    Subtitle {
        index: i32,
        codec: String,
        language: Option<String>,
        name: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum LibraryKind {
    Collection,
    Folder,
    Show,
    Season { season: i32 },
    Other { name: Option<String> },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum ContentKind {
    Movie,
    Episode { season: Option<i32>, episode: i32 },
    Other { name: Option<String> },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Item {
    Library(Library),
    Content(Content),
}

#[derive(Debug, Clone, PartialEq)]
pub struct M3U8Playlist {
    pub main: m3u8_rs::MasterPlaylist,
    pub media: HashMap<String, m3u8_rs::MediaPlaylist>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TranscodeJob {
    M3U8(M3U8Playlist),
}

impl Content {
    /// Generate a sort key for the content.
    pub fn sort_key(&self) -> i64 {
        let mut hasher = std::hash::DefaultHasher::new();

        // self.parent_id.hash(&mut hasher);
        match self.kind {
            ContentKind::Episode { season, episode } => {
                let season = season.unwrap_or(0);
                return (season * 10000 + episode).try_into().unwrap();
                //.hash(&mut hasher);
            }
            ContentKind::Movie | ContentKind::Other { .. } => {
                self.parent_id.hash(&mut hasher);
                self.name.hash(&mut hasher);
            }
        };

        let finish = hasher.finish();
        match finish.try_into() {
            Ok(sort_key) => sort_key,
            Err(_) => 0i64.wrapping_sub_unsigned(finish),
        }
    }
}
