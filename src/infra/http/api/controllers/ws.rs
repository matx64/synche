use axum::{
    Router,
    extract::{State, WebSocketUpgrade, ws::WebSocket},
    response::IntoResponse,
    routing::any,
};
use std::sync::Arc;

struct WsState {}

pub fn router() -> Router {
    let state = Arc::new(WsState {});

    Router::new().route("/ws", any(connect)).with_state(state)
}

async fn connect(ws: WebSocketUpgrade, State(state): State<Arc<WsState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handler(socket, state))
}

async fn handler(socket: WebSocket, state: Arc<WsState>) {}
