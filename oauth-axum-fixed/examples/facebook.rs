mod utils;
use std::sync::Arc;

use axum::extract::Query;
use axum::Router;
use axum::{routing::get, Extension};
use oauth_axum::providers::facebook::FacebookProvider;
use oauth_axum::{CustomProvider, OAuthClient};

use crate::utils::memory_db_util::AxumState;

#[derive(Clone, serde::Deserialize)]
pub struct QueryAxumCallback {
    pub code: String,
    pub state: String,
}

#[tokio::main]
async fn main() {
    dotenv::from_filename("examples/.env").ok();
    println!("Starting server...");

    let state = Arc::new(AxumState::new());
    let app = Router::new()
        .route("/", get(create_url))
        .route("/api/v1/facebook/callback", get(callback))
        .layer(Extension(state.clone()));

    println!("🚀 Server started successfully");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn get_client() -> CustomProvider {
    FacebookProvider::new(
        std::env::var("FACEBOOK_CLIENT_ID").expect("FACEBOOK_CLIENT_ID must be set"),
        std::env::var("FACEBOOK_SECRET").expect("FACEBOOK_SECRET must be set"),
        "http://localhost:3000/api/v1/facebook/callback".to_string(),
    )
}

pub async fn create_url(Extension(state): Extension<Arc<AxumState>>) -> String {
    let state_oauth = get_client()
        .generate_url(
            Vec::from(["public_profile".to_string(), "email".to_string()]),
            |state_e| async move {
                state.set(state_e.state, state_e.verifier);
            },
        )
        .await
        .unwrap()
        .state
        .unwrap();

    state_oauth.url_generated.unwrap()
}

pub async fn callback(
    Extension(state): Extension<Arc<AxumState>>,
    Query(queries): Query<QueryAxumCallback>,
) -> String {
    println!("{:?}", state.clone().get_all_items());
    let item = state.get(queries.state.clone());
    get_client()
        .generate_token(queries.code, item.unwrap())
        .await
}
