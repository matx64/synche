use crate::{
    domain::EntryInfo,
    proto::transport::{PeerSyncData, SyncHandshakeKind, SyncKind},
};
use std::net::IpAddr;
use tokio::{
    io::{self, AsyncRead, AsyncWrite},
    sync::{
        Mutex,
        mpsc::{Receiver, Sender},
    },
};
use uuid::Uuid;

pub trait TransportInterface {
    type Stream: TransportStream;

    async fn recv(&self) -> io::Result<TransportData<Self::Stream>>;

    async fn send_handshake(
        &self,
        addr: IpAddr,
        kind: SyncKind,
        data: PeerSyncData,
    ) -> io::Result<()>;
    async fn read_handshake(&self, stream: &mut Self::Stream) -> io::Result<PeerSyncData>;

    async fn send_metadata(&self, addr: IpAddr, file: &EntryInfo) -> io::Result<()>;
    async fn read_metadata(&self, stream: &mut Self::Stream) -> io::Result<EntryInfo>;

    async fn send_request(&self, addr: IpAddr, file: &EntryInfo) -> io::Result<()>;
    async fn read_request(&self, stream: &mut Self::Stream) -> io::Result<EntryInfo>;

    async fn send_file(&self, addr: IpAddr, file: &EntryInfo, contents: &[u8]) -> io::Result<()>;
    async fn read_file(&self, stream: &mut Self::Stream) -> io::Result<(EntryInfo, Vec<u8>)>;
}

pub trait TransportStream: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

pub struct TransportData<T: TransportStream> {
    pub src_id: Uuid,
    pub src_ip: IpAddr,
    pub kind: SyncKind,
    pub stream: T,
}

pub struct TransportSenders {
    pub watch_tx: Sender<EntryInfo>,
    pub handshake_tx: Sender<(IpAddr, SyncHandshakeKind)>,
    pub request_tx: Sender<(IpAddr, EntryInfo)>,
    pub transfer_tx: Sender<(IpAddr, EntryInfo)>,
}

pub struct TransportReceivers {
    pub watch_rx: Mutex<Receiver<EntryInfo>>,
    pub handshake_rx: Mutex<Receiver<(IpAddr, SyncHandshakeKind)>>,
    pub request_rx: Mutex<Receiver<(IpAddr, EntryInfo)>>,
    pub transfer_rx: Mutex<Receiver<(IpAddr, EntryInfo)>>,
}
