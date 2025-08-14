use crate::domain::{Directory, EntryInfo};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, io::ErrorKind};
use tokio::io::Error;

#[derive(Debug, Serialize, Deserialize)]
pub struct PeerHandshakeData {
    pub directories: Vec<Directory>,
    pub entries: HashMap<String, EntryInfo>,
}

#[derive(Debug)]
pub enum SyncKind {
    Handshake(SyncHandshakeKind),
    Entry(SyncEntryKind),
}

#[derive(Debug, Clone)]
pub enum SyncHandshakeKind {
    Request,
    Response,
}

#[derive(Debug)]
pub enum SyncEntryKind {
    Metadata,
    Request,
    Transfer,
}

impl SyncKind {
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::Handshake(SyncHandshakeKind::Request) => 0,
            Self::Handshake(SyncHandshakeKind::Response) => 1,
            Self::Entry(SyncEntryKind::Metadata) => 2,
            Self::Entry(SyncEntryKind::Request) => 3,
            Self::Entry(SyncEntryKind::Transfer) => 4,
        }
    }
}

impl std::fmt::Display for SyncKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Handshake(SyncHandshakeKind::Request) => write!(f, "Handshake Request"),
            Self::Handshake(SyncHandshakeKind::Response) => write!(f, "Handshake Response"),
            Self::Entry(SyncEntryKind::Metadata) => write!(f, "Entry Metadata"),
            Self::Entry(SyncEntryKind::Request) => write!(f, "Entry Request"),
            Self::Entry(SyncEntryKind::Transfer) => write!(f, "Entry Transfer"),
        }
    }
}

impl TryFrom<u8> for SyncKind {
    type Error = Error;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SyncKind::Handshake(SyncHandshakeKind::Request)),
            1 => Ok(SyncKind::Handshake(SyncHandshakeKind::Response)),
            2 => Ok(SyncKind::Entry(SyncEntryKind::Metadata)),
            3 => Ok(SyncKind::Entry(SyncEntryKind::Request)),
            4 => Ok(SyncKind::Entry(SyncEntryKind::Transfer)),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                "Invalid SyncDataKind value",
            )),
        }
    }
}
