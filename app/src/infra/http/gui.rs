use crate::application::{
    AppState, EntryManager, PeerManager, persistence::interface::PersistenceInterface,
};
use axum::{Router, extract::State, http::StatusCode, response::Html, routing::get};
use minijinja::{Environment, context};
use serde::Serialize;
use std::sync::Arc;
use tower_http::services::ServeDir;
use uuid::Uuid;

#[derive(Serialize)]
struct PeerView {
    id: Uuid,
    id_short: String,
    addr: std::net::IpAddr,
    hostname: String,
    last_seen: u64,
    sync_dirs: Vec<String>,
}

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
    let peers = state
        .peer_manager
        .list()
        .await
        .into_iter()
        .map(|peer| PeerView {
            id: peer.id,
            id_short: compact_device_id(&peer.id),
            addr: peer.addr,
            hostname: peer.hostname,
            last_seen: peer
                .last_seen
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            sync_dirs: peer
                .sync_dirs
                .into_keys()
                .map(|dir| dir.to_string())
                .collect(),
        })
        .collect::<Vec<_>>();

    let tmpl = state
        .engine
        .get_template("index")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rendered = tmpl
        .render(context! {
            dirs => dirs,
            hostname => state.state.hostname(),
            local_id => state.state.local_id(),
            local_id_short => compact_device_id(&state.state.local_id()),
            peers => peers,
            local_ip => state.state.local_ip().await,
            home_path => state.state.home_path().display().to_string(),
            version => env!("CARGO_PKG_VERSION"),
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Html(rendered))
}

fn compact_device_id(id: &impl std::fmt::Display) -> String {
    let value = id.to_string();
    if value.len() <= 13 {
        return value;
    }

    format!("{}...{}", &value[..8], &value[value.len() - 4..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::persistence::interface::PersistenceResult, domain::EntryInfo,
        infra::http::server::init_template_engine,
    };
    use std::{
        collections::HashMap,
        net::{IpAddr, Ipv4Addr},
        time::SystemTime,
    };
    use tokio::sync::Mutex;
    use uuid::Uuid;

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

    fn peer_with_sync_dir() -> crate::domain::Peer {
        let dir = crate::domain::RelativePath::from("Docs");
        crate::domain::Peer {
            id: Uuid::new_v4(),
            addr: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 10)),
            transport_port: 52882,
            hostname: "peer-host".into(),
            instance_id: Uuid::new_v4(),
            last_seen: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_717_000_000),
            sync_dirs: HashMap::from([(dir.clone(), crate::domain::SyncDirectory { name: dir })]),
        }
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
            html.contains(&compact_device_id(&state.local_id())),
            "Should contain compact local_id display"
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
    }

    #[tokio::test]
    async fn test_index_renders_connected_peer_details() {
        let (_env, state, pm, em, engine) = create_test_components().await;
        let peer = peer_with_sync_dir();
        let peer_id = peer.id.to_string();
        let peer_id_short = compact_device_id(&peer.id);
        pm.insert(peer).await;

        let gui_state = Arc::new(GuiState {
            state,
            engine,
            peer_manager: pm,
            entry_manager: em,
        });

        let result = index(State(gui_state)).await;
        assert!(
            result.is_ok(),
            "Index should render successfully with peers"
        );

        let Html(html) = result.unwrap();
        assert!(html.contains("peer-host"), "Should render peer hostname");
        assert!(html.contains("10.0.0.10"), "Should render peer IP");
        assert!(
            html.contains(&peer_id),
            "Should render full peer id (tooltip / sr-only)"
        );
        assert!(
            html.contains(&peer_id_short),
            "Should render the compact peer id"
        );
        assert!(
            html.contains("data-ts=\"1717000000\""),
            "Should render the serialized last-seen epoch seconds"
        );
        assert!(
            html.contains("<li>Docs</li>"),
            "Should render shared sync dirs"
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
            main.contains("requestApi") && main.contains("/api/add-sync-dir"),
            "Add sync dir should use the shared request helper"
        );
        assert!(
            main.contains("A directory with that name is already synced."),
            "Duplicate sync dir errors should have a specific reason"
        );
        assert!(
            main.contains("requestApi") && main.contains("/api/remove-sync-dir"),
            "Remove sync dir should use the shared request helper"
        );
        assert!(
            main.contains("requestApi") && main.contains("/api/set-home-path"),
            "Set home path should use the shared request helper"
        );
        assert!(
            components.contains("requestApi") && components.contains("/api/conflicts"),
            "Conflict refresh should use the shared request helper"
        );
    }
}
