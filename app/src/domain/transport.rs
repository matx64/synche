use crate::domain::{EntryInfo, RelativePath, SyncDirectory};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::IpAddr};
use tokio::sync::{
    Mutex,
    mpsc::{self, Receiver, Sender},
};

pub enum TransportChannelData {
    HandshakeSyn(IpAddr),
    HandshakeAck(IpAddr),
    Metadata(EntryInfo),
    Request((IpAddr, EntryInfo)),
    Transfer((IpAddr, EntryInfo)),
}

pub enum TransportDataV2 {
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

pub struct TransportChannel<K> {
    pub tx: Sender<K>,
    pub rx: Mutex<Receiver<K>>,
}

impl<K> TransportChannel<K> {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(16);
        Self {
            tx,
            rx: Mutex::new(rx),
        }
    }
}
