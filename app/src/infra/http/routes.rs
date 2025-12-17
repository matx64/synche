use crate::{
    application::{
        AppState, EntryManager, PeerManager, persistence::interface::PersistenceInterface,
    },
    infra::http::{api, gui},
};
use axum::Router;
use minijinja::Environment;
use std::sync::Arc;

pub fn build_router<P: PersistenceInterface>(
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
    template_engine: Environment<'static>,
) -> Router {
    Router::new()
        .merge(gui::routes(
            state.clone(),
            template_engine,
            peer_manager.clone(),
            entry_manager.clone(),
        ))
        .merge(api::routes(state, peer_manager, entry_manager))
}
