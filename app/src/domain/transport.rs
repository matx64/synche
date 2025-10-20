use crate::domain::{EntryInfo, RelativePath, SyncDirectory};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::IpAddr};

pub enum TransportChannelData {
    HandshakeSyn(IpAddr),
    HandshakeAck(IpAddr),
    Metadata(EntryInfo),
    Request((IpAddr, EntryInfo)),
    Transfer((IpAddr, EntryInfo)),
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
    pub sync_dirs: Vec<SyncDirectory>,
    pub entries: HashMap<RelativePath, EntryInfo>,
}
