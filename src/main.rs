mod application;
mod config;
mod domain;
mod entry;
mod infra;
mod peer;
mod proto;
mod services;
mod utils;
mod watcher;

use crate::{
    services::{
        file::FileService, handshake::HandshakeService, presence::PresenceService,
        sync::SyncService,
    },
    watcher::FileWatcher,
};
use std::sync::Arc;
use tokio::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    let state = Arc::new(config::init());

    let file_service = Arc::new(FileService::new(state.clone()));
    let handshake_service = Arc::new(HandshakeService::new(state.clone(), file_service.clone()));
    let presence_service = PresenceService::new(state.clone(), handshake_service.clone()).await;
    let sync_service = SyncService::new(state.clone(), file_service.clone(), handshake_service);

    let (mut watcher, mut watcher_sender) = FileWatcher::new(state, file_service);

    tokio::try_join!(
        presence_service.watch_peers(),
        presence_service.send_presence(),
        presence_service.recv_presence(),
        watcher.watch(),
        watcher_sender.send_changes(),
        sync_service.recv_data(),
    )?;
    Ok(())
}
