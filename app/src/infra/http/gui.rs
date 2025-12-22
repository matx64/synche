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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::persistence::interface::PersistenceResult, domain::EntryInfo,
        infra::http::server::init_template_engine,
    };
    use tokio::sync::Mutex;

    struct MockPersistence {
        entries: Arc<Mutex<Vec<EntryInfo>>>,
    }

    impl MockPersistence {
        fn new() -> Self {
            Self {
                entries: Arc::new(Mutex::new(vec![])),
            }
        }
    }

    #[async_trait::async_trait]
    impl PersistenceInterface for MockPersistence {
        async fn insert_or_replace_entry(&self, entry: &EntryInfo) -> PersistenceResult<()> {
            self.entries.lock().await.push(entry.clone());
            Ok(())
        }

        async fn get_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>> {
            Ok(self
                .entries
                .lock()
                .await
                .iter()
                .find(|e| &*e.name == name)
                .cloned())
        }

        async fn list_all_entries(&self) -> PersistenceResult<Vec<EntryInfo>> {
            Ok(self.entries.lock().await.clone())
        }

        async fn delete_entry(&self, name: &str) -> PersistenceResult<()> {
            self.entries.lock().await.retain(|e| &*e.name != name);
            Ok(())
        }
    }

    async fn create_test_components() -> (
        Arc<AppState>,
        Arc<PeerManager>,
        Arc<EntryManager<MockPersistence>>,
        Environment<'static>,
    ) {
        let state = AppState::new().await;
        let peer_manager = PeerManager::new(state.clone());
        let mock_db = MockPersistence::new();
        let entry_manager = EntryManager::new(mock_db, state.clone());
        let engine = init_template_engine();

        (state, peer_manager, entry_manager, engine)
    }

    #[tokio::test]
    async fn test_index_renders_with_metadata() {
        let (state, pm, em, engine) = create_test_components().await;
        let gui_state = Arc::new(GuiState {
            state: state.clone(),
            engine,
            peer_manager: pm,
            entry_manager: em,
        });

        let result = index(State(gui_state)).await;

        assert!(result.is_ok(), "Index should render successfully");

        let Html(html) = result.unwrap();
        assert!(
            html.contains(state.hostname().as_str()),
            "Should contain hostname"
        );
        assert!(
            html.contains(&state.local_id().to_string()),
            "Should contain local_id"
        );
        assert!(
            html.contains(&state.local_ip().await.to_string()),
            "Should contain local_ip"
        );
    }
}
