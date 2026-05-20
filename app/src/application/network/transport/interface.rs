use crate::domain::{TransportData, TransportEvent};
use std::net::IpAddr;
use tokio::io::{self};

/// Port for the peer-to-peer transport that carries handshakes,
/// metadata, requests, and entry transfers.
///
/// Implementations are bidirectional: `recv` blocks until the next
/// inbound `TransportEvent` is available; `send` delivers a
/// `TransportData` payload to a specific peer by address. The
/// production adapter is `TcpAdapter`.
///
/// `recv` errors after a connection is accepted are treated as bad
/// peer messages by the caller and the loop continues — a corrupt
/// transfer must not stop the synchronizer. Listener bind/accept
/// failures, by contrast, are fatal.
pub trait TransportInterface {
    /// Awaits the next inbound transport event.
    async fn recv(&self) -> TransportResult<TransportEvent>;

    /// Sends `data` to `target`. Returns once the payload has been
    /// written to the wire.
    async fn send(&self, target: IpAddr, data: TransportData) -> TransportResult<()>;
}

/// Result alias for fallible transport calls.
pub type TransportResult<T> = Result<T, TransportError>;

/// Error returned by `TransportInterface` implementors. A single
/// opaque variant — see `PersistenceError` for the same rationale.
pub enum TransportError {
    Failure(String),
}

impl TransportError {
    pub fn new(s: &str) -> Self {
        Self::Failure(s.into())
    }
}

impl From<TransportError> for io::Error {
    fn from(err: TransportError) -> Self {
        match err {
            TransportError::Failure(str) => io::Error::other(str),
        }
    }
}

impl From<io::Error> for TransportError {
    fn from(err: io::Error) -> Self {
        Self::Failure(err.to_string())
    }
}

impl From<serde_json::Error> for TransportError {
    fn from(err: serde_json::Error) -> Self {
        Self::Failure(err.to_string())
    }
}
