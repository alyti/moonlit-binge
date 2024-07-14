use std::collections::BTreeMap;

use axum::{body::Body, debug_handler, http::Uri};
use axum_htmx::{HxRedirect, HxRequest};
use cookie::{Cookie, CookieJar};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};
use validation::ModelValidationMessage;

use super::extractors::{Format, ProtoHost};

use crate::{
    initializers::view_engine::BetterTeraView,
    mailers::auth::AuthMailer,
    models::{
        _entities::users,
        users::{LoginParams, RegisterParams},
    },
    views::{
        self,
        auth::{after_verify_redirect, LoginResponse},
    },
};
#[derive(Debug, Deserialize, Serialize)]
pub struct VerifyParams {
    pub token: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ForgotParams {
    pub email: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResetParams {
    pub token: String,
    pub password: String,
}

/// register creates a new user with the given parameters and sends a
/// welcome email to the user
///
/// # Errors
///
/// When the user already exists, validation fails, email can't be sent, or JWT can't be signed.
#[debug_handler]
async fn register(
    State(ctx): State<AppContext>,
    Format(f): Format<BetterTeraView>,
    ProtoHost(host): ProtoHost,
    jar: CookieJar,
    Json(params): Json<RegisterParams>,
) -> Result<impl IntoResponse> {
    let res = users::Model::create_with_password(&ctx.db, &params).await;

    let user = match res {
        Ok(user) => user,
        Err(err) => {
            tracing::info!(
                message = err.to_string(),
                user_email = &params.email,
                "could not register user",
            );
            match err {
                ModelError::DbErr(DbErr::Custom(err)) => {
                    let maybe_validatior_errors: Result<
                        BTreeMap<String, Vec<ModelValidationMessage>>,
                        _,
                    > = serde_json::from_str(&err);
                    if let Ok(err) = maybe_validatior_errors {
                        return f.render(
                            None,
                            "auth",
                            "register",
                            &serde_json::json!({"errors": err}),
                        );
                    }
                    return f.render(None, "auth", "register", &serde_json::json!({"error": err}));
                }
                _ => {
                    return f.render(
                        None,
                        "auth",
                        "register",
                        &serde_json::json!({"errors": "Unknown error, try again later."}),
                    );
                }
            }
        }
    };

    let user = user
        .into_active_model()
        .set_email_verification_sent(&ctx.db)
        .await?;

    AuthMailer::send_welcome(&ctx, &host, &user).await?;

    let jwt_secret = ctx.config.get_jwt_config()?;

    let token = match user.generate_jwt(&jwt_secret.secret, &jwt_secret.expiration) {
        Ok(token) => token,
        Err(_) => {
            return f.render(
                None,
                "auth",
                "register",
                &serde_json::json!({"error": "could not generate token"}),
            );
        }
    };

    LoginResponse::new(&user, &token).render(f)
}

/// Verify register user. if the user not verified his email, he can't login to
/// the system.
#[debug_handler]
async fn verify(State(ctx): State<AppContext>, Path(token): Path<String>) -> Result<Response> {
    let user = users::Model::find_by_verification_token(&ctx.db, &token).await?;

    if user.email_verified_at.is_some() {
        tracing::info!(pid = user.pid.to_string(), "user already verified");
    } else {
        let active_model = user.into_active_model();
        let user = active_model.verified(&ctx.db).await?;
        tracing::info!(pid = user.pid.to_string(), "user verified");
    }

    after_verify_redirect()
}

/// In case the user forgot his password  this endpoints generate a forgot token
/// and send email to the user. In case the email not found in our DB, we are
/// returning a valid request for for security reasons (not exposing users DB
/// list).
#[debug_handler]
async fn forgot(
    State(ctx): State<AppContext>,
    Format(f): Format<BetterTeraView>,
    ProtoHost(host): ProtoHost,
    Json(params): Json<ForgotParams>,
) -> Result<Response> {
    let Ok(user) = users::Model::find_by_email(&ctx.db, &params.email).await else {
        // we don't want to expose our users email. if the email is invalid we still
        // returning success to the caller
        return f.render(
            None,
            "auth",
            "forgot",
            &serde_json::json!({"processed": true}),
        );
    };

    let user = match user
        .into_active_model()
        .set_forgot_password_sent(&ctx.db)
        .await
    {
        Ok(user) => user,
        Err(e) => {
            return f.render(
                None,
                "auth",
                "forgot",
                &serde_json::json!({"error": e.to_string()}),
            );
        }
    };

    if let Err(e) = AuthMailer::forgot_password(&ctx, &host, &user).await {
        return f.render(
            None,
            "auth",
            "forgot",
            &serde_json::json!({"error": e.to_string()}),
        );
    }

    f.render(
        None,
        "auth",
        "forgot",
        &serde_json::json!({"processed": true}),
    )
}

/// reset user password by the given parameters
#[debug_handler]
async fn reset(
    State(ctx): State<AppContext>,
    Format(f): Format<BetterTeraView>,
    Path(token): Path<String>,
    Json(params): Json<ResetParams>,
) -> Result<Response> {
    let Ok(user) = users::Model::find_by_reset_token(&ctx.db, &params.token).await else {
        // we don't want to expose our users email. if the email is invalid we still
        // returning success to the caller
        tracing::info!("reset token not found");

        return f.render(
            None,
            "auth",
            "reset",
            &serde_json::json!({"processed": true, "token": token}),
        );
    };
    if let Err(e) = user
        .into_active_model()
        .reset_password(&ctx.db, &params.password)
        .await
    {
        return f.render(
            None,
            "auth",
            "reset",
            &serde_json::json!({"error": e.to_string(), "token": token}),
        );
    }

    f.render(
        None,
        "auth",
        "reset",
        &serde_json::json!({"processed": true, "token": token}),
    )
}

/// Creates a user login and returns a token
#[debug_handler]
async fn login(
    State(ctx): State<AppContext>,
    jar: CookieJar,
    Json(params): Json<LoginParams>,
) -> Result<impl IntoResponse> {
    let user = users::Model::find_by_email(&ctx.db, &params.email).await?;

    let valid = user.verify_password(&params.password);

    if !valid {
        return unauthorized("unauthorized!");
    }

    let jwt_secret = ctx.config.get_jwt_config()?;

    let token = user
        .generate_jwt(&jwt_secret.secret, &jwt_secret.expiration)
        .or_else(|_| unauthorized("unauthorized!"))?;

    let mut cookie = Cookie::new("moonlit_binge_jwt", token);
    cookie.set_path("/");

    Ok((HxRedirect(Uri::from_static("/")), jar.add(cookie)))
}

pub async fn login_form(Format(f): Format<BetterTeraView>) -> Result<Response> {
    f.render(None, "auth", "login", &serde_json::json!({}))
}

pub async fn register_form(Format(f): Format<BetterTeraView>) -> Result<Response> {
    f.render(None, "auth", "register", &serde_json::json!({}))
}

pub async fn forgot_form(Format(f): Format<BetterTeraView>) -> Result<Response> {
    f.render(None, "auth", "forgot", &serde_json::json!({}))
}

pub async fn reset_form(
    Format(f): Format<BetterTeraView>,
    Path(token): Path<String>,
) -> Result<Response> {
    f.render(None, "auth", "reset", &serde_json::json!({"token": token}))
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("auth")
        .add("/login", get(login_form).post(login))
        .add("/register", get(register_form).post(register))
        .add("/verify/:token", get(verify))
        .add("/forgot", get(forgot_form).post(forgot))
        .add("/reset/:token", get(reset_form).post(reset))
}
