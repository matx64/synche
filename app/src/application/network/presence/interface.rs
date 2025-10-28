use std::net::IpAddr;
use tokio::io;
use uuid::Uuid;

pub trait PresenceInterface {
    async fn advertise(&self) -> io::Result<()>;
    async fn next(&self) -> io::Result<Option<PresenceEvent>>;
    async fn shutdown(&self);
}

pub enum PresenceEvent {
    Ping {
        id: Uuid,
        ip: IpAddr,
        hostname: String,
    },
    Disconnect(Uuid),
}
