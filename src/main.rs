mod config;
mod file;
mod presence;
mod watcher;

use crate::{
    config::SynchedFile,
    file::{recv_files, sync_files},
    presence::PresenceHandler,
    watcher::FileWatcher,
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::SystemTime,
};
use tokio::{io, sync::mpsc};

#[derive(Debug, Clone)]
struct Device {
    addr: SocketAddr,
    synched_files: HashMap<String, SynchedFile>,
    last_seen: SystemTime,
}

impl Device {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            synched_files: HashMap::new(),
            last_seen: SystemTime::now(),
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let cfg = config::init();

    let (sync_tx, sync_rx) = mpsc::channel::<String>(100);
    let devices = Arc::new(RwLock::new(HashMap::<SocketAddr, Device>::new()));

    let presence_handler = PresenceHandler::new(devices.clone()).await;
    let mut watcher = FileWatcher::new(sync_tx, cfg.synched_files.clone());

    tokio::try_join!(
        presence_handler.state(),
        presence_handler.send_presence(),
        presence_handler.recv_presence(),
        watcher.watch_files(),
        sync_files(sync_rx, devices.clone()),
        recv_files(),
    )?;
    Ok(())
}
