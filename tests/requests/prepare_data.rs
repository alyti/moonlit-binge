use axum::http::{HeaderName, HeaderValue};
use axum_test::TestServer;
use loco_rs::app::AppContext;
use moonlit_binge::models::users;

const USER_EMAIL: &str = "test@loco.com";
const USER_PASSWORD: &str = "1234";

#[derive(Debug)]
pub struct LoggedInUser {
    pub user: users::Model,
    pub token: String,
}

pub async fn init_user_login(request: &TestServer, ctx: &AppContext) -> LoggedInUser {
    let register_payload = serde_json::json!({
        "name": "loco",
        "email": USER_EMAIL,
        "password": USER_PASSWORD
    });

    //Creating a new user
    request.post("/auth/register").json(&register_payload).await;
    let user = users::Model::find_by_email(&ctx.db, USER_EMAIL)
        .await
        .unwrap();

    let verify_payload = serde_json::json!({
        "token": user.email_verification_token,
    });

    request.post("/auth/verify").json(&verify_payload).await;

    let response = request
        .post("/auth/login")
        .json(&serde_json::json!({
            "email": USER_EMAIL,
            "password": USER_PASSWORD
        }))
        .await;

    let header = response.header("set-cookie");
    let cookie = header.to_str().unwrap();
    let split: Vec<&str> = cookie.split('=').collect();
    let token = split[1].to_string();

    assert_eq!(split[0], "moonlit_binge_jwt", "Token not found in cookie");
    assert_eq!(&token[0..2], "ey", "Token doesn't look like a JWT");

    LoggedInUser {
        user: users::Model::find_by_email(&ctx.db, USER_EMAIL)
            .await
            .unwrap(),
        token,
    }
}

pub fn auth_header(token: &str) -> (HeaderName, HeaderValue) {
    let auth_header_value =
        HeaderValue::from_str(&format!("moonlit_binge_jwt={}", &token)).unwrap();

    (HeaderName::from_static("cookie"), auth_header_value)
}
