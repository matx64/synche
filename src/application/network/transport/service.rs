use crate::{
    application::network::{TransportInterface, transport::interface::TransportStreamExt},
    domain::{Peer, PeerManager},
    proto::tcp::{SyncFileKind, SyncHandshakeKind, SyncKind},
};
use std::sync::Arc;
use tokio::io;

pub struct TransportService<T: TransportInterface> {
    transport_adapter: T,
    peer_manager: Arc<PeerManager>,
}

impl<T: TransportInterface> TransportService<T> {
    pub fn new(transport_adapter: T, peer_manager: Arc<PeerManager>) -> Self {
        Self {
            transport_adapter,
            peer_manager,
        }
    }

    pub async fn recv(&self) -> io::Result<()> {
        loop {
            let (stream, kind) = self.transport_adapter.recv().await?;

            // TODO: Async
            match kind {
                SyncKind::Handshake(kind) => {
                    self.handle_handshake(stream, kind).await?;
                }
                SyncKind::File(SyncFileKind::Metadata) => {}
                SyncKind::File(SyncFileKind::Request) => {}
                SyncKind::File(SyncFileKind::Transfer) => {}
            }
        }
    }

    pub async fn handle_handshake(
        &self,
        mut stream: T::Stream,
        kind: SyncHandshakeKind,
    ) -> io::Result<()> {
        let src_addr = stream.peer_addr()?;

        let data = self.transport_adapter.read_handshake(&mut stream).await?;

        self.peer_manager.insert(Peer::new(src_addr, Some(data)));

        if matches!(kind, SyncHandshakeKind::Request) {
            // TODO: Send Handshake Response
        }

        // TODO: Sync peers

        Ok(())
    }
}
