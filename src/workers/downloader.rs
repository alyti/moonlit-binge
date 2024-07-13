use std::{
    path::{PathBuf},
    str::FromStr,
};

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

use crate::{
    initializers::media_provider::{ConnectedMediaProvider},
    models::_entities::player_connections::Model,
};

pub struct DownloadWorker {
    pub ctx: AppContext,
}

#[derive(Deserialize, Debug, Serialize)]
pub struct DownloadWorkerArgs {
    pub user_id: i32,
    pub connection_id: i32,

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
const RETRY_DOWNLOADS: u32 = 8;

#[async_trait]
impl worker::Worker<DownloadWorkerArgs> for DownloadWorker {
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

        // return Ok(());
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

                // if let Some(publisher) = &self.ctx.queue {
                //     if let Some(ref mut publisher) = publisher.get().await.ok() {
                //         let conn = publisher.unnamespaced_borrow_mut();
                //         conn.publish::<&str, &str, ()>("transcoding", "ok").await.unwrap();
                //     }
                // }
                let fetches =
                    futures_util::stream::iter(paths.into_iter().map(move |(uri, filename)| {
                        let base_path =
                            std::path::Path::new(&format!("single/{}", &args.connection_id))
                                .join(&args.content.id);
                        self.download_file(uri, filename, base_path)
                    }))
                    .buffer_unordered(CONCURRENT_DOWNLOADS)
                    .collect::<Vec<Result<(), Box<dyn std::error::Error + Sync + Send>>>>();
                fetches.await;
                Ok(())
            }
        }
    }
}

impl DownloadWorker {
    async fn download_file(
        &self,
        url: String,
        filename: String,
        base_path: PathBuf,
    ) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        let client = http_client();
        let resp = client.get(url).send().await?;
        let bytes = resp.error_for_status()?.bytes().await?;
        let path = base_path.join(filename);
        self.ctx.storage.upload(&path, &bytes).await?;
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
