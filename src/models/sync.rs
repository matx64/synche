use std::io::ErrorKind;
use tokio::io::Error;

#[repr(u8)]
pub enum SyncDataKind {
    HandshakeRequest = 0,
    HandshakeResponse = 1,
    File = 2,
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
