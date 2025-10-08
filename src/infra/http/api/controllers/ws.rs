use axum::{
    Router,
    extract::{State, WebSocketUpgrade, ws::WebSocket},
    response::IntoResponse,
    routing::any,
};
use std::sync::Arc;

struct ControllerState {}

pub fn router() -> Router {
    let state = Arc::new(ControllerState {});

    Router::new().route("/ws", any(connect)).with_state(state)
}

async fn connect(
    State(state): State<Arc<ControllerState>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handler(socket, state))
}

async fn handler(socket: WebSocket, state: Arc<ControllerState>) {}
