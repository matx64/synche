use crate::infra;

pub async fn start_server() -> tokio::io::Result<()> {
    let engine = infra::gui::engine::init();
    let service = axum::Router::new()
        .merge(infra::api::router())
        .merge(infra::gui::router(engine));

    let addr = "127.0.0.1:8888";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("🌐 Web GUI: http://{addr}");
    axum::serve(listener, service).await
}
