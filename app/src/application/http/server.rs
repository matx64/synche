use crate::infra::http::{api, gui};
use axum::Router;
use tokio::net::TcpListener;

pub async fn run() -> tokio::io::Result<()> {
    let engine = gui::engine::init();

    let service = Router::new()
        .merge(gui::router(engine))
        .merge(api::router());

    let addr = "127.0.0.1:8888";
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("🌐 Web GUI: http://{addr}");
    axum::serve(listener, service).await
}
