mod application;
mod config;
mod domain;
mod infra;
mod proto;
mod utils;

use crate::application::Synchronizer;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let config = config::init();

    let mut synchronizer = Synchronizer::new_default(config).await;
    synchronizer.run().await
}
