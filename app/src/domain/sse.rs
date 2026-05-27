use crate::domain::RelativePath;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use uuid::Uuid;

/// Events broadcast to the GUI over Server-Sent Events to keep the
/// frontend's view of peers and sync directories in sync with the
/// running application.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ServerEvent {
    /// A new peer was discovered or an existing one reconnected.
    PeerConnected {
        id: Uuid,
        addr: IpAddr,
        hostname: String,
        /// Regenerated on every process start; a change signals a peer restart.
        instance_id: Uuid,
        /// Seconds since UNIX epoch — when the peer last announced itself.
        last_seen: u64,
        /// Names of the sync directories this peer is sharing.
        sync_dirs: Vec<RelativePath>,
    },
    /// A peer was evicted (timed out, or explicitly disconnected).
    PeerDisconnected(Uuid),
    /// A sync directory was added to the local config.
    SyncDirectoryAdded(RelativePath),
    /// A sync directory was removed from the local config.
    SyncDirectoryRemoved(RelativePath),
    /// The server is restarting (e.g. after a `home_path` change) — the
    /// GUI should reconnect.
    ServerRestart,
}
