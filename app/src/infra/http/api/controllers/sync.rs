use crate::application::{HttpService, persistence::interface::PersistenceInterface};
use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    routing::post,
};
use std::sync::Arc;

struct ControllerState<P: PersistenceInterface> {
    http_service: Arc<HttpService<P>>,
}

pub fn router<P: PersistenceInterface>(http_service: Arc<HttpService<P>>) -> Router {
    let state = Arc::new(ControllerState { http_service });

    Router::new()
        .route("/add-folder", post(add_folder))
        .route("/remove-folder", post(remove_folder))
        .with_state(state)
}

async fn add_folder<P: PersistenceInterface>(
    State(state): State<Arc<ControllerState<P>>>,
    Query(name): Query<String>,
) -> StatusCode {
    let name = name.trim();
    StatusCode::OK
}

async fn remove_folder<P: PersistenceInterface>(
    State(state): State<Arc<ControllerState<P>>>,
    Query(name): Query<String>,
) -> StatusCode {
    let name = name.trim();
    StatusCode::OK
}
