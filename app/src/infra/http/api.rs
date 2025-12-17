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
