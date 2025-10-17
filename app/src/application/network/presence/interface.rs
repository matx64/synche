use std::net::IpAddr;
use tokio::io;
use uuid::Uuid;

pub trait PresenceInterface {
    async fn advertise(&self);
    async fn recv(&self) -> io::Result<PresenceEvent>;
    async fn shutdown(&self);
}

pub enum PresenceEvent {
    Ping((Uuid, IpAddr)),
    Disconnect(Uuid),
}
