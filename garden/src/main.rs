#[cfg(feature = "ssr")]
use axum::{
    extract::State,
    response::{IntoResponse, Response},
};
#[cfg(feature = "ssr")]
use axum::{body::Body as AxumBody, http::Request};

#[cfg(feature = "ssr")]
use garden::state::AppState;

#[cfg(feature = "ssr")]
use leptos::provide_context;

#[cfg(feature = "ssr")]
use garden::app::App;

#[cfg(feature = "ssr")]
use leptos_axum::handle_server_fns_with_context;

#[cfg(feature = "ssr")]
async fn server_fn_handler(
    State(app_state): State<AppState>,
    request: Request<AxumBody>,
) -> impl IntoResponse {
    handle_server_fns_with_context(
        move || {
            provide_context(app_state.garden_state.clone());
        },
        request,
    )
    .await
}

#[cfg(feature = "ssr")]
async fn leptos_routes_handler(
    State(app_state): State<AppState>,
    req: Request<AxumBody>,
) -> Response {
    let handler = leptos_axum::render_route_with_context(
        app_state.leptos_options.clone(),
        app_state.routes.clone(),
        move || {
            provide_context(app_state.garden_state.clone());
        },
        App,
    );
    handler(req).await.into_response()
}

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use std::sync::Arc;

    use axum::routing::get;
    use axum::Router;
    use garden::api::api_routes;
    use garden::state::GardenState;
    use leptos::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use garden::app::*;
    use garden::fileserv::file_and_error_handler;

    // Setting get_configuration(None) means we'll be using cargo-leptos's env values
    // For deployment these variables are:
    // <https://github.com/leptos-rs/start-axum#executing-a-server-on-a-remote-machine-without-the-toolchain>
    // Alternately a file can be specified such as Some("Cargo.toml")
    // The file would need to be included with the executable when moved to deployment
    let conf = get_configuration(None).await.unwrap();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(App);

    let app_state = AppState {
        leptos_options,
        garden_state: Arc::new(GardenState::default()),
        routes: routes.clone(),
    };

    // build our application with a route
    let app = Router::new()
        .route(
            "/api/*fn_name",
            get(server_fn_handler).post(server_fn_handler),
        )
        .leptos_routes_with_handler(routes, get(leptos_routes_handler))
        .nest("/api", api_routes())
        .fallback(file_and_error_handler)
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    logging::log!("listening on http://{}", &addr);
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for a purely client-side app
    // see lib.rs for hydration function instead
}
