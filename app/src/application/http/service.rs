use crate::{
    application::{EntryManager, PeerManager, persistence::interface::PersistenceInterface},
    domain::{AppState, Peer, RelativePath, SyncDirectory, TransportChannelData},
};
use shared::ServerEvent;
use std::{net::IpAddr, sync::Arc};
use tokio::{io, sync::mpsc::Sender};
use tracing::info;
use uuid::Uuid;

pub struct HttpService<P: PersistenceInterface> {
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
    sender_tx: Sender<TransportChannelData>,
}

impl<P: PersistenceInterface> HttpService<P> {
    pub fn new(
        state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        entry_manager: Arc<EntryManager<P>>,
        sender_tx: Sender<TransportChannelData>,
    ) -> Arc<Self> {
        Arc::new(Self {
            state,
            sender_tx,
            peer_manager,
            entry_manager,
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

        self.entry_manager.add_sync_dir(name.clone()).await?;
        self.state.update_config_file().await?;
        self.resync().await?;

        info!("📂 Sync dir added: {name:?}");
        Ok(true)
    }

    pub async fn remove_sync_dir(&self, name: RelativePath) -> io::Result<()> {
        if self.entry_manager.remove_sync_dir(&name).await {
            self.state.update_config_file().await?;
            self.resync().await?;
        }
        Ok(())
    }

    async fn resync(&self) -> io::Result<()> {
        for peer in self.peer_manager.list().await {
            self.sender_tx
                .send(TransportChannelData::HandshakeSyn(peer.addr))
                .await
                .map_err(|e| io::Error::other(e.to_string()))?;
        }
        Ok(())
    }

    pub async fn list_peers(&self) -> Vec<Peer> {
        self.peer_manager.list().await
    }

    pub async fn get_local_info(&self) -> (IpAddr, Uuid, String) {
        (
            self.state.local_ip().await,
            self.state.local_id,
            self.state.hostname.clone(),
        )
    }

    pub async fn next_sse_event(&self) -> Option<ServerEvent> {
        self.state.sse_chan.rx.lock().await.recv().await
    }
}
