use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone)]
pub enum ServerEvent {
    PeerConnected(Uuid),
    PeerDisconnected(Uuid),
    SyncDirectoryUpdate(SyncDirectoryUpdateData),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SyncDirectoryUpdateData {
    pub kind: SyncDirectoryUpdateKind,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum SyncDirectoryUpdateKind {
    Ok,
    Syncing,
}
