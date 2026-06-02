use crate::domain::{EntryInfo, RelativePath, SyncDirectory};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::IpAddr, path::PathBuf};
use tracing::warn;
use uuid::Uuid;

/// An inbound transport message, paired with the metadata that
/// identifies which peer sent it.
///
/// For `Transfer` payloads, the bytes are streamed into a transport-
/// owned staging directory (see `StagedTransfer`) rather than written
/// to `home_path` directly, so the application layer can validate the
/// transfer before committing. `staging` is `None` for every other
/// payload kind, and for in-memory transports that do not stage to
/// disk.
pub struct TransportEvent {
    pub payload: TransportData,
    pub metadata: TransportMetadata,
    pub staging: Option<StagedTransfer>,
}

/// A bulk Transfer payload that has been streamed to a per-transfer
/// staging directory in the OS temp dir but not yet committed into
/// `home_path`.
///
/// Issue #33 B1: the TCP adapter deliberately stops short of renaming
/// staging → home so the application layer can run the four
/// pre-commit checks (sync-dir scope, outstanding-request, local
/// compare, and per-entry serialization) BEFORE the user's tree
/// changes. The application commits by calling `take_path` and
/// performing the rename; on any failure path it simply drops this
/// value and the RAII guard cleans up the staging directory.
pub struct StagedTransfer {
    staging_path: Option<PathBuf>,
    staging_root: PathBuf,
}

impl StagedTransfer {
    pub fn new(staging_path: PathBuf, staging_root: PathBuf) -> Self {
        Self {
            staging_path: Some(staging_path),
            staging_root,
        }
    }

    /// Returns the staging file path without consuming the guard.
    /// Intended for inspection (e.g. hash verification in tests).
    #[cfg(test)]
    pub fn path(&self) -> Option<&PathBuf> {
        self.staging_path.as_ref()
    }

    /// Consume the guard, returning the staging file path. The caller
    /// owns the file; this guard will no longer attempt cleanup.
    /// `take_root` should be called afterwards to clean up the per-
    /// transfer parent directory once the file has been moved out.
    pub fn take_path(&mut self) -> Option<PathBuf> {
        self.staging_path.take()
    }
}

impl Drop for StagedTransfer {
    fn drop(&mut self) {
        // Always remove the per-transfer root directory; if the caller
        // took the file path out before drop, the parent is empty and
        // this is a cheap rmdir. If they didn't, this cleans up the
        // staged bytes.
        let _ = std::fs::remove_dir_all(&self.staging_root);
        if self.staging_path.is_some() {
            warn!(
                root = ?self.staging_root,
                "dropping uncommitted staged transfer"
            );
        }
    }
}

/// Provenance of an inbound `TransportEvent` — who sent it and from
/// where — extracted from the connection rather than the payload.
pub struct TransportMetadata {
    pub source_id: Uuid,
    pub source_ip: IpAddr,
}

/// The four wire-protocol message kinds exchanged between peers.
///
/// - `HandshakeSyn` / `HandshakeAck`: two-step exchange of identity,
///   sync directories, and current entry metadata when peers discover
///   each other.
/// - `Metadata`: a unidirectional update advertising the sender's view
///   of a single entry (insert, modify, or removal tombstone).
/// - `Request`: asks the recipient to send the bytes for an entry the
///   sender wants to fetch.
/// - `Transfer`: streams the actual bytes for a requested entry.
pub enum TransportData {
    HandshakeSyn(HandshakeData),
    HandshakeAck(HandshakeData),
    Metadata(EntryInfo),
    Request(EntryInfo),
    Transfer(EntryInfo),
}

/// Payload for the handshake exchange — everything a peer needs to
/// reconcile its world view against the sender's on first contact.
#[derive(Serialize, Deserialize, Clone)]
pub struct HandshakeData {
    pub hostname: String,
    pub instance_id: Uuid,
    pub sync_dirs: Vec<SyncDirectory>,
    pub entries: HashMap<RelativePath, EntryInfo>,
}

/// Outbound transport intent enqueued by application services for the
/// transport sender to dispatch.
///
/// Mirrors `TransportData` but carries the target peer address since the
/// sender, unlike the receiver, must know where to send.
pub enum TransportChannelData {
    HandshakeSyn(IpAddr),
    _HandshakeAck(IpAddr),
    Metadata(EntryInfo),
    Request((IpAddr, EntryInfo)),
    Transfer((IpAddr, EntryInfo)),
}
