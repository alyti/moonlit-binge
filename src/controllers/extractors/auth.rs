use std::collections::HashMap;

use async_trait::async_trait;
use axum::{
    body::Body, extract::{FromRef, FromRequestParts, Query}, http::{request::Parts, HeaderMap}, response::Response
};
use axum_extra::extract::cookie;
use serde::{Deserialize, Serialize};

use loco_rs::{
    app::AppContext, auth, config::JWT as JWTConfig, errors::Error, model::Authenticable,
    Result as LocoResult,
};

// Define constants for token prefix and authorization header
const TOKEN_PREFIX: &str = "Bearer ";
const AUTH_HEADER: &str = "authorization";

// Define a struct to represent user authentication information serialized
// to/from JSON
#[derive(Debug, Deserialize, Serialize)]
pub struct JWTWithUser<T: Authenticable> {
    pub claims: auth::jwt::UserClaims,
    pub user: T,
}

// Implement the FromRequestParts trait for the Auth struct
#[async_trait]
impl<S, T> FromRequestParts<S> for JWTWithUser<T>
where
    AppContext: FromRef<S>,
    S: Send + Sync,
    T: Authenticable,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let ctx: AppContext = AppContext::from_ref(state);

        let token = extract_token(get_jwt_from_config(&ctx).map_err(resp)?, parts).map_err(resp)?;

        let jwt_secret = ctx.config.get_jwt_config().map_err(resp)?;

        match auth::jwt::JWT::new(&jwt_secret.secret).validate(&token) {
            Ok(claims) => {
                let user = T::find_by_claims_key(&ctx.db, &claims.claims.pid)
                    .await
                    .map_err(|e| resp(Error::string(&e.to_string())))?;
                Ok(Self {
                    claims: claims.claims,
                    user,
                })
            }
            Err(e) => {
                return Err(resp(Error::string(&e.to_string())));
            }
        }
    }
}

// Define a struct to represent user authentication information serialized
// to/from JSON
#[derive(Debug, Deserialize, Serialize)]
pub struct JWT {
    pub claims: auth::jwt::UserClaims,
}

// Implement the FromRequestParts trait for the Auth struct
#[async_trait]
impl<S> FromRequestParts<S> for JWT
where
    AppContext: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let ctx: AppContext = AppContext::from_ref(state); // change to ctx

        let token = extract_token(get_jwt_from_config(&ctx).map_err(resp)?, parts).map_err(resp)?;

        let jwt_secret = ctx.config.get_jwt_config().map_err(resp)?;

        match auth::jwt::JWT::new(&jwt_secret.secret).validate(&token) {
            Ok(claims) => Ok(Self {
                claims: claims.claims,
            }),
            Err(e) => {
                return Err(resp(Error::string(&e.to_string())));
            }
        }
    }
}

fn resp(e: loco_rs::Error) -> Response<Body> {
    tracing::warn!("auth error: {}", e);
    Response::builder()
        .status(303)
        .header(axum::http::header::LOCATION, "/auth/login")
        .body(Body::empty())
        .unwrap()
}

/// extract JWT token from context configuration
///
/// # Errors
/// Return an error when JWT token not configured
fn get_jwt_from_config(ctx: &AppContext) -> LocoResult<&JWTConfig> {
    ctx.config
        .auth
        .as_ref()
        .ok_or_else(|| Error::string("auth not configured"))?
        .jwt
        .as_ref()
        .ok_or_else(|| Error::string("JWT token not configured"))
}
/// extract token from the configured jwt location settings
fn extract_token(jwt_config: &JWTConfig, parts: &Parts) -> LocoResult<String> {
    #[allow(clippy::match_wildcard_for_single_variants)]
    match jwt_config
        .location
        .as_ref()
        .unwrap_or(&loco_rs::config::JWTLocation::Bearer)
    {
        loco_rs::config::JWTLocation::Query { name } => extract_token_from_query(name, parts),
        loco_rs::config::JWTLocation::Cookie { name } => extract_token_from_cookie(name, parts),
        loco_rs::config::JWTLocation::Bearer => extract_token_from_header(&parts.headers)
            .map_err(|e| Error::Unauthorized(e.to_string())),
    }
}
/// Function to extract a token from the authorization header
///
/// # Errors
///
/// When token is not valid or out found
pub fn extract_token_from_header(headers: &HeaderMap) -> LocoResult<String> {
    Ok(headers
        .get(AUTH_HEADER)
        .ok_or_else(|| Error::Unauthorized(format!("header {AUTH_HEADER} token not found")))?
        .to_str()
        .map_err(|err| Error::Unauthorized(err.to_string()))?
        .strip_prefix(TOKEN_PREFIX)
        .ok_or_else(|| Error::Unauthorized(format!("error strip {AUTH_HEADER} value")))?
        .to_string())
}

/// Extract a token value from cookie
///
/// # Errors
/// when token value from cookie is not found
pub fn extract_token_from_cookie(name: &str, parts: &Parts) -> LocoResult<String> {
    let jar: cookie::CookieJar = cookie::CookieJar::from_headers(&parts.headers);
    Ok(jar
        .get(name)
        .ok_or(Error::Unauthorized("token is not found".to_string()))?
        .to_string()
        .strip_prefix(&format!("{name}="))
        .ok_or_else(|| Error::Unauthorized("error strip value".to_string()))?
        .to_string())
}

/// Extract a token value from query
///
/// # Errors
/// when token value from cookie is not found
pub fn extract_token_from_query(name: &str, parts: &Parts) -> LocoResult<String> {
    let parameters: Query<HashMap<String, String>> =
        Query::try_from_uri(&parts.uri).map_err(|err| Error::Unauthorized(err.to_string()))?;
    parameters
        .get(name)
        .cloned()
        .ok_or_else(|| Error::Unauthorized(format!("`{name}` query parameter not found")))
}
