use crate::domain::{Directory, FileInfo};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io::ErrorKind};
use tokio::io::Error;

#[derive(Debug, Serialize, Deserialize)]
pub struct PeerSyncData {
    pub directories: Vec<Directory>,
    pub files: HashMap<String, FileInfo>,
}

#[derive(Debug)]
pub enum SyncKind {
    Handshake(SyncHandshakeKind),
    File(SyncFileKind),
}

#[derive(Debug)]
pub enum SyncHandshakeKind {
    Request,
    Response,
}

#[derive(Debug)]
pub enum SyncFileKind {
    Metadata,
    Request,
    Transfer,
}

impl SyncKind {
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::Handshake(SyncHandshakeKind::Request) => 0,
            Self::Handshake(SyncHandshakeKind::Response) => 1,
            Self::File(SyncFileKind::Metadata) => 2,
            Self::File(SyncFileKind::Request) => 3,
            Self::File(SyncFileKind::Transfer) => 4,
        }
    }
}

impl std::fmt::Display for SyncKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Handshake(SyncHandshakeKind::Request) => write!(f, "Handshake Request"),
            Self::Handshake(SyncHandshakeKind::Response) => write!(f, "Handshake Response"),
            Self::File(SyncFileKind::Metadata) => write!(f, "File Metadata"),
            Self::File(SyncFileKind::Request) => write!(f, "File Request"),
            Self::File(SyncFileKind::Transfer) => write!(f, "File Transfer"),
        }
    }
}

impl TryFrom<u8> for SyncKind {
    type Error = Error;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SyncKind::Handshake(SyncHandshakeKind::Request)),
            1 => Ok(SyncKind::Handshake(SyncHandshakeKind::Response)),
            2 => Ok(SyncKind::File(SyncFileKind::Metadata)),
            3 => Ok(SyncKind::File(SyncFileKind::Request)),
            4 => Ok(SyncKind::File(SyncFileKind::Transfer)),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid SyncDataKind value",
            )),
        }
    }
}
