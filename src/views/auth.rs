use cookie::Cookie;
use format::RenderBuilder;
use serde::{Deserialize, Serialize};

use crate::models::_entities::users;

use super::Format;
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

impl LoginResponse {
    pub fn render<V: ViewRenderer>(&self, f: Format<V>) -> Result<Response> {
        if let Format::Json = f {
            return format::json(self);
        }

        let mut cookie = Cookie::new("moonlit_binge_jwt", &self.token);
        cookie.set_path("/");
        format::RenderBuilder::new()
            .cookies(&[cookie])?
            .redirect("/")
    }
}

pub fn after_verify_redirect() -> Result<Response> {
    format::redirect("/auth/login")
}
