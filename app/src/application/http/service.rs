use crate::{
    application::{EntryManager, PeerManager, persistence::interface::PersistenceInterface},
    domain::{AppState, Peer, RelativePath, ServerEvent, SyncDirectory},
};
use std::{net::IpAddr, sync::Arc};
use tokio::io;
use tracing::info;
use uuid::Uuid;

pub struct HttpService<P: PersistenceInterface> {
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
}

impl<P: PersistenceInterface> HttpService<P> {
    pub fn new(
        state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        entry_manager: Arc<EntryManager<P>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            state,
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
        if self.state.add_dir_to_config(&name).await? {
            info!("Sync dir add requested: {name:?}");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn remove_sync_dir(&self, name: RelativePath) -> io::Result<()> {
        self.state.remove_dir_from_config(&name).await
    }

    pub async fn list_peers(&self) -> Vec<Peer> {
        self.peer_manager.list().await
    }

    pub async fn get_local_info(&self) -> (IpAddr, Uuid, String) {
        (
            self.state.local_ip().await,
            self.state.local_id(),
            self.state.hostname().clone(),
        )
    }

    pub fn get_home_path(&self) -> String {
        self.state.home_path().display().to_string()
    }

    pub async fn set_home_path(&self, new_path: String) -> io::Result<()> {
        self.state.set_home_path_in_config(new_path).await
    }

    pub async fn next_sse_event(&self) -> Option<ServerEvent> {
        self.state.sse_chan.rx.lock().await.recv().await
    }
}
