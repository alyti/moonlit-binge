#![allow(clippy::unused_async)]
use std::{collections::HashMap, convert::Infallible, sync::Arc};

use axum::{
    response::{
        sse::{Event, KeepAlive},
        Sse,
    },
    Extension,
};
use axum_htmx::HxRequest;
use futures_util::{Stream, StreamExt};
use loco_rs::prelude::*;
use players::types::{Content, Item};
use sea_orm::{Order, QueryOrder};
use serde_json::json;
use tokio::sync::Mutex;

use crate::{
    initializers::{
        media_provider::{ConnectedMediaProvider, MediaProviders},
        view_engine::BetterTeraView,
    },
    models::{
        _entities::{player_connections, users},
        content_downloads::Notification,
    },
    views,
};

use super::extractors::{auth::JWTWithUser, Format};

/// Renders the dashboard home page
///
/// # Errors
///
/// This function will return an error if render fails
pub async fn render_home(
    State(ctx): State<AppContext>,
    Format(f): Format<BetterTeraView>,
    Extension(_media_providers): Extension<Box<MediaProviders>>,
    auth: JWTWithUser<users::Model>,
) -> Result<Response> {
    let connections = player_connections::Entity::find()
        .order_by(player_connections::Column::Id, Order::Desc)
        .filter(player_connections::Column::UserId.eq(auth.user.id))
        .all(&ctx.db)
        .await?;
    views::dashboard::home(f, &connections)
}

pub async fn notify_sub(
    State(ctx): State<AppContext>,
    auth: JWTWithUser<users::Model>,
    ViewEngine(v): ViewEngine<BetterTeraView>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let connections = player_connections::Entity::find()
        .order_by(player_connections::Column::Id, Order::Desc)
        .filter(player_connections::Column::UserId.eq(auth.user.id))
        .all(&ctx.db)
        .await?;
    let listen_list = connections
        .iter()
        .map(|c| format!("provider-{}", c.id))
        .collect::<Vec<_>>();

    let mut providers: HashMap<i32, ConnectedMediaProvider> = HashMap::new();
    for connection in connections {
        let id = connection.id;
        if let Ok(provider) = connection.try_into() {
            providers.insert(id, provider);
        }
    }

    let providers = Arc::new(providers);
    let contents = Arc::new(Mutex::new(HashMap::new()));
    let downloads = Arc::new(Mutex::new(HashMap::new()));
    let pool = ctx.db.get_postgres_connection_pool();
    let mut listen = sqlx_postgres::PgListener::connect_with(pool)
        .await
        .map_err(|_| Error::string("db connect"))?;
    listen
        .listen_all(listen_list.iter().map(|c| c.as_str()))
        .await
        .map_err(|_| Error::string("db listen"))?;
    let v = Arc::new(v);
    let ctx = Arc::new(ctx);
    let stream = listen
        .into_stream()
        .filter_map(move |notification| {
            let ctx = ctx.clone();
            let providers = providers.clone();
            let contents = contents.clone();
            let v = v.clone();
            let downloads = downloads.clone();
            async move {
                if let Ok(notification) = notification {
                    let payload: Notification =
                        serde_json::from_str(notification.payload()).ok()?;
                    let connection = (&payload).player_connection_id;
                    let content_id = (&payload).content_id.clone();
                    let mut downloads = downloads.lock().await;
                    downloads.insert(payload.download_id.clone(), payload);
                    let mut contents = contents.lock().await;
                    if !contents.contains_key(content_id.as_str()) {
                        if let Ok(content) =
                            crate::models::_entities::contents::Model::content_by_connection_and_id(
                                &ctx.db,
                                connection,
                                &content_id,
                            )
                            .await
                        {
                            contents.insert(content_id.clone(), content);
                        }
                    }
                    let downloads_values = downloads
                        .values()
                        .map(|v| {
                            let content = contents
                                .get(v.content_id.as_str())
                                .expect("content not found");
                            let connection =
                                providers.get(&connection).expect("provider not found");
                            json!({
                                "connection": connection,
                                "content": content,
                                "data": v,
                            })
                        })
                        .collect::<Vec<_>>();
                    Some(
                        Event::default().data(
                            v.render(
                                "notifications/index.html",
                                json!({"downloads": downloads_values}),
                            )
                            .unwrap(),
                        ),
                    )
                } else {
                    None
                }
            }
        })
        .map(Ok);

    Ok(axum::response::Sse::new(stream).keep_alive(KeepAlive::default()))
}

// pub async fn notify_pub(
//     State(ctx): State<AppContext>,
//     HxRequest(req): HxRequest,
// ) -> Result<impl IntoResponse> {
//     let pool = ctx.db.get_postgres_connection_pool();
//     let sql = "NOTIFY test, 'hello'";
//     ctx.db
//         .execute(sea_orm::query::Statement::from_sql_and_values(
//             sea_orm::DatabaseBackend::Postgres,
//             sql,
//             vec![],
//         ))
//         .await?;
//     Ok("ok")
// }

pub fn routes() -> Routes {
    Routes::new()
        .add("/", get(render_home))
        .add("/sub", get(notify_sub))
    // .add("/pub", get(notify_pub))
}
