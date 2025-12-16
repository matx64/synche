use crate::application::{HttpService, persistence::interface::PersistenceInterface};
use async_stream::try_stream;
use axum::{
    Router,
    extract::State,
    response::{Sse, sse::Event},
    routing::get,
};
use futures_util::stream::Stream;
use std::{convert::Infallible, sync::Arc};
use tokio::sync::broadcast;
use tracing::{error, info, warn};

struct ControllerState<P: PersistenceInterface> {
    http_service: Arc<HttpService<P>>,
}

pub fn router<P: PersistenceInterface>(http_service: Arc<HttpService<P>>) -> Router {
    let state = Arc::new(ControllerState { http_service });

    Router::new()
        .route("/events", get(sse_handler))
        .with_state(state)
}

async fn sse_handler<P: PersistenceInterface>(
    State(state): State<Arc<ControllerState<P>>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.http_service.subscribe_to_events();

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
