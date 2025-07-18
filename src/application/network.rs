use std::net::SocketAddr;

pub trait NetworkPort {
    async fn send(&self, data: &[u8]) -> tokio::io::Result<()>;
    async fn recv(&self) -> tokio::io::Result<(Vec<u8>, SocketAddr)>;
}
