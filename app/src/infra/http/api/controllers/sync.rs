use crate::{
    application::{HttpService, persistence::interface::PersistenceInterface},
    domain::RelativePath,
};
use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    routing::post,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::error;

struct ControllerState<P: PersistenceInterface> {
    http_service: Arc<HttpService<P>>,
}

pub fn router<P: PersistenceInterface>(http_service: Arc<HttpService<P>>) -> Router {
    let state = Arc::new(ControllerState { http_service });

    Router::new()
        .route("/add-sync-dir", post(add_sync_dir))
        .route("/remove-sync-dir", post(remove_sync_dir))
        .with_state(state)
}

#[derive(Deserialize)]
struct ModifySyncDirParams {
    pub name: RelativePath,
}

async fn add_sync_dir<P: PersistenceInterface>(
    State(state): State<Arc<ControllerState<P>>>,
    Query(params): Query<ModifySyncDirParams>,
) -> StatusCode {
    let name = params.name.trim().into();

    match state.http_service.add_sync_dir(name).await {
        Ok(true) => StatusCode::CREATED,
        Ok(false) => StatusCode::CONFLICT,
        Err(err) => {
            error!("Add sync dir error: {err}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn remove_sync_dir<P: PersistenceInterface>(
    State(state): State<Arc<ControllerState<P>>>,
    Query(params): Query<ModifySyncDirParams>,
) -> StatusCode {
    let name = params.name.trim().into();

    match state.http_service.remove_sync_dir(name).await {
        Ok(_) => StatusCode::OK,
        Err(err) => {
            error!("Remove sync dir error: {err}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
