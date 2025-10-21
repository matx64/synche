use crate::{
    application::{HttpService, persistence::interface::PersistenceInterface},
    infra::http::gui::engine,
};
use axum::{Router, extract::State, http::StatusCode, response::Html, routing::get};
use minijinja::{Environment, context};
use std::sync::Arc;
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

async fn index<P: PersistenceInterface>(
    State(state): State<Arc<ControllerState<P>>>,
) -> Result<Html<String>, StatusCode> {
    let (local_ip, local_id, hostname) = state.http_service.get_local_info().await;
    let dirs = state.http_service.list_dirs().await;

    let tmpl = state
        .engine
        .get_template("index")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rendered = tmpl
        .render(context! {
            hostname => hostname,
            local_ip => local_ip,
            local_id => local_id,
            dirs => dirs
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Html(rendered))
}
