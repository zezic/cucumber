use std::str::FromStr;

use leptos::{Await, Suspense};
use leptos::{
    component, create_resource, logging, server, view, IntoView, ServerFnError, SignalGet,
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

    // let async_data = create_resource(
    //     token,
    //     |token| async move {
    //         if token.is_some() {
    //             logging::log!("loading data from API...");
    //             if let Ok(user_view) = get_me().await {
    //                 logging::log!("loaded: {:?}", user_view);
    //                 return Some(user_view);
    //             }
    //         }
    //         return None;
    //     },
    // );

    view! {
        <h1>"Profile"</h1>
            // move || match async_data.get() {
            //     Some(data) => view! { <pre>format!("{:#?}", data)</pre> }.into_view(),
            //     None => view! { <span>"Loading..."</span> }.into_view(),
            // }
            <Await
                future=|| get_me()
                let:data
            >
                <pre>{ format!("{:#?}", data) }</pre>
            </Await>
        <button on:click=move |_| set_token(None)>"Logout"</button>
    }
}
