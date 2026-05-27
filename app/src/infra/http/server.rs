use crate::{
    application::{
        AppState, EntryManager, PeerManager, persistence::interface::PersistenceInterface,
    },
    infra::http::routes,
};
use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::{self, Next},
    response::Response,
};
use minijinja::Environment;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

const VERSION_HEADER: HeaderName = HeaderName::from_static("x-synche-version");
const VERSION_HEADER_VALUE: HeaderValue = HeaderValue::from_static(env!("CARGO_PKG_VERSION"));

/// Binds the HTTP listener on the configured port and serves the GUI
/// and JSON API. Runs until the listener errors or the task is
/// cancelled by the synchronizer's `tokio::select!`.
pub async fn run<P: PersistenceInterface>(
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
) -> tokio::io::Result<()> {
    let port = state.ports().http;
    let template_engine = init_template_engine();

    let router = routes::build_router(state, peer_manager, entry_manager, template_engine)
        .layer(middleware::from_fn(insert_version_header))
        .layer(TraceLayer::new_for_http());

    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Web GUI: http://{addr}");
    axum::serve(listener, router).await
}

/// Inserts `X-Synche-Version: <CARGO_PKG_VERSION>` on every outgoing
/// response so scripted health checks and curl can read the version
/// without parsing a JSON body.
async fn insert_version_header(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(VERSION_HEADER, VERSION_HEADER_VALUE);
    response
}

/// Builds the minijinja environment with the GUI template embedded at
/// compile time so no runtime template lookup is needed.
pub fn init_template_engine() -> Environment<'static> {
    let mut engine = Environment::new();
    engine
        .add_template("index", include_str!("../../../../gui/index.html"))
        .unwrap();
    engine
}
