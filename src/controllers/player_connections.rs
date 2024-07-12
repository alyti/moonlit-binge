#![allow(clippy::missing_errors_doc)]
#![allow(clippy::unnecessary_struct_initialization)]
#![allow(clippy::unused_async)]
use axum::{body::Bytes, debug_handler, Extension};
use axum_extra::extract::{Form, Query};
use axum_htmx::HxRequest;
use futures_util::StreamExt;
use loco_rs::prelude::*;
use players::types::{Item, Library, MediaStream};
use sea_orm::{sea_query::Order, QueryOrder};
use serde::{Deserialize, Serialize};

use crate::{
    initializers::{
        media_provider::{ConnectedMediaProvider, MediaProviders},
        view_engine::BetterTeraView,
    },
    models::_entities::{
        player_connections::{ActiveModel, Column, Entity, Model},
        users,
    },
    views,
    workers::downloader::{DownloadWorker, DownloadWorkerArgs},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Params {
    pub media_provider_id: String,
    pub identity: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetupParams {
    pub media_provider_id: String,
    pub setup: Option<String>,
}

async fn load_item(ctx: &AppContext, id: i32) -> Result<Model> {
    let item = Entity::find_by_id(id).one(&ctx.db).await?;
    item.ok_or_else(|| Error::NotFound)
}

#[debug_handler]
pub async fn new(
    ViewEngine(v): ViewEngine<BetterTeraView>,
    State(ctx): State<AppContext>,
    Extension(media_providers): Extension<Box<MediaProviders>>,
    HxRequest(boosted): HxRequest,
) -> Result<Response> {
    views::player_connections::base_view(
        &v,
        boosted,
        "create",
        &serde_json::json!({"providers": &media_providers}),
    )
}

#[debug_handler]
pub async fn setup(
    ViewEngine(v): ViewEngine<BetterTeraView>,
    State(ctx): State<AppContext>,
    Extension(media_providers): Extension<Box<MediaProviders>>,
    Json(params): Json<SetupParams>,
) -> Result<Response> {
    let provider = media_providers
        .get(&params.media_provider_id)
        .ok_or_else(|| Error::NotFound)?;
    let setup: Option<serde_json::Value> = params
        .setup
        .as_ref()
        .map(|s| serde_json::from_str(s).unwrap());

    views::player_connections::setup(&v, &provider, provider.setup(&ctx, setup).await?)
}

#[debug_handler]
pub async fn add(
    ViewEngine(v): ViewEngine<BetterTeraView>,
    State(ctx): State<AppContext>,
    Extension(media_providers): Extension<Box<MediaProviders>>,
    auth: auth::JWTWithUser<users::Model>,
    Json(params): Json<Params>,
) -> Result<Response> {
    let provider = media_providers
        .get(&params.media_provider_id)
        .ok_or_else(|| Error::NotFound)?
        .clone();
    let identity: serde_json::Value = serde_json::from_str(&params.identity)?;
    if ConnectedMediaProvider::from_provider_and_connection(provider, identity.clone())
        .test(&ctx)
        .await
        .is_err()
    {
        return Err(Error::BadRequest("Invalid identity".to_string()));
    }
    let item = ActiveModel {
        user_id: Set(auth.user.id),
        media_provider_id: Set(params.media_provider_id.clone()),
        identity: Set(Some(identity.clone())),
        ..Default::default()
    };
    let item = item.insert(&ctx.db).await?;
    let provider: ConnectedMediaProvider = item.clone().try_into()?;
    let items = provider.items(&ctx, None).await?;
    views::player_connections::show(&v, &provider.provider, &item, items)
}

#[debug_handler]
pub async fn show(
    Path(id): Path<i32>,
    ViewEngine(v): ViewEngine<BetterTeraView>,
    HxRequest(boosted): HxRequest,
    State(ctx): State<AppContext>,
) -> Result<Response> {
    let connection = load_item(&ctx, id).await?;
    let provider: ConnectedMediaProvider = connection.clone().try_into()?;
    let items = provider.items(&ctx, None).await?;
    views::player_connections::base_view(
        &v,
        boosted,
        "show",
        &serde_json::json!({"provider": &provider.provider, "connection": &connection, "items": items}),
    )
}

#[debug_handler]
pub async fn show_library(
    Path((id, library)): Path<(i32, String)>,
    ViewEngine(v): ViewEngine<BetterTeraView>,
    HxRequest(boosted): HxRequest,
    State(ctx): State<AppContext>,
) -> Result<Response> {
    let connection = load_item(&ctx, id).await?;
    let provider: ConnectedMediaProvider = connection.clone().try_into()?;

    let parent = provider.item(&ctx, &library).await?;
    let items = provider
        .items(&ctx, Some(Library::from_path(&library)))
        .await?;
    views::player_connections::base_view(
        &v,
        boosted,
        "show",
        &serde_json::json!({"provider": &provider.provider, "connection": &connection, "parent": parent, "items": items}),
    )
}

#[derive(Deserialize)]
pub struct TranscodeInitParams {
    #[serde(rename = "content")]
    content_ids: Vec<String>,
}

pub async fn transcode(
    Path(connection_id): Path<i32>,
    ViewEngine(v): ViewEngine<BetterTeraView>,
    HxRequest(boosted): HxRequest,
    State(ctx): State<AppContext>,
    Query(data): Query<TranscodeInitParams>,
) -> Result<Response> {
    let connection = load_item(&ctx, connection_id).await?;
    let provider: ConnectedMediaProvider = connection.clone().try_into()?;
    let mut items = vec![];
    for content_id in data.content_ids {
        let item = provider.item(&ctx, &content_id).await?;
        items.push(item);
    }

    views::player_connections::base_view(
        &v,
        boosted,
        "transcode",
        &serde_json::json!({"provider": &provider.provider, "connection": &connection, "items": items}),
    )
}

#[derive(Deserialize, Debug)]
pub struct TranscodeStartParams {
    #[serde(rename = "content")]
    contents: Vec<String>,
    #[serde(rename = "preferred_audio")]
    preferred_audio_streams: Vec<i32>,
    #[serde(rename = "preferred_subtitle")]
    preferred_subtitle_streams: Vec<i32>,
    profile: Option<String>,
}

pub async fn transcode_start(
    Path(connection_id): Path<i32>,
    ViewEngine(v): ViewEngine<BetterTeraView>,
    HxRequest(boosted): HxRequest,
    State(ctx): State<AppContext>,
    Form(data): Form<TranscodeStartParams>,
) -> Result<Response> {
    let connection = load_item(&ctx, connection_id).await?;
    let provider: ConnectedMediaProvider = connection.clone().try_into()?;
    let mut work = vec![];
    for (i, content) in data.contents.iter().enumerate() {
        let item = provider.item(&ctx, &content).await?;

        match item {
            Item::Content(content) => {
                let mut streams = vec![];
                let preferred_audio_stream = data.preferred_audio_streams[i];
                let preferred_subtitle_stream = data.preferred_subtitle_streams[i];
                for stream in &content.media_streams {
                    match stream {
                        MediaStream::Audio { index, .. } if index == &preferred_audio_stream => {
                            streams.push(stream.clone());
                        }
                        MediaStream::Subtitle { index, .. }
                            if index == &preferred_subtitle_stream =>
                        {
                            streams.push(stream.clone());
                        }
                        _ => {}
                    }
                }
                work.push(DownloadWorkerArgs {
                    user_id: connection.user_id,
                    connection_id: connection.id,
                    profile: data.profile.clone(),
                    content: content.clone(),
                    preferred_mediastreams: streams,
                });
            }
            Item::Library(_) => {
                return Err(Error::BadRequest("Not a content item".to_string()));
            }
        }
    }

    for work in work {
        // tracing::error!("{:?} {:?}", work.content.name, work.preferred_mediastreams);
        DownloadWorker::perform_later(&ctx, work).await.unwrap();
    }

    Ok(Response::new("k".into()))
}

#[debug_handler]
pub async fn content_transcode(
    Path((provider_id, content_id)): Path<(i32, String)>,
    ViewEngine(v): ViewEngine<BetterTeraView>,
    State(ctx): State<AppContext>,
) -> Result<Response> {
    let connection = load_item(&ctx, provider_id).await?;
    let provider: ConnectedMediaProvider = connection.clone().try_into()?;

    let item = provider.item(&ctx, &content_id).await?;
    match item {
        Item::Content(content) => {
            DownloadWorker::perform_later(
                &ctx,
                DownloadWorkerArgs {
                    user_id: connection.user_id,
                    connection_id: connection.id,
                    profile: None,
                    content: content,
                    preferred_mediastreams: vec![],
                },
            )
            .await
            .expect("fuck u");
        }
        Item::Library(_) => {
            return Err(Error::BadRequest("Not a content item".to_string()));
        }
    }

    Ok(Response::new("k".into()))
}

pub async fn stream(
    Path(path): Path<String>,
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse> {
    let p = std::path::Path::new(&path);
    let body: Vec<u8> = ctx.storage.download(p).await?;
    let content_type = if path.ends_with(".ts") {
        "video/mp2t"
    } else if path.ends_with(".m3u8") {
        "application/vnd.apple.mpegurl"
    } else {
        "application/json"
    };

    Ok((
        axum::response::AppendHeaders([("content-type", content_type)]),
        Bytes::from(body),
    ))
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("p")
        .add("/new", get(new))
        .add("/:id", get(show))
        .add("/:id/:library", get(show_library))
        .add("/:id/transcode", get(transcode).post(transcode_start))
        .add("/:id/:content/transcode", get(content_transcode))
        .add("/setup", post(setup))
        .add("/", post(add))
        .add("/stream/*path", get(stream))
}
