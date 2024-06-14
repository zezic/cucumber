use axum::extract::FromRef;
use leptos::{use_context, LeptosOptions, ServerFnError};
use leptos_router::RouteListing;
use tokio::sync::Mutex;
use std::{collections::HashMap, sync::Arc};

/// This takes advantage of Axum's SubStates feature by deriving FromRef. This is the only way to have more than one
/// item in Axum's State. Leptos requires you to have leptosOptions in your State struct for the leptos route handlers
#[derive(FromRef, Clone)]
pub struct AppState {
    pub leptos_options: LeptosOptions,
    pub garden_state: Arc<GardenState>,
    pub routes: Vec<RouteListing>,
}

#[derive(Clone, Default)]
pub struct GardenState {
    pub oauth_states: Arc<Mutex<HashMap<String, String>>>,
}

pub fn garden_state() -> Result<Arc<GardenState>, ServerFnError> {
    use_context::<Arc<GardenState>>()
        .ok_or_else(|| ServerFnError::ServerError("GardenState missing.".into()))
}