mod application;
mod config;
mod domain;
mod infra;
mod proto;
mod services;
mod utils;
mod watcher;

use crate::application::Synchronizer;
use tokio::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    let state = config::init();

    let synchronizer = Synchronizer::new_default(state).await;

    // tokio::try_join!(
    //     presence_service.watch_peers(),
    //     presence_service.send_presence(),
    //     presence_service.recv_presence(),
    //     watcher.watch(),
    //     watcher_sender.send_changes(),
    //     sync_service.recv_data(),
    // )?;
    Ok(())
}
