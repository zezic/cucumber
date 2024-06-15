use leptos::{component, view, IntoView};
use leptos_use::{use_cookie, utils::FromToStringCodec};
use leptos::SignalWith;
use leptos::SignalGet;

#[component]
pub fn TopBar() -> impl IntoView {
    let (token, _set_token) = use_cookie::<String, FromToStringCodec>("token");

    let account_area = move || {
        token.with(|val| if val.is_some() {
            view! {
                <div class="account-area">
                    <a href="/my">"My Themes"</a>
                    <a href="/profile">"Profile"</a>
                </div>
            }
        } else {
            view! {
                <div class="account-area">
                    <a href="/login">"Login / Register"</a>
                    <p>"Token is >" { move || token.get() } "<"</p>
                </div>
            }
        })
    };

    // let account_area = || view!{ <h2>"snrtiernst"</h2> };

    view! {
        <div class="top-bar">
            <a href="/">"Cucumber"</a>
            { account_area }
        </div>
    }
}