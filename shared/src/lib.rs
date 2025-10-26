use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone)]
pub enum ServerEvent {
    PeerConnected {
        id: Uuid,
        addr: IpAddr,
        hostname: String,
    },
    PeerDisconnected(Uuid),
    SyncDirectoryUpdate {
        name: String,
        kind: SyncDirectoryUpdateKind,
    },
}

#[derive(Serialize, Deserialize, Clone)]
pub enum SyncDirectoryUpdateKind {
    Ok,
    Syncing,
}
