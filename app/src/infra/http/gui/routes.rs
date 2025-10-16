use crate::infra::http::gui::engine;
use axum::{Router, extract::State, http::StatusCode, response::Html, routing::get};
use minijinja::{Environment, context};
use tower_http::services::ServeDir;

pub fn router() -> Router {
    let engine = engine::init();

    Router::new()
        .route("/", get(index))
        .with_state(engine)
        .nest_service("/static", ServeDir::new("./gui/static"))
}

async fn index(State(engine): State<Environment<'static>>) -> Result<Html<String>, StatusCode> {
    let tmpl = engine
        .get_template("index")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rendered = tmpl
        .render(context! {})
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Html(rendered))
}
