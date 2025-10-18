use crate::{
    domain::EntryInfo,
    proto::transport::{PeerHandshakeData, SyncHandshakeKind, SyncKind},
};
use std::net::IpAddr;
use tokio::{
    io::{self, AsyncRead, AsyncWrite},
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender},
    },
};
use uuid::Uuid;

pub trait TransportInterfaceV2 {
    async fn recv(&self) -> TransportResult<TransportDataV2>;

    async fn send_handshake(
        &self,
        target: IpAddr,
        kind: SyncKind,
        data: PeerHandshakeData,
    ) -> TransportResult<()>;

    async fn send_metadata(&self, target: IpAddr, entry: &EntryInfo) -> TransportResult<()>;

    async fn transfer_entry(&self, target: IpAddr, entry: &EntryInfo) -> TransportResult<()>;
}

pub enum TransportDataV2 {
    Handshake(PeerHandshakeData),
    Metadata(EntryInfo),
    Request(EntryInfo),
    Transfer(EntryInfo),
}

pub type TransportResult<T> = Result<T, TransportError>;

pub enum TransportError {
    Failure(String),
}

impl From<TransportError> for io::Error {
    fn from(err: TransportError) -> Self {
        match err {
            TransportError::Failure(str) => io::Error::other(str),
        }
    }
}

pub trait TransportInterface {
    type Stream: TransportStream;

    async fn recv(&self) -> io::Result<TransportData<Self::Stream>>;

    async fn send_handshake(
        &self,
        addr: IpAddr,
        kind: SyncKind,
        data: PeerHandshakeData,
    ) -> io::Result<()>;
    async fn read_handshake(&self, stream: &mut Self::Stream) -> io::Result<PeerHandshakeData>;

    async fn send_metadata(&self, addr: IpAddr, entry: &EntryInfo) -> io::Result<()>;
    async fn read_metadata(&self, stream: &mut Self::Stream) -> io::Result<EntryInfo>;

    async fn send_request(&self, addr: IpAddr, entry: &EntryInfo) -> io::Result<()>;
    async fn read_request(&self, stream: &mut Self::Stream) -> io::Result<EntryInfo>;

    async fn send_entry(&self, addr: IpAddr, entry: &EntryInfo, contents: &[u8]) -> io::Result<()>;
    async fn read_entry(&self, stream: &mut Self::Stream) -> io::Result<(EntryInfo, Vec<u8>)>;
}

pub trait TransportStream: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

pub struct TransportData<T: TransportStream> {
    pub src_id: Uuid,
    pub src_ip: IpAddr,
    pub kind: SyncKind,
    pub stream: T,
}

pub struct TransportSenders {
    pub metadata_tx: Sender<EntryInfo>,
    pub handshake_tx: Sender<(IpAddr, SyncHandshakeKind)>,
    pub request_tx: Sender<(IpAddr, EntryInfo)>,
    pub transfer_tx: Sender<(IpAddr, EntryInfo)>,
}

pub struct TransportReceivers {
    pub metadata_rx: Mutex<Receiver<EntryInfo>>,
    pub handshake_rx: Mutex<Receiver<(IpAddr, SyncHandshakeKind)>>,
    pub request_rx: Mutex<Receiver<(IpAddr, EntryInfo)>>,
    pub transfer_rx: Mutex<Receiver<(IpAddr, EntryInfo)>>,
}

pub struct ReceiverChannel<T: TransportInterface> {
    pub tx: Sender<TransportData<T::Stream>>,
    pub rx: Mutex<Receiver<TransportData<T::Stream>>>,
}

impl<T: TransportInterface> ReceiverChannel<T> {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(100);
        Self {
            tx,
            rx: Mutex::new(rx),
        }
    }
}
