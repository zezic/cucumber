use std::str::FromStr;

use leptos::Await;
use leptos::SignalGet;
use leptos::{
    component, server, view, IntoView, ServerFnError,
};
use leptos_use::{use_cookie, utils::FromToStringCodec};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UserView {
    pub username: String,
    pub display_name: String,
    pub show_display_name: bool,
    pub bio: String,
}

#[server(GetMe, "/api")]
pub async fn get_me() -> Result<UserView, ServerFnError> {
    use crate::state::garden_state;
    use axum_extra::extract::CookieJar;
    use leptos_axum::extract;
    use uuid::Uuid;

    let jar: CookieJar = extract().await?;

    let Some(token) = jar
        .get("token")
        .map(|cookie| cookie.value())
        .map(|token| Uuid::from_str(token).ok())
        .flatten()
    else {
        return Err(ServerFnError::ServerError("no token".into()));
    };

    let garden_state = garden_state()?;

    let Some(entity::user::Model {
        username,
        display_name,
        show_display_name,
        bio,
        ..
    }) = garden_state.db.get_user_by_token(token).await
    else {
        return Err(ServerFnError::ServerError("expired token?".into()));
    };

    Ok(UserView {
        username,
        display_name,
        show_display_name,
        bio,
    })
}

#[component]
pub fn ProfileScreen() -> impl IntoView {
    let (token, set_token) = use_cookie::<String, FromToStringCodec>("token");

    view! {
        <h1>"Profile"</h1>
            {
                if token.get().is_some() {
                    view! {
                        <Await
                            future=|| get_me()
                            let:data
                        >
                            <pre>{ format!("{:#?}", data) }</pre>
                        </Await>
                    }.into_view()
                } else {
                    view! {}.into_view()
                }
            }
        <button on:click=move |_| set_token(None)>"Logout"</button>
    }
}
