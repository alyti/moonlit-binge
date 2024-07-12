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

pub fn partial_login(v: &impl ViewRenderer) -> Result<Response> {
    format::render().view(v, "auth/login.html", serde_json::json!({}))
}

pub fn partial_register(v: &impl ViewRenderer) -> Result<Response> {
    format::render().view(v, "auth/register.html", serde_json::json!({}))
}

pub fn partial_forgot(v: &impl ViewRenderer) -> Result<Response> {
    format::render().view(v, "auth/forgot.html", serde_json::json!({}))
}

pub fn partial_reset(v: &impl ViewRenderer) -> Result<Response> {
    format::render().view(v, "auth/reset.html", serde_json::json!({}))
}

pub fn base_view(v: &impl ViewRenderer, action: &str) -> Result<Response> {
    format::render().view(v, "auth/index.html", serde_json::json!({"action": action}))
}
