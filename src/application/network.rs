use crate::{
    domain::file::FileInfo,
    proto::tcp::{PeerSyncData, SyncKind},
};
use std::net::SocketAddr;
use tokio::{io, net::TcpStream};

pub trait PresenceInterface {
    async fn broadcast(&self, data: &[u8]) -> io::Result<()>;
    async fn recv(&self) -> io::Result<(Vec<u8>, SocketAddr)>;
}

pub trait TransportInterface {
    async fn recv(&self) -> io::Result<(TcpStream, SyncKind)>;

    async fn send_handshake(
        &self,
        addr: SocketAddr,
        kind: SyncKind,
        data: PeerSyncData,
    ) -> io::Result<()>;
    async fn read_handshake(&self, stream: &mut TcpStream) -> io::Result<PeerSyncData>;

    async fn send_metadata(&self, addr: SocketAddr, file: &FileInfo) -> io::Result<()>;
    async fn read_metadata(&self, stream: &mut TcpStream) -> io::Result<FileInfo>;

    async fn send_request(&self, addr: SocketAddr, file: &FileInfo) -> io::Result<()>;
    async fn read_request(&self, stream: &mut TcpStream) -> io::Result<FileInfo>;

    async fn send_file(&self, addr: SocketAddr, file: &FileInfo, contents: &[u8])
    -> io::Result<()>;
    async fn read_file(&self, stream: &mut TcpStream) -> io::Result<(FileInfo, Vec<u8>)>;
}
