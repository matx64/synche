use async_stream::try_stream;
use axum::{
    Router,
    extract::State,
    response::{Sse, sse::Event},
    routing::get,
};
use futures_util::stream::Stream;
use shared::ServerEvent;
use std::{convert::Infallible, sync::Arc};
use tokio::sync::{
    Mutex,
    mpsc::{self, Receiver, Sender},
};
use tracing::error;

struct ControllerState {
    pub _tx: Sender<ServerEvent>,
    pub rx: Mutex<Receiver<ServerEvent>>,
}

pub fn router() -> Router {
    let (tx, rx) = mpsc::channel::<ServerEvent>(16);

    let state = Arc::new(ControllerState {
        _tx: tx,
        rx: Mutex::new(rx),
    });

    Router::new()
        .route("/events", get(sse_handler))
        .with_state(state)
}

async fn sse_handler(
    State(state): State<Arc<ControllerState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    Sse::new(try_stream! {
        while let Some(event) = state.rx.lock().await.recv().await {
            match serde_json::to_string(&event) {
                Ok(data) => {
                    yield Event::default().data(data);
                }
                Err(err) => {
                    error!("Error serializing event: {err}");
                }
            }
        }
    })
}
