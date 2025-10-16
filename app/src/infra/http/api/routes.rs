use crate::{
    application::{HttpService, persistence::interface::PersistenceInterface},
    infra::http::api::controllers,
};
use axum::Router;
use std::sync::Arc;

pub fn router<P: PersistenceInterface>(http_service: Arc<HttpService<P>>) -> Router {
    let routes = Router::new()
        .merge(controllers::sse::router())
        .merge(controllers::sync::router(http_service));

    Router::new().nest("/api", routes)
}
