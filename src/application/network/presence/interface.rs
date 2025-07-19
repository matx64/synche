use std::net::SocketAddr;
use tokio::io::{self};

pub trait PresenceInterface {
    async fn broadcast(&self, data: &[u8]) -> io::Result<()>;
    async fn recv(&self) -> io::Result<(String, SocketAddr)>;
}
