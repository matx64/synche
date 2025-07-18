use crate::application::network::TcpPort;
use tokio::{io, net::TcpListener};

pub struct TcpTransporter {
    listener: TcpListener,
}

impl TcpTransporter {
    pub async fn new(port: u16) -> Self {
        let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap();

        Self { listener }
    }
}

impl TcpPort for TcpTransporter {
    async fn recv(&self) -> io::Result<(tokio::net::TcpStream, crate::proto::tcp::SyncKind)> {
        todo!()
    }

    async fn send_handshake(
        &self,
        addr: std::net::SocketAddr,
        kind: crate::proto::tcp::SyncHandshakeKind,
        contents: &[u8],
    ) -> io::Result<()> {
        todo!()
    }

    async fn read_handshake(&self, stream: &mut tokio::net::TcpStream) -> io::Result<Vec<u8>> {
        todo!()
    }

    async fn send_metadata(
        &self,
        addr: std::net::SocketAddr,
        file: &crate::domain::file::File,
    ) -> io::Result<()> {
        todo!()
    }

    async fn read_metadata(
        &self,
        stream: &mut tokio::net::TcpStream,
    ) -> io::Result<crate::domain::file::File> {
        todo!()
    }

    async fn send_request(
        &self,
        addr: std::net::SocketAddr,
        file: &crate::domain::file::File,
    ) -> io::Result<()> {
        todo!()
    }

    async fn read_request(
        &self,
        stream: &mut tokio::net::TcpStream,
    ) -> io::Result<crate::domain::file::File> {
        todo!()
    }

    async fn send_file(
        &self,
        addr: std::net::SocketAddr,
        file: &crate::domain::file::File,
    ) -> io::Result<()> {
        todo!()
    }

    async fn read_file(
        &self,
        stream: &mut tokio::net::TcpStream,
    ) -> io::Result<(crate::domain::file::File, Vec<u8>)> {
        todo!()
    }
}
