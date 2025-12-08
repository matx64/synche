use crate::domain::RelativePath;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ServerEvent {
    PeerConnected {
        id: Uuid,
        addr: IpAddr,
        hostname: String,
    },
    PeerDisconnected(Uuid),
    SyncDirectoryAdded(RelativePath),
    SyncDirectoryRemoved(RelativePath),
    ServerRestart,
}
