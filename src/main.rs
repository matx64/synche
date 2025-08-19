mod application;
mod config;
mod domain;
mod infra;
mod proto;
mod utils;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let config = config::init();
    let mut synchronizer = application::Synchronizer::new_default(config).await;

    synchronizer.run().await
}
