use crate::{
    application::network::transport::interface::{
        TransportInterface, TransportRecvEvent, TransportResult,
    },
    domain::{CanonicalPath, TransportData},
    infra::network::tcp::{kind::TcpStreamKind, receiver::TcpReceiver, sender::TcpSender},
};
use std::net::IpAddr;
use tokio::{io::AsyncReadExt, net::TcpListener};
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
    async fn recv(&self) -> TransportResult<TransportRecvEvent> {
        let (mut stream, src_addr) = self.listener.accept().await?;
        let src_ip = src_addr.ip();

        let mut src_id_buf = [0u8; 16];
        stream.read_exact(&mut src_id_buf).await?;
        let src_id = Uuid::from_bytes(src_id_buf);

        let mut kind_buf = [0u8; 1];
        stream.read_exact(&mut kind_buf).await?;
        let kind = TcpStreamKind::try_from(kind_buf[0])?;

        let data = self.receiver.read_data(stream, kind)?;

        Ok(TransportRecvEvent {
            src_id,
            src_ip,
            data,
        })
    }

    async fn send(&self, target: IpAddr, data: TransportData) -> TransportResult<()> {
        self.sender.send_data(target, data).await
    }
}
