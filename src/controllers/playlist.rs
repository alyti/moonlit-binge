use std::collections::HashMap;

use axum::debug_handler;
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{common::settings::SETTINGS, models::_entities::users, views::user::CurrentResponse};

#[derive(Debug, Deserialize, Serialize)]
struct PlaylistCreateParams {
    name: String,
    contents: Vec<SingleContent>,
}

#[derive(Debug, Deserialize, Serialize)]
struct SingleContent {
    connection: i32,
    content_id: String,
}

#[debug_handler]
async fn splice(
    // auth: auth::JWTWithUser<users::Model>,
    State(ctx): State<AppContext>,
    Json(params): Json<PlaylistCreateParams>,
) -> Result<impl IntoResponse> {
    // validate the playlists exist...
    let transcoding_base_path = &SETTINGS.get().unwrap().transcoding_dir;
    let single_base_path = transcoding_base_path.join("single");
    for content in &params.contents {
        let dir = tokio::fs::try_exists(
            single_base_path
                .join(format!("{}", content.connection))
                .join(&content.content_id)
                .join("main.m3u8"),
        )
        .await?;
        if !dir {
            return Err(Error::BadRequest("Content does not exist".to_string()));
        }
    }

    let mut spliced_master: Option<m3u8_rs::MasterPlaylist> = None;
    let mut spliced_media: Option<HashMap<String, m3u8_rs::MediaPlaylist>> = None;

    for content in params.contents.into_iter() {
        let single_content_dir = single_base_path
            .join(format!("{}", content.connection))
            .join(&content.content_id);
        let mut playlist = tokio::fs::read(single_content_dir.join("main.m3u8")).await?;
        let manifest = m3u8_rs::parse_master_playlist_res(&playlist).unwrap();
        spliced_master = match spliced_master {
            Some(mut spliced_master) => {
                if spliced_master.variants.len() != manifest.variants.len() {
                    return Err(Error::BadRequest("Media count mismatch".to_string()));
                }
                Some(spliced_master)
            }
            None => Some(manifest.clone()),
        };

        for variant in manifest.variants.into_iter() {
            let playlist = tokio::fs::read(single_content_dir.join(&variant.uri)).await?;
            let mut media = m3u8_rs::parse_media_playlist_res(&playlist).unwrap();
            let name = variant.uri.strip_suffix(".m3u8").unwrap();

            for segment in media.segments.iter_mut() {
                segment.uri = format!(
                    "../../single/{}/{}/{}",
                    content.connection, content.content_id, segment.uri
                );
                segment.discontinuity = true;
            }

            spliced_media = match spliced_media {
                Some(mut spliced_media) => {
                    if let Some(spliced_media) = spliced_media.get_mut(name) {
                        // media.segments.first_mut().unwrap().discontinuity = true;
                        spliced_media.segments.extend(media.segments);
                    } else {
                        return Err(Error::BadRequest("Missing media playlist".to_string()));
                    }
                    Some(spliced_media)
                }
                None => {
                    let mut map = HashMap::new();
                    map.insert(name.to_string(), media);
                    Some(map)
                }
            };
        }
    }

    let playlist_base_path = transcoding_base_path.join("playlist").join(params.name);
    tokio::fs::create_dir_all(&playlist_base_path).await?;
    let mut v: Vec<u8> = Vec::new();
    spliced_master
        .expect("Missing spliced main file")
        .write_to(&mut v)
        .unwrap();
    tokio::fs::write(playlist_base_path.join("main.m3u8"), v).await?;

    for (name, media) in spliced_media
        .expect("Missing spliced media files")
        .iter_mut()
    {
        let mut v: Vec<u8> = Vec::new();
        media.write_to(&mut v).unwrap();
        tokio::fs::write(playlist_base_path.join(format!("{}.m3u8", name)), v).await?;
    }
    Ok(())
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/playlist")
        .add("/splice", post(splice))
}
