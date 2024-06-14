use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query},
    response::Result,
    routing::get,
    Extension, Json, Router,
};
use oauth_axum::{providers::twitter::TwitterProvider, CustomProvider, OAuthClient};
use tokio::sync::Mutex;
use twitter_v2::{authorization::BearerToken, TwitterApi, User};

pub fn api_routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    async fn handler() -> &'static str {
        "Hello, World!"
    }

    let oauth_state = Arc::new(Mutex::new(HashMap::<String, String>::new()));

    Router::new()
        .route("/hi", get(handler))
        .nest(
            "/oauth",
            Router::new()
                .route("/verify/:provider", get(callback)),
        )
        .layer(Extension(oauth_state))
}

pub fn get_client() -> CustomProvider {
    dotenv::from_filename(".env").ok();

    TwitterProvider::new(
        std::env::var("OAUTH_TWITTER_CLIENT_ID").expect("OAUTH_TWITTER_CLIENT_ID must be set"),
        std::env::var("OAUTH_TWITTER_SECRET").expect("OAUTH_TWITTER_SECRET must be set"),
        "http://cucumber.vision:3000/api/oauth/verify/twitter".to_string(),
    )
}

#[derive(Clone, serde::Deserialize)]
pub struct QueryAxumCallback {
    pub code: String,
    pub state: String,
}

pub async fn callback(
    Path(provider): Path<String>,
    Extension(state): Extension<Arc<Mutex<HashMap<String, String>>>>,
    Query(queries): Query<QueryAxumCallback>,
) -> Result<Json<User>> {
    // println!("{:?}", state.clone().get_all_items());
    let mut state = state.lock().await;
    // GET DATA FROM DB OR MEMORY
    // get data using state as ID
    let verifier = state.remove(&queries.state);
    let token = get_client()
        .generate_token(queries.code, verifier.unwrap())
        .await;
    // curl -X GET "https://api.twitter.com/2/users/me"      -H "Authorization: Bearer xxx" -H "Content-Type: application/json"
    // {"data":{"id":"1322333601380388869","name":"Sergey Ukolov","username":"zezic"}}

    let auth = BearerToken::new(token);
    let user = TwitterApi::new(auth)
        .get_users_me()
        .send()
        .await
        .unwrap()
        .into_data()
        .unwrap();

    // {"id":"1322333601380388869","name":"Sergey Ukolov","username":"zezic"}

    Ok(Json(user))
}
