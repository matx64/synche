use crate::domain::EntryInfo;
use std::net::IpAddr;
use tokio::sync::{
    Mutex,
    mpsc::{self, Receiver, Sender},
};

pub enum TransportSendData {
    Handshake((IpAddr, HandshakeKind)),
    Metadata(EntryInfo),
    Request((IpAddr, EntryInfo)),
    Transfer((IpAddr, EntryInfo)),
}

pub enum HandshakeKind {
    Request,
    Response,
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
