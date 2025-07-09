use std::{io::ErrorKind, time::SystemTime};
use tokio::io::Error;

pub struct ReceivedFile {
    pub name: String,
    pub size: u64,
    pub contents: Vec<u8>,
    pub hash: String,
    pub last_modified_at: SystemTime,
}

#[repr(u8)]
pub enum SyncDataKind {
    HandshakeRequest = 0,
    HandshakeResponse = 1,
    File = 2,
}

impl std::fmt::Display for SyncDataKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HandshakeRequest => write!(f, "HandshakeRequest"),
            Self::HandshakeResponse => write!(f, "HandshakeResponse"),
            Self::File => write!(f, "File"),
        }
    }
}

impl TryFrom<u8> for SyncDataKind {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SyncDataKind::HandshakeRequest),
            1 => Ok(SyncDataKind::HandshakeResponse),
            2 => Ok(SyncDataKind::File),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid SyncDataKind value",
            )),
        }
    }
}
