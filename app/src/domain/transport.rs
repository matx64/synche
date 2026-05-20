use crate::domain::{EntryInfo, RelativePath, SyncDirectory};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::IpAddr};
use uuid::Uuid;

/// An inbound transport message, paired with the metadata that
/// identifies which peer sent it.
pub struct TransportEvent {
    pub payload: TransportData,
    pub metadata: TransportMetadata,
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
