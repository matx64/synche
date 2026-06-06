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

/// GUI routes — renders the index template at `/` and serves static
/// assets from `gui/static/` under `/static`.
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
            version => env!("CARGO_PKG_VERSION"),
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

        async fn gc_tombstones(&self, _cutoff_ms: i64) -> PersistenceResult<u64> {
            Ok(0)
        }
    }

    async fn create_test_components() -> (
        crate::utils::test_support::TestEnv,
        Arc<AppState>,
        Arc<PeerManager>,
        Arc<EntryManager<MockPersistence>>,
        Environment<'static>,
    ) {
        let env = crate::utils::test_support::test_env().await;
        let state = env.state.clone();
        let peer_manager = PeerManager::new(state.clone());
        let mock_db = MockPersistence::new();
        let entry_manager = EntryManager::new(mock_db, state.clone());
        let engine = init_template_engine();

        (env, state, peer_manager, entry_manager, engine)
    }

    #[tokio::test]
    async fn test_index_renders_with_metadata() {
        let (_env, state, pm, em, engine) = create_test_components().await;
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
        assert!(
            html.contains(env!("CARGO_PKG_VERSION")),
            "Should contain crate version in footer"
        );
        assert!(
            html.contains("class=\"app-shell\""),
            "Should render the dashboard shell"
        );
        assert!(
            html.contains("class=\"brand-rail\""),
            "Should render the brand rail instead of a card-style header"
        );
        assert!(
            html.contains("class=\"dashboard\""),
            "Should render the dashboard content grid"
        );
        assert!(
            html.contains("id=\"toast-region\""),
            "Should include the global toast region"
        );
        assert!(
            html.contains("id=\"add-dir-error\""),
            "Should include add directory inline error"
        );
        assert!(
            html.contains("id=\"remove-dir-error\""),
            "Should include remove directory inline error"
        );
        assert!(
            html.contains("id=\"home-path-error\""),
            "Should include home path inline error"
        );
        assert!(
            html.contains(
                "No devices yet &mdash; start Synche on another device on the same network"
            ),
            "Should include the peers empty state copy"
        );
        assert!(
            html.contains("Add a folder under your home path to start syncing"),
            "Should include the sync directories empty state copy"
        );
        assert!(
            !html.contains("class=\"app-header\""),
            "The old card-style app header should not be rendered"
        );
    }

    #[test]
    fn test_gui_static_js_uses_shared_api_error_feedback() {
        let helper = include_str!("../../../../gui/static/api_feedback.js");
        let main = include_str!("../../../../gui/static/main.js");
        let components = include_str!("../../../../gui/static/components.js");

        assert!(
            helper.contains("Could not reach Synche."),
            "Network errors should have a reusable user-facing message"
        );
        assert!(
            helper.contains("HTTP ${response.status}: ${reason}"),
            "HTTP failures should include status and reason"
        );
        assert!(
            main.contains("requestApi(`/api/add-sync-dir"),
            "Add sync dir should use the shared request helper"
        );
        assert!(
            main.contains("A directory with that name is already synced."),
            "Duplicate sync dir errors should have a specific reason"
        );
        assert!(
            main.contains("requestApi(\n      `/api/remove-sync-dir"),
            "Remove sync dir should use the shared request helper"
        );
        assert!(
            main.contains("requestApi(\n    `/api/set-home-path"),
            "Set home path should use the shared request helper"
        );
        assert!(
            components.contains("requestApi(\"/api/conflicts\""),
            "Conflict refresh should use the shared request helper"
        );
    }

    #[test]
    fn test_gui_visual_refresh_contract() {
        let template = include_str!("../../../../gui/index.html");
        let components = include_str!("../../../../gui/static/components.js");
        let main = include_str!("../../../../gui/static/main.js");
        let styles = include_str!("../../../../gui/static/style.css");

        assert!(
            template.contains("data-empty-state=\"peers\""),
            "Peer list should include a template-rendered empty state"
        );
        assert!(
            template.contains("data-empty-state=\"dirs\""),
            "Directory list should include a template-rendered empty state"
        );
        assert!(
            components.contains("function setEmptyStateVisibility"),
            "List rendering should keep empty states in sync after live updates"
        );
        assert!(
            components.contains("updatePeerEmptyState(listElement)"),
            "Adding peers should hide the peer empty state"
        );
        assert!(
            components.contains("updateDirEmptyState();"),
            "Removing directories should reveal the directory empty state"
        );
        assert!(
            template.contains("class=\"app-shell\""),
            "Template should expose the app shell"
        );
        assert!(
            template.contains("class=\"brand-rail\""),
            "Template should expose the persistent brand rail"
        );
        assert!(
            template.contains("class=\"dashboard-panel panel-primary\""),
            "Directories should be presented as the primary dashboard panel"
        );
        assert!(
            components.contains("list-item dir-item"),
            "Dynamic directory rows should match the redesigned row classes"
        );
        assert!(
            components.contains("status-pill status-pill-online"),
            "Dynamic peer rows should use the redesigned status pill"
        );
        assert!(
            main.contains("syncEmptyStates();"),
            "Initial page load should reconcile server-rendered empty states"
        );
        for token in [
            "--space-1:",
            "--space-4:",
            "--font-size-base:",
            "--line-height-base:",
            "--brand-primary:",
            "--accent-indigo:",
            "--accent-cyan:",
            "--accent-amber:",
            "--danger-color:",
        ] {
            assert!(
                styles.contains(token),
                "Styles should define the visual scale token {token}"
            );
        }
        assert!(
            styles.contains("#04745c"),
            "Palette should remain anchored to the logo green"
        );
        assert!(
            styles.contains(".brand-rail"),
            "Styles should define the brand rail"
        );
        assert!(
            styles.contains(".dashboard-panel"),
            "Styles should define the dashboard panels"
        );
        assert!(
            styles.contains("prefers-color-scheme: dark"),
            "Styles should keep an explicit dark-mode palette"
        );
        assert!(
            styles.contains(":focus-visible"),
            "Styles should include keyboard focus states"
        );
        assert!(
            styles.contains("dialog::backdrop"),
            "Dialog styling should include backdrop treatment"
        );
    }
}
