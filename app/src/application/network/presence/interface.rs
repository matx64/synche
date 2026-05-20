use std::net::IpAddr;
use tokio::io;
use uuid::Uuid;

/// Port for peer discovery and liveness announcements.
///
/// `advertise` publishes this instance on the local network;
/// `next` blocks for the next observed peer event; `shutdown`
/// retracts the advertisement before the process exits so peers
/// learn of the disconnect immediately rather than waiting for a
/// timeout.
///
/// The production adapter is `MdnsAdapter`.
pub trait PresenceInterface {
    /// Starts announcing this instance to peers.
    async fn advertise(&self) -> io::Result<()>;
    /// Awaits the next presence event from the network. `None` means
    /// the underlying stream has terminated.
    async fn next(&self) -> io::Result<Option<PresenceEvent>>;
    /// Retracts the advertisement and stops the underlying service.
    async fn shutdown(&self);
}

/// Discovery-layer event delivered by `PresenceInterface::next`.
pub enum PresenceEvent {
    /// A peer announced itself (or reconfirmed liveness). A change in
    /// `instance_id` for the same `id` indicates the peer restarted.
    Ping {
        id: Uuid,
        addr: IpAddr,
        instance_id: Uuid,
    },
    /// A peer explicitly retracted its advertisement.
    Disconnect(Uuid),
}
