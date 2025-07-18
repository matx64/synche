use crate::{
    domain::file::File,
    proto::tcp::{PeerSyncData, SyncKind},
};
use std::net::SocketAddr;
use tokio::{io, net::TcpStream};

pub trait BroadcastPort {
    async fn broadcast(&self, data: &[u8]) -> io::Result<()>;
    async fn recv(&self) -> io::Result<(Vec<u8>, SocketAddr)>;
}

pub trait TcpPort {
    async fn recv(&self) -> io::Result<(TcpStream, SyncKind)>;

    async fn send_handshake(
        &self,
        addr: SocketAddr,
        kind: SyncKind,
        data: PeerSyncData,
    ) -> io::Result<()>;
    async fn read_handshake(&self, stream: &mut TcpStream) -> io::Result<PeerSyncData>;

    async fn send_metadata(&self, addr: SocketAddr, file: &File) -> io::Result<()>;
    async fn read_metadata(&self, stream: &mut TcpStream) -> io::Result<File>;

    async fn send_request(&self, addr: SocketAddr, file: &File) -> io::Result<()>;
    async fn read_request(&self, stream: &mut TcpStream) -> io::Result<File>;

    async fn send_file(&self, addr: SocketAddr, file: &File) -> io::Result<()>;
    async fn read_file(&self, stream: &mut TcpStream) -> io::Result<(File, Vec<u8>)>;
}
