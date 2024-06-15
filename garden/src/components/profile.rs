use leptos::{component, view, IntoView};
use leptos_use::{use_cookie, utils::FromToStringCodec};

#[component]
pub fn ProfileScreen() -> impl IntoView {
    let (token, set_token) = use_cookie::<String, FromToStringCodec>("token");

    view! {
        <h1>"Profile"</h1>
        <button on:click=move |_| set_token(None)>"Logout"</button>
    }
}