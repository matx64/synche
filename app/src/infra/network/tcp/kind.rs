use crate::{application::network::transport::interface::TransportError, domain::TransportData};

#[repr(u8)]
pub enum TcpStreamKind {
    HandshakeSyn = 1,
    HandshakeAck = 2,
    Metadata = 3,
    Request = 4,
    Transfer = 5,
}

impl TryFrom<u8> for TcpStreamKind {
    type Error = TransportError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::HandshakeSyn),
            2 => Ok(Self::HandshakeAck),
            3 => Ok(Self::Metadata),
            4 => Ok(Self::Request),
            5 => Ok(Self::Transfer),
            _ => Err(TransportError::new("Invalid Tcp Stream kind")),
        }
    }
}

impl From<&TransportData> for TcpStreamKind {
    fn from(value: &TransportData) -> Self {
        match value {
            TransportData::HandshakeSyn(_) => Self::HandshakeSyn,
            TransportData::HandshakeAck(_) => Self::HandshakeAck,
            TransportData::Metadata(_) => Self::Metadata,
            TransportData::Request(_) => Self::Request,
            TransportData::Transfer(_) => Self::Transfer,
        }
    }
}

impl std::fmt::Display for TcpStreamKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TcpStreamKind::HandshakeSyn => f.write_str("Handshake SYN"),
            TcpStreamKind::HandshakeAck => f.write_str("Handshake ACK"),
            TcpStreamKind::Metadata => f.write_str("Metadata"),
            TcpStreamKind::Request => f.write_str("Request"),
            TcpStreamKind::Transfer => f.write_str("Transfer"),
        }
    }
}
