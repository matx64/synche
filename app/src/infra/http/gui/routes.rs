use std::sync::Arc;
use crate::{application::{persistence::interface::PersistenceInterface, HttpService}, infra::http::gui::engine};
use axum::{Router, extract::State, http::StatusCode, response::Html, routing::get};
use minijinja::{Environment, context};
use tower_http::services::ServeDir;

struct ControllerState<P: PersistenceInterface> {
    engine: Environment<'static>,
    http_service: Arc<HttpService<P>>,
}

pub fn router<P: PersistenceInterface>(http_service: Arc<HttpService<P>>) -> Router {
    let state = Arc::new(ControllerState {
        http_service,
        engine: engine::init(),
    });

    Router::new()
        .route("/", get(index))
        .with_state(state)
        .nest_service("/static", ServeDir::new("./gui/static"))
}

async fn index<P: PersistenceInterface>(State(state): State<Arc<ControllerState<P>>>,) -> Result<Html<String>, StatusCode> {
    let dirs = state.http_service.list_dirs().await;

    let tmpl = state.engine
        .get_template("index")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rendered = tmpl
        .render(context! {
            dirs => dirs
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Html(rendered))
}
