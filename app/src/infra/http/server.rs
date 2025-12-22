use crate::{
    application::{
        AppState, EntryManager, PeerManager, persistence::interface::PersistenceInterface,
    },
    infra::http::routes,
};
use minijinja::Environment;
use std::sync::Arc;
use tokio::net::TcpListener;

pub async fn run<P: PersistenceInterface>(
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
) -> tokio::io::Result<()> {
    let port = state.ports().http;
    let template_engine = init_template_engine();

    let router = routes::build_router(state, peer_manager, entry_manager, template_engine);

    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Web GUI: http://{addr}");
    axum::serve(listener, router).await
}

pub fn init_template_engine() -> Environment<'static> {
    let mut engine = Environment::new();
    engine
        .add_template("index", include_str!("../../../../gui/index.html"))
        .unwrap();
    engine
}
