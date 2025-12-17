use crate::application::{
    AppState, EntryManager, PeerManager, persistence::interface::PersistenceInterface,
};
use axum::{Router, extract::State, http::StatusCode, response::Html, routing::get};
use minijinja::{Environment, context};
use std::sync::Arc;
use tower_http::services::ServeDir;

struct GuiState<P: PersistenceInterface> {
    pub state: Arc<AppState>,
    pub engine: Environment<'static>,
    pub peer_manager: Arc<PeerManager>,
    pub entry_manager: Arc<EntryManager<P>>,
}

pub fn routes<P: PersistenceInterface>(
    state: Arc<AppState>,
    engine: Environment<'static>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
) -> Router {
    let gui_state = Arc::new(GuiState {
        state,
        peer_manager,
        entry_manager,
        engine,
    });

    Router::new()
        .route("/", get(index::<P>))
        .with_state(gui_state)
        .nest_service("/static", ServeDir::new("./gui/static"))
}

async fn index<P: PersistenceInterface>(
    State(state): State<Arc<GuiState<P>>>,
) -> Result<Html<String>, StatusCode> {
    let dirs = state.entry_manager.list_dirs().await;
    let dirs: Vec<_> = dirs.values().cloned().collect();

    let tmpl = state
        .engine
        .get_template("index")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rendered = tmpl
        .render(context! {
            dirs => dirs,
            hostname => state.state.hostname(),
            local_id => state.state.local_id(),
            peers => state.peer_manager.list().await,
            local_ip => state.state.local_ip().await,
            home_path => state.state.home_path().display().to_string(),
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Html(rendered))
}
