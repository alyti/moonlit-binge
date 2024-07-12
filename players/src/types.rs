use std::{collections::HashMap, hash::Hash};

use serde::{Deserialize, Serialize};

use crate::jellyfin::types::BaseItem;


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Library {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub kind: LibraryKind,
}

impl Library {
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Content {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub media_streams: Vec<MediaStream>,
    pub kind: ContentKind,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum MediaStream {
    Video{
        index: i32,
        codec: String,
    },
    Audio{
        index: i32,
        codec: String,
        language: Option<String>,
        name: Option<String>,
    },
    Subtitle{
        index: i32,
        codec: String,
        language: Option<String>,
        name: Option<String>,
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum LibraryKind {
    Collection,
    Folder,
    Show,
    Season {
        season: i32,
    },
    Other {
        name: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum ContentKind {
    Movie,
    Episode {
        season: Option<i32>,
        episode: i32,
    },
    Other {
        name: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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
