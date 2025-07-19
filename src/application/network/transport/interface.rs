use crate::{
    domain::file::FileInfo,
    proto::tcp::{PeerSyncData, SyncKind},
};
use std::net::SocketAddr;
use tokio::io::{self, AsyncRead, AsyncWrite};

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

pub trait TransportStream: AsyncRead + AsyncWrite + Unpin + Send + 'static {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> TransportStream for T {}
