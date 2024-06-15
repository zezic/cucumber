use std::{collections::HashMap, str::FromStr, sync::Arc};

use anyhow::anyhow;
use axum::{
    extract::{Path, Query},
    response::{Redirect, Result},
    routing::get,
    Extension, Json, Router,
};
use axum_extra::extract::{cookie::Cookie, CookieJar};
use oauth_axum::{providers::twitter::TwitterProvider, CustomProvider, OAuthClient};
use tokio::sync::Mutex;
use twitter_v2::{authorization::BearerToken, TwitterApi, User};
use uuid::Uuid;

use crate::{
    db::UserArgs,
    state::{AppState, GardenState},
};

pub fn api_routes<S: Clone + Send + Sync + 'static>(app_state: Arc<GardenState>) -> Router<S> {
    async fn handler() -> &'static str {
        "Hello, World!"
    }

    let oauth_state = Arc::new(Mutex::new(HashMap::<String, String>::new()));

    Router::new()
        .route("/hi", get(handler))
        .nest(
            "/oauth",
            Router::new()
                .route("/verify/:provider", get(callback))
                .route("/begin/:provider", get(begin)),
        )
        .layer(Extension(oauth_state))
        .layer(Extension(app_state))
}

pub fn get_client(provider: &str) -> anyhow::Result<(CustomProvider, Vec<String>)> {
    dotenv::from_filename(".env").ok();

    let redirect_url = format!("http://cucumber.vision:3000/api/oauth/verify/{provider}");

    let provider = match provider {
        "twitter" => (
            TwitterProvider::new(
                std::env::var("OAUTH_TWITTER_CLIENT_ID")
                    .expect("OAUTH_TWITTER_CLIENT_ID must be set"),
                std::env::var("OAUTH_TWITTER_SECRET").expect("OAUTH_TWITTER_SECRET must be set"),
                redirect_url,
            ),
            vec![
                "users.read".to_string(),
                "tweet.read".to_string(), // tweet.read is also required to read user info
                "offline.access".to_string(), // needed too?
            ],
        ),
        x => return Err(anyhow!("{x} OAuth2 provider is not supported")),
    };

    Ok(provider)
}

#[derive(Clone, serde::Deserialize)]
pub struct QueryAxumCallback {
    pub code: String,
    pub state: String,
}

pub struct OauthInfo {
    external_id: String,
    username: String,
    display_name: String,
    data: serde_json::Value,
}

pub async fn begin(Path(provider): Path<String>,
    Extension(garden_state): Extension<Arc<GardenState>>,
) -> Result<Redirect> {
    use crate::api::get_client;

    let (provider, scopes) = get_client(&provider)
        .map_err(|err| err.to_string())?;

    let state_oauth = provider
        .generate_url(scopes, |state_e| async move {
            garden_state
                .oauth_states
                .lock()
                .await
                .insert(state_e.state, state_e.verifier);
        })
        .await
        .unwrap()
        .state
        .unwrap();

    Ok(Redirect::to(&state_oauth.url_generated.unwrap()))
}

pub async fn callback(
    Path(provider): Path<String>,
    Extension(garden_state): Extension<Arc<GardenState>>,
    Query(queries): Query<QueryAxumCallback>,
    jar: CookieJar,
) -> Result<(CookieJar, Redirect)> {
    let mut state = garden_state.oauth_states.lock().await;
    let verifier = state.remove(&queries.state);
    drop(state);
    let (custom_provider, _scopes) = get_client(&provider).map_err(|err| err.to_string())?;
    let access_token = custom_provider
        .generate_token(queries.code, verifier.unwrap())
        .await;

    let result = match provider.as_str() {
        "twitter" => {
            let access_token = BearerToken::new(access_token.clone());
            let user = TwitterApi::new(access_token)
                .get_users_me()
                .send()
                .await
                .unwrap()
                .into_data()
                .unwrap();
            let external_id = user.id.to_string();
            let info = OauthInfo {
                external_id,
                username: user.username.clone(),
                display_name: user.name.clone(),
                data: serde_json::to_value(user).map_err(|err| err.to_string())?,
            };
            Ok(info)
        }
        x => Err(format!("{x} OAuth2 provider is not supported")),
    }?;

    let token = jar
        .get("token")
        .map(|cookie| cookie.to_string())
        .map(|token| Uuid::from_str(&token).ok())
        .flatten();

    let logged_in_user = if let Some(token) = token {
        garden_state.db.get_user_by_token(token).await
    } else {
        None
    };

    let (user_id, token) = if let Some(user) = logged_in_user {
        (user.id, token.unwrap())
    } else {
        let user_id = garden_state
            .db
            .create_user(UserArgs {
                username: result.username.clone(),
                display_name: result.display_name.clone(),
            })
            .await
            .map_err(|err| err.to_string())?;
        let token = garden_state
            .db
            .login_user(user_id)
            .await
            .map_err(|err| err.to_string())?;
        (user_id, token)
    };

    garden_state
        .db
        .link_external_user(result, access_token, user_id)
        .await
        .map_err(|err| err.to_string())?;

    Ok((
        jar.add(Cookie::new("token", token.hyphenated().to_string())),
        Redirect::to("/"),
    ))
}
