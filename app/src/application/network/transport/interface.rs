use crate::domain::transport::TransportDataV2;
use std::net::IpAddr;
use tokio::io::{self};
use uuid::Uuid;

pub trait TransportInterfaceV2 {
    async fn recv(&self) -> TransportResult<TransportRecvEvent>;

    async fn send(&self, target: IpAddr, data: TransportDataV2) -> TransportResult<()>;
}

pub struct TransportRecvEvent {
    pub src_id: Uuid,
    pub src_ip: IpAddr,
    pub data: TransportDataV2,
}

pub type TransportResult<T> = Result<T, TransportError>;

pub enum TransportError {
    Failure(String),
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
