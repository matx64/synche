mod application;
mod config;
mod domain;
mod infra;
mod proto;
mod utils;

use crate::application::Synchronizer;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let state = config::init();

    let mut synchronizer = Synchronizer::new_default(state).await;
    synchronizer.run().await
}
