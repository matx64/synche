use crate::{
    application::{HttpService, persistence::interface::PersistenceInterface},
    infra::http::{api, gui},
};
use axum::Router;
use std::sync::Arc;
use tokio::net::TcpListener;

pub async fn run<P: PersistenceInterface>(
    http_service: Arc<HttpService<P>>,
) -> tokio::io::Result<()> {
    let service = Router::new()
        .merge(gui::router(http_service.clone()))
        .merge(api::router(http_service));

    let addr = "127.0.0.1:8888";
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("🌐 Web GUI: http://{addr}");
    axum::serve(listener, service).await
}
