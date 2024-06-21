use leptos::{
    component, leptos_dom::logging::console_log, server, server_fn::error::NoCustomError,
    spawn_local, view, IntoView, ServerFnError, SignalWith,
};


#[server(GetOauthUrl, "/api")]
pub async fn get_oauth_url(provider: String) -> Result<String, ServerFnError> {
    use crate::api::get_client;
    use crate::state::garden_state;
    use oauth_axum::OAuthClient;

    let garden_state = garden_state()?;

    let (provider, scopes) = get_client(&provider)
        .map_err(|err| ServerFnError::ServerError::<NoCustomError>(err.to_string()))?;

    let state_oauth = provider
        .generate_url(scopes, |state_e| async move {
            garden_state
                .oauth_states
                .lock()
                .await
                .insert(state_e.state, state_e.verifier);
        })
        .await
        .unwrap()
        .state
        .unwrap();

    Ok(state_oauth.url_generated.unwrap())
}

#[component]
pub fn LoginScreen(
) -> impl IntoView {
    // let go_to_auth = move |_| {
    //     spawn_local(async {
    //         let url = get_oauth_url("twitter".to_string()).await;
    //         console_log(&format!("oauth url is: {:?}", url));
    //     });
    // };

    view! {
        <h1>"Login"</h1>
        <a href="/api/oauth/begin/twitter" rel="external">"Login via Twitter"</a>
        // <button on:click=go_to_auth>
        //     "Do Twitter Auth"
        // </button>
    }
}
