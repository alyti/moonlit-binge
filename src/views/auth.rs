use serde::{Deserialize, Serialize};

use crate::models::_entities::users;

use loco_rs::prelude::*;

#[derive(Debug, Deserialize, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub pid: String,
    pub name: String,
    pub is_verified: bool,
}

impl LoginResponse {
    #[must_use]
    pub fn new(user: &users::Model, token: &String) -> Self {
        Self {
            token: token.to_string(),
            pid: user.pid.to_string(),
            name: user.name.clone(),
            is_verified: user.email_verified_at.is_some(),
        }
    }
}

pub fn base_view<T: Serialize>(
    v: &impl ViewRenderer,
    partial: bool,
    action: &str,
    ctx: &T,
) -> Result<Response> {
    format::render().view(
        v,
        &format!("auth/{}.html", if partial { action } else { "index" }),
        HtmxPartial { action, ctx },
    )
}

#[derive(serde::Serialize)]
pub struct HtmxPartial<'a, T: Serialize> {
    pub action: &'a str,
    #[serde(flatten)]
    pub ctx: &'a T,
}
