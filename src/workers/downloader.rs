use std::{path::PathBuf, str::FromStr};

use axum::{body::Bytes, http::Uri};
use futures_util::StreamExt;
use loco_rs::prelude::*;
use players::types::{Content, MediaStream, TranscodeJob};
use serde::{Deserialize, Serialize};

use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{
    default_on_request_failure, policies::ExponentialBackoff, RetryTransientMiddleware, Retryable,
    RetryableStrategy,
};
use uuid::Uuid;

use crate::{
    common::notifications,
    initializers::media_provider::ConnectedMediaProvider,
    models::_entities::{
        content_downloads,
        player_connections::Model,
        sea_orm_active_enums::StatusName::{Error as ErrorStatus, InProgress, Success},
    },
};

pub struct DownloadWorker {
    pub ctx: AppContext,
}

impl std::fmt::Debug for DownloadWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadWorker").finish()
    }
}

#[derive(Deserialize, Debug, Serialize)]
pub struct DownloadWorkerArgs {
    pub user_id: i32,
    pub connection_id: i32,
    pub content_download_id: Uuid,

    pub profile: Option<String>,
    pub content: Content,
    pub preferred_mediastreams: Vec<MediaStream>,
}

impl worker::AppWorker<DownloadWorkerArgs> for DownloadWorker {
    fn build(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }
}

const CONCURRENT_DOWNLOADS: usize = 4;
const RETRY_DOWNLOADS: u32 = 15;

#[async_trait]
impl worker::Worker<DownloadWorkerArgs> for DownloadWorker {
    #[tracing::instrument(skip_all, fields(user_id = args.user_id, connection_id = args.connection_id, content_download_id = ?args.content_download_id))]
    async fn perform(&self, args: DownloadWorkerArgs) -> worker::Result<()> {
        let connection = Model::find_by_user_and_id(&self.ctx.db, args.user_id, args.connection_id)
            .await
            .map_err(|_| sidekiq::Error::Message("Could not find player connection".to_string()))?;

        let provider: ConnectedMediaProvider = connection
            .clone()
            .try_into()
            .map_err(|_| sidekiq::Error::Message("Could not create provider".to_string()))?;
        let transcode = provider
            .transcode(
                &self.ctx,
                &args.content,
                args.profile.as_deref(),
                &args.preferred_mediastreams,
            )
            .await
            .map_err(|e| sidekiq::Error::Message(e.to_string()))?;

        let content_id = &args.content.id.clone();

        match transcode {
            TranscodeJob::M3U8(mut playlist) => {
                let mut v: Vec<u8> = Vec::new();
                playlist.main.write_to(&mut v).unwrap();
                let base_path = std::path::Path::new(&format!("single/{}", &args.connection_id))
                    .join(&args.content.id);
                self.ctx
                    .storage
                    .upload(&base_path.join("main.m3u8"), &Bytes::from(v))
                    .await
                    .unwrap();

                let mut paths = Vec::new();
                for (name, media) in &mut playlist.media {
                    media.segments.iter_mut().for_each(|segment| {
                        let uri = segment.uri.clone();
                        segment.uri = format!(
                            "{}/{}",
                            name,
                            Uri::from_str(&segment.uri)
                                .unwrap()
                                .path()
                                .split('/')
                                .last()
                                .unwrap()
                                .to_owned()
                        );
                        paths.push((uri, segment.uri.clone()));
                    });
                    let mut v: Vec<u8> = Vec::new();
                    media.write_to(&mut v).unwrap();
                    self.ctx
                        .storage
                        .upload(&base_path.join(format!("{name}.m3u8")), &Bytes::from(v))
                        .await
                        .unwrap();
                }

                let start_time = std::time::Instant::now();
                let total = paths.len();
                let mut eta = eta::Eta::new(total, eta::TimeAcc::SEC);
                let (tx, mut rx) = tokio::sync::mpsc::channel(CONCURRENT_DOWNLOADS * 4);
                let ctx: AppContext = self.ctx.clone();
                tokio::spawn(async move {
                    let fetches = futures_util::stream::iter(paths.into_iter().enumerate().map(
                        move |(idx, (uri, filename))| {
                            let base_path =
                                std::path::Path::new(&format!("single/{}", &args.connection_id))
                                    .join(&args.content.id);
                            let tx = tx.clone();
                            let ctx: AppContext = ctx.clone();
                            async move {
                                match Self::download_file(ctx, uri, filename, base_path).await {
                                    Ok(_) => {
                                        if let Err(_) = tx.send(Ok(idx)).await {
                                            tracing::error!("receiver dropped");
                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        if let Err(_) = tx.send(Err((idx, e))).await {
                                            tracing::error!("receiver dropped");
                                            return;
                                        }
                                    }
                                }
                            }
                        },
                    ))
                    .buffer_unordered(CONCURRENT_DOWNLOADS)
                    .collect::<Vec<()>>();
                    fetches.await;
                });

                let mut seen_idx = vec![];
                while let Some(data) = rx.recv().await {
                    let res = match data {
                        Ok(i) => {
                            seen_idx.push(i);
                            eta.step();
                            tracing::debug!(
                                done = seen_idx.len(),
                                total,
                                idx = i,
                                "Downloaded segment"
                            );
                            // if i % (CONCURRENT_DOWNLOADS * 4) == 1 {
                            let var_name = notifications::DownloaderStatus::SegmentProgressReport {
                                done: seen_idx.len(),
                                total,
                                eta: eta.to_string(),
                                eta_seconds: eta.time_remaining(),
                            };
                            content_downloads::Model::notify_status(
                                &self.ctx.db,
                                args.content_download_id,
                                content_id,
                                InProgress,
                                &var_name,
                            )
                            .await
                            // } else {
                            //     continue;
                            // }
                        }
                        Err((i, e)) => {
                            tracing::error!(error = ?e, idx = i, "Failed to download segment");
                            let var_name = notifications::DownloaderStatus::SegmentFailed {
                                segment_id: i,
                                error: e.to_string(),
                            };
                            content_downloads::Model::notify_status(
                                &self.ctx.db,
                                args.content_download_id,
                                content_id,
                                ErrorStatus,
                                &var_name,
                            )
                            .await
                        }
                    };
                    if let Err(e) = res {
                        tracing::error!(error = ?e, "Failed to notify status");
                    }
                }
                let elapsed = start_time.elapsed();
                tracing::info!(?elapsed, "Downloaded all segments");
                let var_name = notifications::DownloaderStatus::Finished { elapsed };
                if let Err(e) = content_downloads::Model::notify_status(
                    &self.ctx.db,
                    args.content_download_id,
                    content_id,
                    Success,
                    &var_name,
                )
                .await
                {
                    tracing::error!(error = ?e, "Failed to notify status");
                }
                Ok(())
            }
        }
    }
}

impl DownloadWorker {
    async fn download_file(
        ctx: AppContext,
        url: String,
        filename: String,
        base_path: PathBuf,
    ) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        let client = http_client();
        let resp = client.get(url).send().await?;
        let bytes = resp.error_for_status()?.bytes().await?;
        let path = base_path.join(filename);
        ctx.storage.upload(&path, &bytes).await?;
        Ok(())
    }
}

fn http_client() -> ClientWithMiddleware {
    let client = reqwest::Client::builder().build().unwrap();

    ClientBuilder::new(client)
        .with(RetryTransientMiddleware::new_with_policy_and_strategy(
            ExponentialBackoff::builder().build_with_max_retries(RETRY_DOWNLOADS),
            RetryNot200,
        ))
        .build()
}

struct RetryNot200;
impl RetryableStrategy for RetryNot200 {
    fn handle(
        &self,
        res: &Result<reqwest::Response, reqwest_middleware::Error>,
    ) -> Option<Retryable> {
        match res {
            // retry if not 200
            Ok(success) if success.status() != 200 => Some(Retryable::Transient),
            // otherwise do not retry a successful request
            Ok(_) => None,
            // but maybe retry a request failure
            Err(error) => default_on_request_failure(error),
        }
    }
}
