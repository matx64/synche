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
    /// This device started receiving an entry from a peer.
    EntrySyncStarted {
        /// Top-level sync directory the entry belongs to.
        dir: RelativePath,
        /// Full path inside the home directory.
        relative_path: RelativePath,
        /// Peer the entry is being received from.
        peer: Uuid,
    },
    /// An entry finished syncing and was applied to disk.
    EntrySyncCompleted {
        dir: RelativePath,
        relative_path: RelativePath,
        peer: Uuid,
    },
    /// An entry sync was aborted mid-transfer.
    EntrySyncFailed {
        dir: RelativePath,
        relative_path: RelativePath,
        peer: Uuid,
        /// Human-readable failure reason (hash mismatch, oversized, I/O error).
        reason: String,
    },
    /// The server is restarting (e.g. after a `home_path` change) — the
    /// GUI should reconnect.
    ServerRestart,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_sync_variants_serialize_as_tagged_json() {
        let peer = Uuid::nil();
        let started = ServerEvent::EntrySyncStarted {
            dir: "sync".into(),
            relative_path: "sync/foo.txt".into(),
            peer,
        };
        let started_json = serde_json::to_value(&started).unwrap();
        assert_eq!(started_json["EntrySyncStarted"]["dir"], "sync");
        assert_eq!(
            started_json["EntrySyncStarted"]["relative_path"],
            "sync/foo.txt"
        );
        assert_eq!(started_json["EntrySyncStarted"]["peer"], peer.to_string());

        let failed = ServerEvent::EntrySyncFailed {
            dir: "sync".into(),
            relative_path: "sync/foo.txt".into(),
            peer,
            reason: "Hash mismatch".into(),
        };
        let failed_json = serde_json::to_value(&failed).unwrap();
        assert_eq!(failed_json["EntrySyncFailed"]["reason"], "Hash mismatch");
    }
}
