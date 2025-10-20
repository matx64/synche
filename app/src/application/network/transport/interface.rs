use crate::domain::{TransportData, TransportEvent};
use std::net::IpAddr;
use tokio::io::{self};

pub trait TransportInterface {
    async fn recv(&self) -> TransportResult<TransportEvent>;

    async fn send(&self, target: IpAddr, data: TransportData) -> TransportResult<()>;
}

pub type TransportResult<T> = Result<T, TransportError>;

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
