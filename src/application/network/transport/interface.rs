use crate::{
    domain::FileInfo,
    proto::tcp::{PeerSyncData, SyncHandshakeKind, SyncKind},
};
use std::net::SocketAddr;
use tokio::{
    io::{self, AsyncRead, AsyncWrite},
    sync::mpsc::{Receiver, Sender},
};

pub trait TransportInterface {
    type Stream: TransportStream;

    async fn recv(&self) -> io::Result<(Self::Stream, SyncKind)>;

    async fn send_handshake(
        &self,
        addr: SocketAddr,
        kind: SyncKind,
        data: PeerSyncData,
    ) -> io::Result<()>;
    async fn read_handshake(&self, stream: &mut Self::Stream) -> io::Result<PeerSyncData>;

    async fn send_metadata(&self, addr: SocketAddr, file: &FileInfo) -> io::Result<()>;
    async fn read_metadata(&self, stream: &mut Self::Stream) -> io::Result<FileInfo>;

    async fn send_request(&self, addr: SocketAddr, file: &FileInfo) -> io::Result<()>;
    async fn read_request(&self, stream: &mut Self::Stream) -> io::Result<FileInfo>;

    async fn send_file(&self, addr: SocketAddr, file: &FileInfo, contents: &[u8])
    -> io::Result<()>;
    async fn read_file(&self, stream: &mut Self::Stream) -> io::Result<(FileInfo, Vec<u8>)>;
}

pub trait TransportStream: TransportStreamExt {}
impl<T: TransportStreamExt> TransportStream for T {}

pub trait TransportStreamExt: AsyncRead + AsyncWrite + Unpin + Send + 'static {
    fn peer_addr(&self) -> io::Result<SocketAddr>;
}

pub struct TransportSenders {
    pub watch_tx: Sender<FileInfo>,
    pub handshake_tx: Sender<(SocketAddr, SyncHandshakeKind)>,
    pub request_tx: Sender<(SocketAddr, FileInfo)>,
    pub transfer_tx: Sender<(SocketAddr, FileInfo)>,
}

pub struct TransportReceivers {
    pub watch_rx: Receiver<FileInfo>,
    pub handshake_rx: Receiver<(SocketAddr, SyncHandshakeKind)>,
    pub request_rx: Receiver<(SocketAddr, FileInfo)>,
    pub transfer_rx: Receiver<(SocketAddr, FileInfo)>,
}
