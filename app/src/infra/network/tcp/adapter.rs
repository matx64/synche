use crate::{
    application::network::transport::interface::{TransportInterface, TransportResult},
    domain::{CanonicalPath, TransportData, TransportEvent, TransportMetadata},
    infra::network::tcp::{kind::TcpStreamKind, receiver::TcpReceiver, sender::TcpSender},
};
use futures::io;
use std::net::{IpAddr, SocketAddr};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
};
use uuid::Uuid;

pub struct TcpAdapter {
    sender: TcpSender,
    receiver: TcpReceiver,
    listener: TcpListener,
}

impl TcpAdapter {
    pub async fn new(port: u16, local_id: Uuid, home_path: CanonicalPath) -> Self {
        let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap();

        let receiver = TcpReceiver::new(local_id.clone(), home_path.clone());
        let sender = TcpSender::new(port, local_id, home_path);

        Self {
            sender,
            receiver,
            listener,
        }
    }
}

impl TransportInterface for TcpAdapter {
    async fn recv(&self) -> TransportResult<TransportEvent> {
        let (mut stream, addr) = self.listener.accept().await?;

        let (metadata, kind) = self.read_metadata(stream, addr).await;

        let payload = self.receiver.read_data(stream, kind)?;

        Ok(TransportEvent { metadata, payload })
    }

    async fn send(&self, target: IpAddr, data: TransportData) -> TransportResult<()> {
        self.sender.send_data(target, data).await
    }
}

impl TcpAdapter {
    async fn read_metadata(
        &self,
        mut stream: TcpStream,
        addr: SocketAddr,
    ) -> io::Result<(TransportMetadata, TcpStreamKind)> {
        let source_ip = addr.ip();

        let mut source_id_buf = [0u8; 16];
        stream.read_exact(&mut source_id_buf).await?;
        let source_id = Uuid::from_bytes(source_id_buf);

        let mut kind_buf = [0u8; 1];
        stream.read_exact(&mut kind_buf).await?;
        let kind = TcpStreamKind::try_from(kind_buf[0])?;

        Ok((
            TransportMetadata {
                source_id,
                source_ip,
            },
            kind,
        ))
    }
}
