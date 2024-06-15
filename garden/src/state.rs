use anyhow::Result;
use axum::extract::FromRef;
use leptos::{use_context, LeptosOptions, ServerFnError};
use leptos_router::RouteListing;
use tokio::sync::Mutex;
use std::{collections::HashMap, sync::Arc};

use crate::db::Db;

/// This takes advantage of Axum's SubStates feature by deriving FromRef. This is the only way to have more than one
/// item in Axum's State. Leptos requires you to have leptosOptions in your State struct for the leptos route handlers
#[derive(FromRef, Clone)]
pub struct AppState {
    pub leptos_options: LeptosOptions,
    pub garden_state: Arc<GardenState>,
    pub routes: Vec<RouteListing>,
}

#[derive(Clone)]
pub struct GardenState {
    pub oauth_states: Arc<Mutex<HashMap<String, String>>>,
    pub db: Arc<Db>,
}

impl GardenState {
    pub async fn new(db_url: &str) -> Result<GardenState> {
        let db = Db::new(db_url).await?;
        Ok(GardenState { oauth_states: Arc::new(Mutex::new(HashMap::new())), db: Arc::new(db) })
    }
}

pub fn garden_state() -> Result<Arc<GardenState>, ServerFnError> {
    use_context::<Arc<GardenState>>()
        .ok_or_else(|| ServerFnError::ServerError("GardenState missing.".into()))
}