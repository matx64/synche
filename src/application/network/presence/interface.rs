use std::net::IpAddr;

pub trait PresenceInterface {
    async fn broadcast(&self, data: &[u8]) -> PresenceResult<()>;
    async fn recv(&self) -> PresenceResult<(String, IpAddr)>;
}

pub type PresenceResult<T> = Result<T, PresenceError>;

pub type PresenceError = String;
