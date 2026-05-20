use crate::{
    application::AppState,
    application::network::transport::interface::{
        TransportError, TransportInterface, TransportResult,
    },
    domain::{TransportData, TransportEvent, TransportMetadata},
    infra::network::tcp::{kind::TcpStreamKind, receiver::TcpReceiver, sender::TcpSender},
};
use std::{net::IpAddr, sync::Arc};
use tokio::{io::AsyncReadExt, net::TcpListener};
use tracing::{trace, warn};
use uuid::Uuid;

/// TCP adapter implementing `TransportInterface`.
///
/// Owns the listening socket plus a `TcpSender` / `TcpReceiver` pair
/// that implement the wire format. Listener bind/accept failures are
/// fatal; framing errors on an accepted connection are logged and the
/// loop continues so a single bad peer cannot stop the synchronizer.
pub struct TcpAdapter {
    sender: TcpSender,
    receiver: TcpReceiver,
    listener: TcpListener,
}

impl TcpAdapter {
    pub async fn new(state: Arc<AppState>) -> Self {
        let addr = format!("0.0.0.0:{}", state.ports().transport);
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
        loop {
            let (mut stream, addr) = self.listener.accept().await?;

            let source_ip = addr.ip();
            trace!(peer = %source_ip, "tcp listener accepted");

            let result: TransportResult<TransportEvent> = async {
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
            .await;

            match result {
                Ok(event) => return Ok(event),
                Err(TransportError::Failure(message)) => {
                    warn!(peer = ?source_ip, "Ignoring invalid TCP transport message: {message}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{EntryInfo, EntryKind};
    use std::{collections::HashMap, time::Duration};
    use tokio::{io::AsyncWriteExt, net::TcpStream, time::timeout};

    async fn write_metadata(stream: &mut TcpStream, source_id: Uuid, entry: &EntryInfo) {
        let contents = serde_json::to_vec(entry).unwrap();

        stream.write_all(source_id.as_bytes()).await.unwrap();
        stream
            .write_all(&[TcpStreamKind::Metadata as u8])
            .await
            .unwrap();
        stream
            .write_all(&(contents.len() as u32).to_be_bytes())
            .await
            .unwrap();
        stream.write_all(&contents).await.unwrap();
    }

    async fn write_corrupt_transfer(stream: &mut TcpStream, source_id: Uuid, entry: &EntryInfo) {
        let contents = b"not the advertised hash";
        let entry_json = serde_json::to_vec(entry).unwrap();

        stream.write_all(source_id.as_bytes()).await.unwrap();
        stream
            .write_all(&[TcpStreamKind::Transfer as u8])
            .await
            .unwrap();
        stream
            .write_all(&(entry_json.len() as u32).to_be_bytes())
            .await
            .unwrap();
        stream.write_all(&entry_json).await.unwrap();
        stream
            .write_all(&(contents.len() as u64).to_be_bytes())
            .await
            .unwrap();
        stream.write_all(contents).await.unwrap();
    }

    fn file_entry(name: &str, hash: &str) -> EntryInfo {
        EntryInfo {
            name: name.into(),
            kind: EntryKind::File,
            hash: Some(hash.to_string()),
            version: HashMap::from([(Uuid::new_v4(), 1)]),
        }
    }

    #[tokio::test]
    async fn recv_ignores_corrupt_transfer_and_keeps_listening() {
        let env = crate::utils::test_support::test_env().await;
        let adapter = TcpAdapter::new(env.state.clone()).await;
        let addr = adapter.listener.local_addr().unwrap();
        let source_id = Uuid::new_v4();
        let corrupt_entry = file_entry("bad/payload.bin", "deadbeef");
        let metadata_entry = file_entry("ok/payload.bin", "hash");

        let metadata_entry_clone = metadata_entry.clone();
        let writer = tokio::spawn(async move {
            let mut bad_stream = TcpStream::connect(addr).await.unwrap();
            write_corrupt_transfer(&mut bad_stream, source_id, &corrupt_entry).await;
            drop(bad_stream);

            let mut good_stream = TcpStream::connect(addr).await.unwrap();
            write_metadata(&mut good_stream, source_id, &metadata_entry_clone).await;
        });

        let result = timeout(Duration::from_secs(2), adapter.recv())
            .await
            .expect("adapter should keep listening after corrupt transfer");
        writer.await.unwrap();

        let event = match result {
            Ok(event) => event,
            Err(TransportError::Failure(message)) => {
                panic!("unexpected transport error: {message}")
            }
        };

        assert_eq!(event.metadata.source_id, source_id);
        match event.payload {
            TransportData::Metadata(entry) => assert_eq!(entry.name, metadata_entry.name),
            _ => panic!("expected metadata after corrupt transfer"),
        }
    }
}
