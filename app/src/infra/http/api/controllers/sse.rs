use async_stream::try_stream;
use axum::{
    Router,
    extract::State,
    response::{Sse, sse::Event},
    routing::get,
};
use futures_util::stream::Stream;
use std::{convert::Infallible, sync::Arc};
use tracing::error;

use crate::application::{HttpService, persistence::interface::PersistenceInterface};

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
    Sse::new(try_stream! {
        while let Some(event) = state.http_service.next_sse_event().await {
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
