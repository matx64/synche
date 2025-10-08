use axum::{
    Router,
    extract::{Query, State},
    http::StatusCode,
    routing::post,
};
use std::sync::Arc;

struct ControllerState {}

pub fn router() -> Router {
    let state = Arc::new(ControllerState {});

    Router::new()
        .route("/add-folder", post(add_folder))
        .route("/remove-folder", post(remove_folder))
        .with_state(state)
}

async fn add_folder(
    State(state): State<Arc<ControllerState>>,
    Query(name): Query<String>,
) -> StatusCode {
    let name = name.trim();
    StatusCode::OK
}

async fn remove_folder(
    State(state): State<Arc<ControllerState>>,
    Query(name): Query<String>,
) -> StatusCode {
    let name = name.trim();
    StatusCode::OK
}
