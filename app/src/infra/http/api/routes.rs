use crate::infra::http::api::controllers;
use axum::Router;

pub fn router() -> Router {
    let routes = Router::new()
        .merge(controllers::ws::router())
        .merge(controllers::sync::router());

    Router::new().nest("/api", routes)
}
