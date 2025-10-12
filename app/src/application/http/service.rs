use tokio::{io, sync::mpsc::Sender};
use tracing::error;

use crate::{
    application::{
        EntryManager, PeerManager,
        persistence::interface::PersistenceInterface,
        watcher::interface::{FileWatcherSyncDirectoryUpdate, FileWatcherSyncDirectoryUpdateKind},
    },
    domain::CanonicalPath,
    proto::transport::SyncHandshakeKind,
};
use core::error;
use std::{net::IpAddr, sync::Arc};

pub struct HttpService<D: PersistenceInterface> {
    entry_manager: Arc<EntryManager<D>>,
    peer_manager: Arc<PeerManager>,
    dirs_updates_tx: Sender<FileWatcherSyncDirectoryUpdate>,
    handshake_tx: Sender<(IpAddr, SyncHandshakeKind)>,
}

impl<D: PersistenceInterface> HttpService<D> {
    pub fn new(
        entry_manager: Arc<EntryManager<D>>,
        peer_manager: Arc<PeerManager>,
        dirs_updates_tx: Sender<FileWatcherSyncDirectoryUpdate>,
        handshake_tx: Sender<(IpAddr, SyncHandshakeKind)>,
    ) -> Self {
        Self {
            entry_manager,
            peer_manager,
            dirs_updates_tx,
            handshake_tx,
        }
    }

    pub async fn add_sync_dir(&self, name: &str) -> io::Result<()> {
        let path = self.entry_manager.add_sync_dir(name).await?;

        self.update_watcher_and_resync(path, FileWatcherSyncDirectoryUpdateKind::Added)
            .await;

        Ok(())
    }

    pub fn remove_folder() {}

    async fn update_watcher_and_resync(
        &self,
        path: CanonicalPath,
        kind: FileWatcherSyncDirectoryUpdateKind,
    ) {
        if let Err(err) = self
            .dirs_updates_tx
            .send(FileWatcherSyncDirectoryUpdate { path, kind })
            .await
        {
            error!("Dir update send error: {err}");
        }

        for (_, addr) in self.peer_manager.list() {
            if let Err(err) = self
                .handshake_tx
                .send((addr, SyncHandshakeKind::Request))
                .await
            {
                error!("Handshake send error: {err}");
            }
        }
    }

    pub fn send_event() {}
}
