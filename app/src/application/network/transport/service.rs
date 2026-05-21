use crate::{
    application::{
        AppState, EntryManager, PeerManager,
        network::transport::{
            interface::TransportInterface, receiver::TransportReceiver, sender::TransportSender,
        },
        persistence::interface::PersistenceInterface,
    },
    domain::{MutexChannel, TransportChannelData},
};
use std::sync::Arc;
use tokio::{io, sync::mpsc::Sender};

/// Pairs a `TransportSender` and `TransportReceiver` over the same
/// adapter, running them concurrently.
///
/// `new` returns the service together with the sender side of the
/// outbound channel so other subsystems (file watcher, presence) can
/// enqueue messages without holding a reference to the service.
pub struct TransportService<T: TransportInterface, P: PersistenceInterface> {
    sender: TransportSender<T, P>,
    receiver: TransportReceiver<T, P>,
}

impl<T: TransportInterface, P: PersistenceInterface> TransportService<T, P> {
    pub fn new(
        adapter: T,
        state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        entry_manager: Arc<EntryManager<P>>,
    ) -> (Self, Sender<TransportChannelData>) {
        let adapter = Arc::new(adapter);
        let sender_chan = MutexChannel::new(100);

        (
            Self {
                sender: TransportSender::new(
                    adapter.clone(),
                    state.clone(),
                    peer_manager.clone(),
                    entry_manager.clone(),
                    sender_chan.rx,
                ),
                receiver: TransportReceiver::new(
                    adapter,
                    state,
                    peer_manager,
                    entry_manager,
                    sender_chan.tx.clone(),
                ),
            },
            sender_chan.tx,
        )
    }

    pub async fn run(&self) -> io::Result<()> {
        tokio::select!(
            res = self.sender.run() => res,
            res = self.receiver.run() => res
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::network::transport::test_support::RecordingTransport,
        domain::{EntryInfo, EntryKind, HandshakeData, TransportEvent, TransportMetadata},
        infra::persistence::sqlite::SqliteDb,
    };
    use std::{
        collections::HashMap,
        net::{IpAddr, Ipv4Addr},
        time::Duration,
    };
    use uuid::Uuid;

    struct Harness {
        _env: crate::utils::test_support::TestEnv,
        service: TransportService<RecordingTransport, SqliteDb>,
        sender_tx: tokio::sync::mpsc::Sender<TransportChannelData>,
        peer_manager: Arc<PeerManager>,
        sends: Arc<tokio::sync::Mutex<Vec<(IpAddr, crate::domain::TransportData)>>>,
        push: tokio::sync::mpsc::UnboundedSender<
            crate::application::network::transport::interface::TransportResult<TransportEvent>,
        >,
    }

    async fn setup() -> Harness {
        let env = crate::utils::test_support::test_env().await;
        let state = env.state.clone();
        let db = SqliteDb::new(":memory:").await.unwrap();
        let entry_manager = EntryManager::new(db, state.clone());
        let peer_manager = PeerManager::new(state.clone());

        let adapter = RecordingTransport::new();
        let sends = adapter.sends.clone();
        let push = adapter.push_handle();

        let (service, sender_tx) =
            TransportService::new(adapter, state, peer_manager.clone(), entry_manager);

        Harness {
            _env: env,
            service,
            sender_tx,
            peer_manager,
            sends,
            push,
        }
    }

    fn handshake_event(source_id: Uuid, source_ip: IpAddr) -> TransportEvent {
        TransportEvent {
            payload: crate::domain::TransportData::HandshakeSyn(HandshakeData {
                hostname: "remote".into(),
                instance_id: Uuid::new_v4(),
                sync_dirs: Vec::new(),
                entries: HashMap::new(),
            }),
            metadata: TransportMetadata {
                source_id,
                source_ip,
            },
        }
    }

    fn metadata_event(source_id: Uuid, source_ip: IpAddr, entry: EntryInfo) -> TransportEvent {
        TransportEvent {
            payload: crate::domain::TransportData::Metadata(entry),
            metadata: TransportMetadata {
                source_id,
                source_ip,
            },
        }
    }

    /// Driving the service-level seam: pushing a `HandshakeSyn` onto the
    /// outbound channel must reach the adapter as a single `send` call.
    #[tokio::test]
    async fn new_returns_usable_sender_channel() {
        let h = setup().await;

        let target = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50));
        h.sender_tx
            .send(TransportChannelData::HandshakeSyn(target))
            .await
            .unwrap();

        let _ = tokio::time::timeout(Duration::from_millis(150), h.service.run()).await;

        let recorded = h.sends.lock().await;
        assert_eq!(recorded.len(), 1, "expected one outbound send");
        assert_eq!(recorded[0].0, target);
        assert!(matches!(
            recorded[0].1,
            crate::domain::TransportData::HandshakeSyn(_)
        ));
    }

    /// Inbound handshake should add the peer via `PeerManager` — that's
    /// the routing contract the service provides to the rest of the app.
    #[tokio::test]
    async fn run_routes_inbound_handshake_to_peer_manager() {
        let h = setup().await;

        let source_id = Uuid::new_v4();
        let source_ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 7));
        h.push
            .send(Ok(handshake_event(source_id, source_ip)))
            .unwrap();

        let _ = tokio::time::timeout(Duration::from_millis(150), h.service.run()).await;

        let peers = h.peer_manager.list().await;
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].id, source_id);
        assert_eq!(peers[0].addr, source_ip);
    }

    /// Inbound metadata for an unknown entry routes through `EntryManager`
    /// (yielding `KeepOther`) and then out as a `Request` — verifying the
    /// full receiver-to-application-to-sender wiring inside the service.
    #[tokio::test]
    async fn run_routes_inbound_metadata_to_outbound_request() {
        let h = setup().await;

        let peer_id = Uuid::new_v4();
        let entry = EntryInfo {
            name: "Default Folder/file.txt".into(),
            kind: EntryKind::File,
            hash: Some("h".into()),
            version: HashMap::from([(peer_id, 1)]),
        };

        h.push
            .send(Ok(metadata_event(
                peer_id,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                entry.clone(),
            )))
            .unwrap();

        let _ = tokio::time::timeout(Duration::from_millis(150), h.service.run()).await;

        let recorded = h.sends.lock().await;
        assert_eq!(recorded.len(), 1);
        assert!(matches!(
            recorded[0].1,
            crate::domain::TransportData::Request(_)
        ));
    }
}
