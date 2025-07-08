mod config;
mod file;
mod handshake;
mod models;
mod presence;
mod sync;
mod utils;
mod watcher;

use crate::{
    file::FileService, handshake::HandshakeService, models::file::SynchedFile,
    presence::PresenceService, sync::SyncService, watcher::FileWatcher,
};
use std::sync::Arc;
use tokio::{io, sync::mpsc};

#[tokio::main]
async fn main() -> io::Result<()> {
    let state = Arc::new(config::init());

    let (sync_tx, sync_rx) = mpsc::channel::<SynchedFile>(100);

    let mut file_watcher = FileWatcher::new(state.clone(), sync_tx);

    let file_service = Arc::new(FileService::new(state.clone()));
    let handshake_service = Arc::new(HandshakeService::new(state.clone(), file_service.clone()));
    let presence_service = PresenceService::new(state.clone(), handshake_service.clone()).await;
    let sync_service = SyncService::new(state, file_service, handshake_service);

    tokio::try_join!(
        file_watcher.watch(),
        presence_service.watch_devices(),
        presence_service.send_presence(),
        presence_service.recv_presence(),
        sync_service.recv_data(),
        sync_service.sync_files(sync_rx),
    )?;
    Ok(())
}
