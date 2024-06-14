use leptos::{component, leptos_dom::logging::console_log, server, spawn_local, view, IntoView, ServerFnError};

#[server(GetOauthUrl, "/api")]
pub async fn get_oauth_url(provider: String) -> Result<String, ServerFnError> {
    use oauth_axum::OAuthClient;
    use crate::api::get_client;
    use crate::state::garden_state;

    let garden_state = garden_state()?;

    let (provider, scopes) = match provider.as_str() {
        "twitter" => {
            (get_client(), Vec::from([
                "users.read".to_string(),
                "tweet.read".to_string(), // tweet.read is also required to read user info
                "offline.access".to_string(), // needed too?
            ]))
        },
        x => {
            return Err(ServerFnError::ServerError(format!("{x} OAuth2 provider is not supported")))
        }
    };

    let state_oauth = provider
        .generate_url(scopes,
            |state_e| async move {
                garden_state.oauth_states.lock().await.insert(state_e.state, state_e.verifier);
            },
        )
        .await
        .unwrap()
        .state
        .unwrap();

    Ok(state_oauth.url_generated.unwrap())
}

#[component]
pub fn LoginScreen() -> impl IntoView {
    let go_to_auth = move |_| {
        spawn_local(async {
            let url = get_oauth_url("twitter".to_string()).await;
            console_log(&format!("oauth url is: {:?}", url));
        });
    };

    view! {
        <h1>"Login"</h1>
        <button on:click=go_to_auth>
            "Do Twitter Auth"
        </button>
    }
}
