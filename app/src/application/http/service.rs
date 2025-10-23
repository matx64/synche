use crate::{
    application::{
        EntryManager, PeerManager, persistence::interface::PersistenceInterface,
        watcher::interface::FileWatcherSyncDirectoryUpdate,
    },
    domain::{AppState, RelativePath, SyncDirectory, TransportChannelData},
};
use std::{net::IpAddr, sync::Arc};
use tokio::{io, sync::mpsc::Sender};
use tracing::info;
use uuid::Uuid;

pub struct HttpService<P: PersistenceInterface> {
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
    sender_tx: Sender<TransportChannelData>,
    dirs_updates_tx: Sender<FileWatcherSyncDirectoryUpdate>,
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

    pub async fn add_sync_dir(&self, name: RelativePath) -> io::Result<bool> {
        if self.entry_manager.get_sync_dir(&name).await.is_some() {
            return Ok(false);
        }

        let path = self.entry_manager.add_sync_dir(name.clone()).await?;

        self.state
            .update_config_file(self.entry_manager.list_dirs().await)
            .await?;

        self.update_watcher_and_resync(FileWatcherSyncDirectoryUpdate::Added(path))
            .await?;

        info!("📂 Sync dir added: {name:?}");
        Ok(true)
    }

    pub async fn remove_sync_dir(&self, name: RelativePath) -> io::Result<()> {
        let Some(_dir) = self.entry_manager.get_sync_dir(&name).await else {
            return Ok(());
        };

        Ok(())
    }

    async fn update_watcher_and_resync(
        &self,
        event: FileWatcherSyncDirectoryUpdate,
    ) -> io::Result<()> {
        self.dirs_updates_tx
            .send(event)
            .await
            .map_err(|e| io::Error::other(e.to_string()))?;

        for (_, addr) in self.peer_manager.list() {
            self.sender_tx
                .send(TransportChannelData::HandshakeSyn(addr))
                .await
                .map_err(|e| io::Error::other(e.to_string()))?;
        }
        Ok(())
    }

    pub async fn get_local_info(&self) -> (IpAddr, Uuid, String) {
        (
            self.state.local_ip().await,
            self.state.local_id,
            self.state.hostname.clone(),
        )
    }

    pub fn _send_event() {}
}
