use crate::domain::{EntryInfo, RelativePath, SyncDirectory};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::IpAddr};
use uuid::Uuid;

pub struct TransportEvent {
    pub payload: TransportData,
    pub metadata: TransportMetadata,
}

pub struct TransportMetadata {
    pub source_id: Uuid,
    pub source_ip: IpAddr,
}

pub enum TransportData {
    HandshakeSyn(HandshakeData),
    HandshakeAck(HandshakeData),
    Metadata(EntryInfo),
    Request(EntryInfo),
    Transfer(EntryInfo),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HandshakeData {
    pub hostname: String,
    pub instance_id: Uuid,
    pub sync_dirs: Vec<SyncDirectory>,
    pub entries: HashMap<RelativePath, EntryInfo>,
}

pub enum TransportChannelData {
    HandshakeSyn(IpAddr),
    _HandshakeAck(IpAddr),
    Metadata(EntryInfo),
    Request((IpAddr, EntryInfo)),
    Transfer((IpAddr, EntryInfo)),
}
