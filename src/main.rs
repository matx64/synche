mod config;
mod file;
mod handshake;
mod presence;
mod sync;
mod watcher;

use crate::{
    config::SynchedFile,
    handshake::HandshakeHandler,
    presence::PresenceHandler,
    sync::{recv_data, sync_files},
    watcher::FileWatcher,
};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, RwLock},
    time::SystemTime,
};
use tokio::{io, sync::mpsc};

#[derive(Debug, Clone)]
struct Device {
    addr: SocketAddr,
    synched_files: HashMap<String, SynchedFile>,
    last_seen: SystemTime,
    handshake_hash: Option<u64>,
}

impl Device {
    pub fn new(addr: SocketAddr, synched_files: Option<HashMap<String, SynchedFile>>) -> Self {
        Self {
            addr,
            synched_files: synched_files.unwrap_or_default(),
            last_seen: SystemTime::now(),
            handshake_hash: None,
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let cfg = config::init();

    let (sync_tx, sync_rx) = mpsc::channel::<SynchedFile>(100);
    let devices = Arc::new(RwLock::new(HashMap::<IpAddr, Device>::new()));

    let handshake_handler = Arc::new(HandshakeHandler::new(
        devices.clone(),
        cfg.synched_files.clone(),
    ));
    let presence_handler = PresenceHandler::new(handshake_handler.clone(), devices.clone()).await;
    let mut file_watcher = FileWatcher::new(sync_tx, cfg.synched_files.clone());

    tokio::try_join!(
        presence_handler.watch_devices(),
        presence_handler.send_presence(),
        presence_handler.recv_presence(),
        file_watcher.watch(),
        sync_files(sync_rx, devices.clone()),
        recv_data(handshake_handler, cfg.synched_files, devices),
    )?;
    Ok(())
}
