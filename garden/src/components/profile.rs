use std::str::FromStr;

use leptos::{component, create_resource, logging, server, view, IntoView, ServerFnError, SignalGet};
use leptos_use::{use_cookie, utils::FromToStringCodec};
use serde::{Deserialize, Serialize};
use leptos::SignalWith;
use leptos::Suspense;

#[derive(Debug, Deserialize, Serialize)]
pub struct UserView {
    pub username: String,
    pub display_name: String,
    pub show_display_name: bool,
    pub bio: String,
}

#[server(GetMe, "/api")]
pub async fn get_me() -> Result<UserView, ServerFnError> {
    use crate::state::garden_state;
    use leptos_axum::extract;
use axum_extra::extract::CookieJar;
use uuid::Uuid;
use axum_extra::extract::cookie::Cookie;

    let jar: CookieJar = extract().await?;

    let Some(token) = jar
        .get("token")
        .map(|cookie| cookie.value())
        .map(|token| Uuid::from_str(token).ok())
        .flatten() else {
            return Err(ServerFnError::ServerError("no token".into()))
        };

    let garden_state = garden_state()?;

    let Some(entity::user::Model {
        username,
        display_name,
        show_display_name,
        bio,
        ..
    }) = garden_state.db.get_user_by_token(token).await else {
        return Err(ServerFnError::ServerError("expired token?".into()))
    };

    Ok(UserView {
        username,
        display_name,
        show_display_name,
        bio,
    })
}

fn render_user(user: &Option<UserView>) -> impl IntoView {
    if let Some(user) = user {
    let data_view = format!("{:#?}", user);
    view! { <pre>{ data_view }</pre> }
    } else {
        view! { <pre> "NININININ" </pre> }
    }
}

#[component]
pub fn ProfileScreen() -> impl IntoView {
    let (token, set_token) = use_cookie::<String, FromToStringCodec>("token");

    // our resource
    let async_data = create_resource(
        move || token.get(),
        // every time `count` changes, this will run
        |token| async move {
            if token.is_some() {
                logging::log!("loading data from API");
                if let Ok(user_view) = get_me().await {
                    return Some(user_view)
                }
            }
            return None
        },
    );

    let rendered_profile = move || {
        async_data.map(|data| {
            logging::log!("NOW DATA IS: {:?}", data);
            Ok::<_, ServerFnError>(render_user(data))
        })
    };

    view! {
        <h1>"Profile"</h1>
        <Suspense fallback=move || view! { <span>"Loading..."</span> }> { rendered_profile } </Suspense>
        <button on:click=move |_| set_token(None)>"Logout"</button>
    }
}
