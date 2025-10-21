use crate::{
    application::network::transport::interface::{TransportInterface, TransportResult},
    domain::{AppState, TransportData, TransportEvent, TransportMetadata},
    infra::network::tcp::{kind::TcpStreamKind, receiver::TcpReceiver, sender::TcpSender},
};
use std::{net::IpAddr, sync::Arc};
use tokio::{io::AsyncReadExt, net::TcpListener};
use uuid::Uuid;

pub struct TcpAdapter {
    sender: TcpSender,
    receiver: TcpReceiver,
    listener: TcpListener,
}

impl TcpAdapter {
    pub async fn new(state: Arc<AppState>) -> Self {
        let addr = format!("0.0.0.0:{}", state.ports.transport);
        let listener = TcpListener::bind(addr).await.unwrap();

        let receiver = TcpReceiver::new(state.clone());
        let sender = TcpSender::new(state);

        Self {
            sender,
            receiver,
            listener,
        }
    }
}

impl TransportInterface for TcpAdapter {
    async fn send(&self, target: IpAddr, data: TransportData) -> TransportResult<()> {
        self.sender.send_data(target, data).await
    }

    async fn recv(&self) -> TransportResult<TransportEvent> {
        let (mut stream, addr) = self.listener.accept().await?;

        let source_ip = addr.ip();

        let mut source_id_buf = [0u8; 16];
        stream.read_exact(&mut source_id_buf).await?;
        let source_id = Uuid::from_bytes(source_id_buf);

        let mut kind_buf = [0u8; 1];
        stream.read_exact(&mut kind_buf).await?;
        let kind = TcpStreamKind::try_from(kind_buf[0])?;

        let payload = self.receiver.read_data(stream, kind).await?;

        Ok(TransportEvent {
            metadata: TransportMetadata {
                source_id,
                source_ip,
            },
            payload,
        })
    }
}
