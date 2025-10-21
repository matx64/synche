use crate::{
    application::{
        EntryManager, PeerManager, persistence::interface::PersistenceInterface,
        watcher::interface::FileWatcherSyncDirectoryUpdate,
    },
    domain::{AppState, SyncDirectory, TransportChannelData},
};
use std::{net::IpAddr, sync::Arc};
use tokio::{io, sync::mpsc::Sender};
use tracing::{error, info};
use uuid::Uuid;

pub struct HttpService<P: PersistenceInterface> {
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
    dirs_updates_tx: Sender<FileWatcherSyncDirectoryUpdate>,
    sender_tx: Sender<TransportChannelData>,
}

impl<P: PersistenceInterface> HttpService<P> {
    pub fn new(
        state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        entry_manager: Arc<EntryManager<P>>,
        sender_tx: Sender<TransportChannelData>,
        dirs_updates_tx: Sender<FileWatcherSyncDirectoryUpdate>,
    ) -> Arc<Self> {
        Arc::new(Self {
            state,
            sender_tx,
            peer_manager,
            entry_manager,
            dirs_updates_tx,
        })
    }

    pub async fn list_dirs(&self) -> Vec<SyncDirectory> {
        self.entry_manager
            .list_dirs()
            .await
            .values()
            .cloned()
            .collect()
    }

    pub async fn add_sync_dir(&self, name: &str) -> io::Result<bool> {
        if self.entry_manager.is_sync_dir(name).await {
            return Ok(false);
        }

        let path = self.entry_manager.add_sync_dir(name).await?;

        self.update_watcher_and_resync(FileWatcherSyncDirectoryUpdate::Added(path))
            .await;

        info!("📂 Sync dir added: {name}");
        Ok(true)
    }

    pub fn _remove_folder() {}

    async fn update_watcher_and_resync(&self, event: FileWatcherSyncDirectoryUpdate) {
        if let Err(err) = self.dirs_updates_tx.send(event).await {
            error!("Dir update send error: {err}");
        }

        for (_, addr) in self.peer_manager.list() {
            if let Err(err) = self
                .sender_tx
                .send(TransportChannelData::HandshakeSyn(addr))
                .await
            {
                error!("Handshake send error: {err}");
            }
        }
    }

    pub async fn get_local_info(&self) -> (IpAddr, Uuid) {
        (self.state.local_ip().await, self.state.local_id)
    }

    pub fn _send_event() {}
}
