use crate::{
    application::{
        AppState, EntryManager, PeerManager, ResolveAction,
        persistence::interface::PersistenceInterface,
    },
    domain::{RelativePath, TransportChannelData},
};
use async_stream::try_stream;
use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{Sse, sse::Event},
    routing::{get, post},
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, sync::Arc};
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Clone)]
struct ApiState<P: PersistenceInterface> {
    pub state: Arc<AppState>,
    #[allow(dead_code)]
    pub peer_manager: Arc<PeerManager>,
    pub entry_manager: Arc<EntryManager<P>>,
    pub sender_tx: mpsc::Sender<TransportChannelData>,
}

#[derive(Deserialize)]
struct ModifySyncDirParams {
    pub name: RelativePath,
}

#[derive(Deserialize)]
struct SetHomePathParams {
    pub path: String,
}

#[derive(Serialize)]
struct InfoResponse {
    pub version: &'static str,
    pub device_id: Uuid,
    pub instance_id: Uuid,
    pub hostname: String,
}

#[derive(Deserialize)]
struct ResolveConflictParams {
    /// Relative path of the `_CONFLICT_` copy to resolve.
    pub path: RelativePath,
    /// `keep-mine` (delete the copy, keep the original) or `keep-theirs`
    /// (overwrite the original with the copy, then delete the copy).
    pub action: String,
}

#[derive(Serialize)]
struct ConflictsResponse {
    pub conflicts: Vec<ConflictGroup>,
}

#[derive(Serialize)]
struct ConflictGroup {
    pub dir: RelativePath,
    pub items: Vec<ConflictItem>,
}

#[derive(Serialize)]
struct ConflictItem {
    pub conflict_path: RelativePath,
    pub original_path: RelativePath,
    pub abs_path: String,
}

/// JSON API routes — peer listing, sync-directory management,
/// `home_path` updates, and the SSE stream of `ServerEvent`s.
pub fn routes<P: PersistenceInterface>(
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
    sender_tx: mpsc::Sender<TransportChannelData>,
) -> Router {
    let api_state = Arc::new(ApiState {
        state,
        peer_manager,
        entry_manager,
        sender_tx,
    });

    Router::new().nest(
        "/api",
        Router::new()
            .route("/events", get(sse_events::<P>))
            .route("/info", get(info::<P>))
            .route("/add-sync-dir", post(add_sync_dir::<P>))
            .route("/remove-sync-dir", post(remove_sync_dir::<P>))
            .route("/set-home-path", post(set_home_path::<P>))
            .route("/conflicts", get(conflicts::<P>))
            .route("/resolve-conflict", post(resolve_conflict::<P>))
            .with_state(api_state),
    )
}

async fn info<P: PersistenceInterface>(
    State(state): State<Arc<ApiState<P>>>,
) -> Json<InfoResponse> {
    Json(InfoResponse {
        version: env!("CARGO_PKG_VERSION"),
        device_id: state.state.local_id(),
        instance_id: state.state.instance_id(),
        hostname: state.state.hostname().clone(),
    })
}

async fn add_sync_dir<P: PersistenceInterface>(
    State(state): State<Arc<ApiState<P>>>,
    Query(params): Query<ModifySyncDirParams>,
) -> StatusCode {
    let name = params.name.trim().into();

    match state.state.add_dir_to_config(&name).await {
        Ok(true) => {
            tracing::info!("Sync dir add requested: {name:?}");
            StatusCode::CREATED
        }
        Ok(false) => StatusCode::CONFLICT,
        Err(err) => {
            error!("Add sync dir error: {err}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn remove_sync_dir<P: PersistenceInterface>(
    State(state): State<Arc<ApiState<P>>>,
    Query(params): Query<ModifySyncDirParams>,
) -> StatusCode {
    let name = params.name.trim().into();

    match state.state.remove_dir_from_config(&name).await {
        Ok(_) => StatusCode::OK,
        Err(err) => {
            error!("Remove sync dir error: {err}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn set_home_path<P: PersistenceInterface>(
    State(state): State<Arc<ApiState<P>>>,
    Query(params): Query<SetHomePathParams>,
) -> StatusCode {
    match state.state.set_home_path_in_config(params.path).await {
        Ok(_) => StatusCode::OK,
        Err(err) => {
            error!("Set home path error: {err}");
            match err.kind() {
                std::io::ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            }
        }
    }
}

/// Lists the current conflict copies grouped by sync directory for the
/// GUI's Conflicts section.
async fn conflicts<P: PersistenceInterface>(
    State(state): State<Arc<ApiState<P>>>,
) -> Result<Json<ConflictsResponse>, StatusCode> {
    let infos = state.entry_manager.list_conflicts().await.map_err(|err| {
        error!("List conflicts error: {err}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut groups: Vec<ConflictGroup> = Vec::new();
    for info in infos {
        let item = ConflictItem {
            conflict_path: info.conflict_path,
            original_path: info.original_path,
            abs_path: info.abs_path,
        };
        match groups.iter_mut().find(|g| g.dir == info.dir) {
            Some(group) => group.items.push(item),
            None => groups.push(ConflictGroup {
                dir: info.dir,
                items: vec![item],
            }),
        }
    }

    Ok(Json(ConflictsResponse { conflicts: groups }))
}

/// Resolves a single conflict copy (`keep-mine` / `keep-theirs`) and
/// broadcasts the resulting metadata to peers so the resolution converges.
async fn resolve_conflict<P: PersistenceInterface>(
    State(state): State<Arc<ApiState<P>>>,
    Query(params): Query<ResolveConflictParams>,
) -> StatusCode {
    let action = match params.action.as_str() {
        "keep-mine" => ResolveAction::KeepMine,
        "keep-theirs" => ResolveAction::KeepTheirs,
        other => {
            warn!("Resolve conflict: unknown action {other:?}");
            return StatusCode::BAD_REQUEST;
        }
    };

    let path = params.path.trim().into();

    match state.entry_manager.resolve_conflict(&path, action).await {
        Ok(entries) => {
            tracing::info!("Conflict resolved ({action:?}): {path:?}");
            for entry in entries {
                if let Err(err) = state
                    .sender_tx
                    .send(TransportChannelData::Metadata(entry))
                    .await
                {
                    error!("Failed to broadcast resolved conflict metadata: {err}");
                }
            }
            StatusCode::OK
        }
        Err(err) => {
            error!("Resolve conflict error: {err}");
            match err.kind() {
                std::io::ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
                std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            }
        }
    }
}

async fn sse_events<P: PersistenceInterface>(
    State(state): State<Arc<ApiState<P>>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.state.sse_subscribe();

    Sse::new(try_stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    match serde_json::to_string(&event) {
                        Ok(data) => {
                            yield Event::default().data(data);
                            info!("Sent SSE: {:?}", event);
                        }
                        Err(err) => {
                            error!("Error serializing event: {err}");
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("SSE client lagged by {n} messages, continuing");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("SSE broadcast channel closed");
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{application::persistence::interface::PersistenceResult, domain::EntryInfo};
    use axum::http::StatusCode;
    use std::time::Duration;
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
    ) {
        let env = crate::utils::test_support::test_env().await;
        let state = env.state.clone();
        let peer_manager = PeerManager::new(state.clone());
        let mock_db = MockPersistence::new();
        let entry_manager = EntryManager::new(mock_db, state.clone());

        (env, state, peer_manager, entry_manager)
    }

    /// A transport sender whose receiver is intentionally leaked so the
    /// channel stays open for tests that never inspect broadcasts.
    fn test_sender() -> mpsc::Sender<TransportChannelData> {
        let (tx, rx) = mpsc::channel(8);
        std::mem::forget(rx);
        tx
    }

    #[tokio::test]
    async fn test_add_sync_dir_success() {
        let (_env, state, pm, em) = create_test_components().await;

        let unique_dir = format!("TestDir_{}", Uuid::new_v4());
        let dir_path = state.home_path().join(&unique_dir);
        tokio::fs::create_dir_all(&dir_path).await.ok();

        let api_state = Arc::new(ApiState {
            state: state.clone(),
            peer_manager: pm,
            entry_manager: em,
            sender_tx: test_sender(),
        });

        let params = ModifySyncDirParams {
            name: unique_dir.into(),
        };

        let status = add_sync_dir(State(api_state), Query(params)).await;

        assert_eq!(status, StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_add_sync_dir_duplicate() {
        // Seed `sync_dirs` via the initial config so the duplicate check
        // in `add_dir_to_config` trips. (In production the file watcher
        // populates that map from `config.toml` on startup.)
        let env =
            crate::utils::test_support::test_env_with_dirs(&["Default Folder", "DuplicateDir"])
                .await;
        let state = env.state.clone();
        let pm = PeerManager::new(state.clone());
        let em = EntryManager::new(MockPersistence::new(), state.clone());

        let test_dir_path = state.home_path().join("DuplicateDir");
        tokio::fs::create_dir_all(&test_dir_path).await.ok();

        let api_state = Arc::new(ApiState {
            state: state.clone(),
            peer_manager: pm,
            entry_manager: em,
            sender_tx: test_sender(),
        });

        let params = ModifySyncDirParams {
            name: "DuplicateDir".into(),
        };

        let status = add_sync_dir(State(api_state), Query(params)).await;

        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_add_sync_dir_trims_whitespace() {
        let (_env, state, pm, em) = create_test_components().await;

        let unique_dir = format!("TestDir_{}", Uuid::new_v4());
        let trimmed_dir = state.home_path().join(&unique_dir);
        tokio::fs::create_dir_all(&trimmed_dir).await.ok();

        let api_state = Arc::new(ApiState {
            state: state.clone(),
            peer_manager: pm,
            entry_manager: em,
            sender_tx: test_sender(),
        });

        let params = ModifySyncDirParams {
            name: format!("  {}  ", unique_dir).into(),
        };

        let status = add_sync_dir(State(api_state), Query(params)).await;

        assert_eq!(
            status,
            StatusCode::CREATED,
            "Whitespace should be trimmed and directory added"
        );
    }

    #[tokio::test]
    async fn test_remove_sync_dir_success() {
        let (_env, state, pm, em) = create_test_components().await;

        let test_dir = RelativePath::from("RemoveMe");
        state.add_dir_to_config(&test_dir).await.ok();

        let api_state = Arc::new(ApiState {
            state: state.clone(),
            peer_manager: pm,
            entry_manager: em,
            sender_tx: test_sender(),
        });

        let params = ModifySyncDirParams {
            name: "RemoveMe".into(),
        };

        let status = remove_sync_dir(State(api_state), Query(params)).await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn test_remove_sync_dir_nonexistent() {
        let (_env, state, pm, em) = create_test_components().await;
        let api_state = Arc::new(ApiState {
            state,
            peer_manager: pm,
            entry_manager: em,
            sender_tx: test_sender(),
        });

        let params = ModifySyncDirParams {
            name: "NonExistent".into(),
        };

        let status = remove_sync_dir(State(api_state), Query(params)).await;

        assert_eq!(
            status,
            StatusCode::OK,
            "Removing nonexistent should be idempotent"
        );
    }

    #[tokio::test]
    async fn test_remove_sync_dir_trims_whitespace() {
        let (_env, state, pm, em) = create_test_components().await;

        let test_dir = RelativePath::from("TrimTest");
        state.add_dir_to_config(&test_dir).await.ok();

        let api_state = Arc::new(ApiState {
            state,
            peer_manager: pm,
            entry_manager: em,
            sender_tx: test_sender(),
        });

        let params = ModifySyncDirParams {
            name: "  TrimTest  ".into(),
        };

        let status = remove_sync_dir(State(api_state), Query(params)).await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn test_set_home_path_success() {
        let (_env, state, pm, em) = create_test_components().await;
        let api_state = Arc::new(ApiState {
            state,
            peer_manager: pm,
            entry_manager: em,
            sender_tx: test_sender(),
        });

        let temp_dir = tempfile::tempdir().unwrap();
        let new_path = temp_dir.path().to_str().unwrap().to_string();

        let params = SetHomePathParams { path: new_path };

        let status = set_home_path(State(api_state), Query(params)).await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn test_set_home_path_invalid() {
        let (_env, state, pm, em) = create_test_components().await;
        let api_state = Arc::new(ApiState {
            state,
            peer_manager: pm,
            entry_manager: em,
            sender_tx: test_sender(),
        });

        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("file.txt");
        std::fs::write(&file_path, "test").unwrap();

        let params = SetHomePathParams {
            path: file_path.to_str().unwrap().to_string(),
        };

        let status = set_home_path(State(api_state), Query(params)).await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "File path should be rejected"
        );
    }

    #[tokio::test]
    async fn test_info_returns_version_and_ids() {
        let (_env, state, pm, em) = create_test_components().await;
        let api_state = Arc::new(ApiState {
            state: state.clone(),
            peer_manager: pm,
            entry_manager: em,
            sender_tx: test_sender(),
        });

        let Json(body) = info(State(api_state)).await;

        assert_eq!(body.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(body.device_id, state.local_id());
        assert_eq!(body.instance_id, state.instance_id());
        assert_eq!(&body.hostname, state.hostname());
    }

    #[tokio::test]
    async fn test_sse_broadcast_channel() {
        let (_env, state, _pm, _em) = create_test_components().await;
        let sender = state.sse_sender();
        let mut receiver = state.sse_subscribe();

        let test_event = crate::domain::ServerEvent::SyncDirectoryAdded("TestDir".into());
        sender.send(test_event.clone()).unwrap();

        let result = tokio::time::timeout(Duration::from_millis(500), receiver.recv()).await;

        assert!(result.is_ok(), "Should receive event within timeout");
        assert!(
            result.unwrap().is_ok(),
            "Event should be received successfully"
        );
    }

    fn conflict_entry(name: &RelativePath, device: Uuid) -> EntryInfo {
        EntryInfo {
            name: name.clone(),
            kind: crate::domain::EntryKind::File,
            hash: Some("h".into()),
            version: std::collections::HashMap::from([(device, 1)]),
            deleted: false,
        }
    }

    #[tokio::test]
    async fn test_conflicts_endpoint_groups_by_dir() {
        let env = crate::utils::test_support::test_env_with_dirs(&["sync"]).await;
        let state = env.state.clone();
        let pm = PeerManager::new(state.clone());
        let em = EntryManager::new(MockPersistence::new(), state.clone());
        let device = state.local_id();

        let name = RelativePath::conflict_file_name("img", "jpg", 1, device, "a1b2c3d4");
        let conflict_rel: RelativePath = format!("sync/{name}").into();
        em.insert_entry(conflict_entry(&conflict_rel, device))
            .await
            .unwrap();

        let api_state = Arc::new(ApiState {
            state,
            peer_manager: pm,
            entry_manager: em,
            sender_tx: test_sender(),
        });

        let Json(body) = conflicts(State(api_state)).await.unwrap();

        assert_eq!(body.conflicts.len(), 1);
        assert_eq!(body.conflicts[0].dir, "sync".into());
        assert_eq!(body.conflicts[0].items.len(), 1);
        assert_eq!(body.conflicts[0].items[0].conflict_path, conflict_rel);
        assert_eq!(
            body.conflicts[0].items[0].original_path,
            "sync/img.jpg".into()
        );
    }

    #[tokio::test]
    async fn test_resolve_conflict_unknown_action_is_bad_request() {
        let (_env, state, pm, em) = create_test_components().await;
        let api_state = Arc::new(ApiState {
            state,
            peer_manager: pm,
            entry_manager: em,
            sender_tx: test_sender(),
        });

        let params = ResolveConflictParams {
            path: "sync/x".into(),
            action: "bogus".into(),
        };

        let status = resolve_conflict(State(api_state), Query(params)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_resolve_conflict_non_conflict_path_is_bad_request() {
        let (_env, state, pm, em) = create_test_components().await;
        let api_state = Arc::new(ApiState {
            state,
            peer_manager: pm,
            entry_manager: em,
            sender_tx: test_sender(),
        });

        let params = ResolveConflictParams {
            path: "sync/not-a-conflict.txt".into(),
            action: "keep-mine".into(),
        };

        let status = resolve_conflict(State(api_state), Query(params)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_resolve_conflict_missing_entry_is_not_found() {
        let env = crate::utils::test_support::test_env_with_dirs(&["sync"]).await;
        let state = env.state.clone();
        let pm = PeerManager::new(state.clone());
        let em = EntryManager::new(MockPersistence::new(), state.clone());
        let device = state.local_id();

        let name = RelativePath::conflict_file_name("img", "jpg", 1, device, "a1b2c3d4");
        let api_state = Arc::new(ApiState {
            state,
            peer_manager: pm,
            entry_manager: em,
            sender_tx: test_sender(),
        });

        let params = ResolveConflictParams {
            path: format!("sync/{name}").into(),
            action: "keep-mine".into(),
        };

        let status = resolve_conflict(State(api_state), Query(params)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_resolve_conflict_keep_mine_broadcasts_tombstone() {
        let env = crate::utils::test_support::test_env_with_dirs(&["sync"]).await;
        let state = env.state.clone();
        let pm = PeerManager::new(state.clone());
        let em = EntryManager::new(MockPersistence::new(), state.clone());
        let device = state.local_id();

        let sync_abs = state.home_path().join("sync");
        tokio::fs::create_dir_all(&sync_abs).await.unwrap();
        let name = RelativePath::conflict_file_name("img", "jpg", 1, device, "a1b2c3d4");
        let conflict_rel: RelativePath = format!("sync/{name}").into();
        tokio::fs::write(sync_abs.join(&name), b"copy")
            .await
            .unwrap();
        em.insert_entry(conflict_entry(&conflict_rel, device))
            .await
            .unwrap();

        let (tx, mut rx) = mpsc::channel(8);
        let api_state = Arc::new(ApiState {
            state: state.clone(),
            peer_manager: pm,
            entry_manager: em,
            sender_tx: tx,
        });

        let params = ResolveConflictParams {
            path: conflict_rel.clone(),
            action: "keep-mine".into(),
        };

        let status = resolve_conflict(State(api_state), Query(params)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(!sync_abs.join(&name).exists(), "copy must be removed");

        match rx.try_recv().expect("a tombstone must be broadcast") {
            TransportChannelData::Metadata(entry) => {
                assert_eq!(entry.name, conflict_rel);
                assert!(entry.is_removed());
            }
            _ => panic!("expected a Metadata broadcast"),
        }
    }
}
