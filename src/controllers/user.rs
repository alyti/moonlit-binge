use axum::debug_handler;
use loco_rs::prelude::*;

use crate::{
    controllers::extractors::auth::JWT, models::_entities::users, views::user::CurrentResponse,
};

#[debug_handler]
async fn current(auth: JWT, State(ctx): State<AppContext>) -> Result<Response> {
    let user = users::Model::find_by_pid(&ctx.db, &auth.claims.pid).await?;
    format::json(CurrentResponse::new(&user))
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("api/user")
        .add("/current", get(current))
}
