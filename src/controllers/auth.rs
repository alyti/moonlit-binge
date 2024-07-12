use axum::{debug_handler, http::Uri};
use axum_htmx::{HxRequest, HxRedirect};
use cookie::{Cookie, CookieJar};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    initializers::view_engine::BetterTeraView, mailers::auth::AuthMailer, models::{
        _entities::users,
        users::{LoginParams, RegisterParams},
    }, views::{self, auth::LoginResponse}
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

/// Register function creates a new user with the given parameters and sends a
/// welcome email to the user
#[debug_handler]
async fn register(
    ViewEngine(v): ViewEngine<BetterTeraView>,
    State(ctx): State<AppContext>,
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
            return Ok(views::auth::partial_register(&v)?.into_response());
        }
    };

    let user = user
        .into_active_model()
        .set_email_verification_sent(&ctx.db)
        .await?;

    AuthMailer::send_welcome(&ctx, &user).await?;

    let jwt_secret = ctx.config.get_jwt_config()?;

    let token = user
        .generate_jwt(&jwt_secret.secret, &jwt_secret.expiration)
        .or_else(|_| unauthorized("unauthorized!"))?;

    let mut cookie = Cookie::new("moonlit_binge_jwt", token.clone());
    cookie.set_path("/");

    Ok((HxRedirect(Uri::from_static("/")), jar.add(cookie), format::json(LoginResponse::new(&user, &token))).into_response())
}

/// Verify register user. if the user not verified his email, he can't login to
/// the system.
#[debug_handler]
async fn verify(
    State(ctx): State<AppContext>,
    Json(params): Json<VerifyParams>,
) -> Result<Response> {
    let user = users::Model::find_by_verification_token(&ctx.db, &params.token).await?;

    if user.email_verified_at.is_some() {
        tracing::info!(pid = user.pid.to_string(), "user already verified");
    } else {
        let active_model = user.into_active_model();
        let user = active_model.verified(&ctx.db).await?;
        tracing::info!(pid = user.pid.to_string(), "user verified");
    }

    format::json(())
}

/// In case the user forgot his password  this endpoints generate a forgot token
/// and send email to the user. In case the email not found in our DB, we are
/// returning a valid request for for security reasons (not exposing users DB
/// list).
#[debug_handler]
async fn forgot(
    State(ctx): State<AppContext>,
    Json(params): Json<ForgotParams>,
) -> Result<Response> {
    let Ok(user) = users::Model::find_by_email(&ctx.db, &params.email).await else {
        // we don't want to expose our users email. if the email is invalid we still
        // returning success to the caller
        return format::json(());
    };

    let user = user
        .into_active_model()
        .set_forgot_password_sent(&ctx.db)
        .await?;

    AuthMailer::forgot_password(&ctx, &user).await?;

    format::json(())
}

/// reset user password by the given parameters
#[debug_handler]
async fn reset(State(ctx): State<AppContext>, Json(params): Json<ResetParams>) -> Result<Response> {
    let Ok(user) = users::Model::find_by_reset_token(&ctx.db, &params.token).await else {
        // we don't want to expose our users email. if the email is invalid we still
        // returning success to the caller
        tracing::info!("reset token not found");

        return format::json(());
    };
    user.into_active_model()
        .reset_password(&ctx.db, &params.password)
        .await?;

    format::json(())
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

    let mut cookie = Cookie::new("moonlit_binge_jwt", token.clone());
    cookie.set_path("/");

    Ok((HxRedirect(Uri::from_static("/")), jar.add(cookie), format::json(LoginResponse::new(&user, &token))))
}

pub async fn render_auth_login(ViewEngine(v): ViewEngine<BetterTeraView>, HxRequest(boosted): HxRequest) -> Result<Response> {
    if boosted {
        views::auth::partial_login(&v)
    } else {
        views::auth::base_view(&v, "login")
    }
}
pub async fn render_auth_register(ViewEngine(v): ViewEngine<BetterTeraView>, HxRequest(boosted): HxRequest) -> Result<Response> {
    if boosted {
        views::auth::partial_register(&v)
    } else {
        views::auth::base_view(&v, "register")
    }
}


pub fn routes() -> Routes {
    Routes::new()
        .prefix("auth")
        .add("/register", post(register))
        .add("/verify", post(verify))
        .add("/login", post(login))
        .add("/forgot", post(forgot))
        .add("/reset", post(reset))
        .add("/login", get(render_auth_login))
        .add("/register", get(render_auth_register))
}
