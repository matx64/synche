use crate::infra::http::api::controllers;
use axum::Router;

pub fn router() -> Router {
    let routes = Router::new()
        .merge(controllers::sse::router())
        .merge(controllers::sync::router());

    Router::new().nest("/api", routes)
}
