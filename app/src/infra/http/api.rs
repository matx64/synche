use crate::{
    application::{
        AppState, EntryManager, PeerManager, persistence::interface::PersistenceInterface,
    },
    domain::RelativePath,
};
use async_stream::try_stream;
use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    response::{Sse, sse::Event},
    routing::{get, post},
};
use futures_util::stream::Stream;
use serde::Deserialize;
use std::{convert::Infallible, sync::Arc};
use tokio::sync::broadcast;
use tracing::{error, info, warn};

#[derive(Clone)]
struct ApiState<P: PersistenceInterface> {
    pub state: Arc<AppState>,
    #[allow(dead_code)]
    pub peer_manager: Arc<PeerManager>,
    #[allow(dead_code)]
    pub entry_manager: Arc<EntryManager<P>>,
}

#[derive(Deserialize)]
struct ModifySyncDirParams {
    pub name: RelativePath,
}

#[derive(Deserialize)]
struct SetHomePathParams {
    pub path: String,
}

pub fn routes<P: PersistenceInterface>(
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
) -> Router {
    let api_state = Arc::new(ApiState {
        state,
        peer_manager,
        entry_manager,
    });

    Router::new().nest(
        "/api",
        Router::new()
            .route("/events", get(sse_events::<P>))
            .route("/add-sync-dir", post(add_sync_dir::<P>))
            .route("/remove-sync-dir", post(remove_sync_dir::<P>))
            .route("/set-home-path", post(set_home_path::<P>))
            .with_state(api_state),
    )
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
    }

    async fn create_test_components() -> (
        Arc<AppState>,
        Arc<PeerManager>,
        Arc<EntryManager<MockPersistence>>,
    ) {
        let state = AppState::new().await;
        let peer_manager = PeerManager::new(state.clone());
        let mock_db = MockPersistence::new();
        let entry_manager = EntryManager::new(mock_db, state.clone());

        (state, peer_manager, entry_manager)
    }

    #[tokio::test]
    async fn test_add_sync_dir_success() {
        let (state, pm, em) = create_test_components().await;

        let unique_dir = format!("TestDir_{}", Uuid::new_v4());
        let dir_path = state.home_path().join(&unique_dir);
        tokio::fs::create_dir_all(&dir_path).await.ok();

        let api_state = Arc::new(ApiState {
            state: state.clone(),
            peer_manager: pm,
            entry_manager: em,
        });

        let params = ModifySyncDirParams {
            name: unique_dir.into(),
        };

        let status = add_sync_dir(State(api_state), Query(params)).await;

        assert_eq!(status, StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_add_sync_dir_duplicate() {
        let (state, pm, em) = create_test_components().await;

        let test_dir = RelativePath::from("DuplicateDir");
        let test_dir_path = state
            .home_path()
            .join(<RelativePath as AsRef<str>>::as_ref(&test_dir));
        tokio::fs::create_dir_all(&test_dir_path).await.ok();
        state.add_dir_to_config(&test_dir).await.ok();

        let api_state = Arc::new(ApiState {
            state: state.clone(),
            peer_manager: pm,
            entry_manager: em,
        });

        let params = ModifySyncDirParams {
            name: "DuplicateDir".into(),
        };

        let status = add_sync_dir(State(api_state), Query(params)).await;

        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_add_sync_dir_trims_whitespace() {
        let (state, pm, em) = create_test_components().await;

        let unique_dir = format!("TestDir_{}", Uuid::new_v4());
        let trimmed_dir = state.home_path().join(&unique_dir);
        tokio::fs::create_dir_all(&trimmed_dir).await.ok();

        let api_state = Arc::new(ApiState {
            state: state.clone(),
            peer_manager: pm,
            entry_manager: em,
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
        let (state, pm, em) = create_test_components().await;

        let test_dir = RelativePath::from("RemoveMe");
        state.add_dir_to_config(&test_dir).await.ok();

        let api_state = Arc::new(ApiState {
            state: state.clone(),
            peer_manager: pm,
            entry_manager: em,
        });

        let params = ModifySyncDirParams {
            name: "RemoveMe".into(),
        };

        let status = remove_sync_dir(State(api_state), Query(params)).await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn test_remove_sync_dir_nonexistent() {
        let (state, pm, em) = create_test_components().await;
        let api_state = Arc::new(ApiState {
            state,
            peer_manager: pm,
            entry_manager: em,
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
        let (state, pm, em) = create_test_components().await;

        let test_dir = RelativePath::from("TrimTest");
        state.add_dir_to_config(&test_dir).await.ok();

        let api_state = Arc::new(ApiState {
            state,
            peer_manager: pm,
            entry_manager: em,
        });

        let params = ModifySyncDirParams {
            name: "  TrimTest  ".into(),
        };

        let status = remove_sync_dir(State(api_state), Query(params)).await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn test_set_home_path_success() {
        let (state, pm, em) = create_test_components().await;
        let api_state = Arc::new(ApiState {
            state,
            peer_manager: pm,
            entry_manager: em,
        });

        let temp_dir = tempfile::tempdir().unwrap();
        let new_path = temp_dir.path().to_str().unwrap().to_string();

        let params = SetHomePathParams { path: new_path };

        let status = set_home_path(State(api_state), Query(params)).await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn test_set_home_path_invalid() {
        let (state, pm, em) = create_test_components().await;
        let api_state = Arc::new(ApiState {
            state,
            peer_manager: pm,
            entry_manager: em,
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
    async fn test_sse_broadcast_channel() {
        let (state, _pm, _em) = create_test_components().await;
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
}
