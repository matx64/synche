use std::io::ErrorKind;
use tokio::io::Error;

#[derive(Debug)]
pub enum SyncKind {
    Handshake(HandshakeSyncKind),
    File(FileSyncKind),
}

#[derive(Debug)]
pub enum HandshakeSyncKind {
    Request,
    Response,
}

#[derive(Debug)]
pub enum FileSyncKind {
    Metadata,
    Request,
    Transfer,
}

impl SyncKind {
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::Handshake(HandshakeSyncKind::Request) => 0,
            Self::Handshake(HandshakeSyncKind::Response) => 1,
            Self::File(FileSyncKind::Metadata) => 2,
            Self::File(FileSyncKind::Request) => 3,
            Self::File(FileSyncKind::Transfer) => 4,
        }
    }
}

impl std::fmt::Display for SyncKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Handshake(HandshakeSyncKind::Request) => write!(f, "Handshake Request"),
            Self::Handshake(HandshakeSyncKind::Response) => write!(f, "Handshake Response"),
            Self::File(FileSyncKind::Metadata) => write!(f, "File Metadata"),
            Self::File(FileSyncKind::Request) => write!(f, "File Request"),
            Self::File(FileSyncKind::Transfer) => write!(f, "File Transfer"),
        }
    }
}

impl TryFrom<u8> for SyncKind {
    type Error = Error;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SyncKind::Handshake(HandshakeSyncKind::Request)),
            1 => Ok(SyncKind::Handshake(HandshakeSyncKind::Response)),
            2 => Ok(SyncKind::File(FileSyncKind::Metadata)),
            3 => Ok(SyncKind::File(FileSyncKind::Request)),
            4 => Ok(SyncKind::File(FileSyncKind::Transfer)),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid SyncDataKind value",
            )),
        }
    }
}
