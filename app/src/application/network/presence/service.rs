use crate::{
    application::AppState,
    application::{
        PeerManager,
        network::presence::interface::{PresenceEvent, PresenceInterface},
    },
    domain::TransportChannelData,
};
use std::{net::IpAddr, sync::Arc};
use tokio::{io, sync::mpsc::Sender};
use tracing::warn;
use uuid::Uuid;

pub struct PresenceService<P: PresenceInterface> {
    adapter: P,
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    sender_tx: Sender<TransportChannelData>,
}

impl<P: PresenceInterface> PresenceService<P> {
    pub fn new(
        adapter: P,
        state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        sender_tx: Sender<TransportChannelData>,
    ) -> Self {
        Self {
            state,
            adapter,
            sender_tx,
            peer_manager,
        }
    }

    pub async fn run(&self) -> io::Result<()> {
        self.adapter.advertise().await?;

        while let Some(event) = self.adapter.next().await? {
            match event {
                PresenceEvent::Ping {
                    id,
                    addr,
                    instance_id,
                } => {
                    self.handle_ping(id, addr, instance_id).await?;
                }

                PresenceEvent::Disconnect(id) => {
                    self.handle_disconnect(id).await?;
                }
            }
        }
        warn!("Presence adapter channel closed");
        Ok(())
    }

    async fn handle_ping(&self, id: Uuid, addr: IpAddr, instance_id: Uuid) -> io::Result<()> {
        let seen = self.peer_manager.seen(&id, &instance_id).await;

        if !seen && self.state.local_id() < id {
            self.sender_tx
                .send(TransportChannelData::HandshakeSyn(addr))
                .await
                .map_err(io::Error::other)?;
        }
        Ok(())
    }

    async fn handle_disconnect(&self, id: Uuid) -> io::Result<()> {
        self.peer_manager.remove_peer(id).await;
        Ok(())
    }

    pub async fn shutdown(&self) {
        self.adapter.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::network::presence::interface::{PresenceEvent, PresenceInterface};
    use std::net::{IpAddr, Ipv4Addr};
    use tokio::sync::{Mutex, mpsc};
    use uuid::Uuid;

    struct MockPresenceAdapter {
        events: Arc<Mutex<Vec<PresenceEvent>>>,
        advertise_called: Arc<Mutex<bool>>,
        shutdown_called: Arc<Mutex<bool>>,
    }

    impl MockPresenceAdapter {
        fn new(events: Vec<PresenceEvent>) -> Self {
            Self {
                events: Arc::new(Mutex::new(events)),
                advertise_called: Arc::new(Mutex::new(false)),
                shutdown_called: Arc::new(Mutex::new(false)),
            }
        }

        async fn advertise_was_called(&self) -> bool {
            *self.advertise_called.lock().await
        }

        async fn shutdown_was_called(&self) -> bool {
            *self.shutdown_called.lock().await
        }
    }

    impl PresenceInterface for MockPresenceAdapter {
        async fn advertise(&self) -> io::Result<()> {
            *self.advertise_called.lock().await = true;
            Ok(())
        }

        async fn next(&self) -> io::Result<Option<PresenceEvent>> {
            let mut events = self.events.lock().await;
            Ok(events.pop())
        }

        async fn shutdown(&self) {
            *self.shutdown_called.lock().await = true;
        }
    }

    async fn create_test_components() -> (
        Arc<AppState>,
        Arc<PeerManager>,
        mpsc::Sender<TransportChannelData>,
        mpsc::Receiver<TransportChannelData>,
    ) {
        let state = AppState::new().await;
        let peer_manager = PeerManager::new(state.clone());
        let (sender_tx, sender_rx) = mpsc::channel(10);
        (state, peer_manager, sender_tx, sender_rx)
    }

    #[tokio::test]
    async fn test_service_calls_advertise_on_run() {
        let (state, peer_manager, sender_tx, _sender_rx) = create_test_components().await;

        let adapter = MockPresenceAdapter::new(vec![]);
        let service = PresenceService::new(adapter, state, peer_manager, sender_tx);
        service.run().await.unwrap();

        assert!(
            service.adapter.advertise_was_called().await,
            "advertise() should have been called"
        );
    }

    #[tokio::test]
    async fn test_handle_ping_sends_handshake_when_not_seen_and_local_id_smaller() {
        let (state, peer_manager, sender_tx, mut sender_rx) = create_test_components().await;

        let addr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        let larger_id = Uuid::from_u128(u128::MAX);

        let ping_event = PresenceEvent::Ping {
            id: larger_id,
            addr,
            instance_id: Uuid::new_v4(),
        };

        let adapter = MockPresenceAdapter::new(vec![ping_event]);
        let service = PresenceService::new(adapter, state, peer_manager, sender_tx);
        tokio::spawn(async move {
            let _ = service.run().await;
        });

        let received =
            tokio::time::timeout(tokio::time::Duration::from_millis(100), sender_rx.recv()).await;

        assert!(received.is_ok(), "Should receive handshake message");
        if let Ok(Some(TransportChannelData::HandshakeSyn(received_addr))) = received {
            assert_eq!(received_addr, addr);
        } else {
            panic!("Expected HandshakeSyn message");
        }
    }

    #[tokio::test]
    async fn test_handle_ping_does_not_send_handshake_when_local_id_larger() {
        let (state, peer_manager, sender_tx, mut sender_rx) = create_test_components().await;

        let addr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        let smaller_id = Uuid::from_u128(0);

        let ping_event = PresenceEvent::Ping {
            id: smaller_id,
            addr,
            instance_id: Uuid::new_v4(),
        };

        let adapter = MockPresenceAdapter::new(vec![ping_event]);
        let service = PresenceService::new(adapter, state, peer_manager, sender_tx);
        tokio::spawn(async move {
            let _ = service.run().await;
        });

        let received =
            tokio::time::timeout(tokio::time::Duration::from_millis(100), sender_rx.recv()).await;

        // Either timeout (Err) or channel closed (Ok(None)) means no handshake was sent
        assert!(
            matches!(received, Err(_) | Ok(None)),
            "Should not send handshake when local ID is larger"
        );
    }

    #[tokio::test]
    async fn test_handle_ping_does_not_send_handshake_when_already_seen() {
        use crate::domain::Peer;
        use std::time::SystemTime;

        let (state, peer_manager, sender_tx, mut sender_rx) = create_test_components().await;

        let remote_id = Uuid::from_u128(u128::MAX);
        let instance_id = Uuid::new_v4();
        let addr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

        peer_manager
            .insert(Peer {
                id: remote_id,
                instance_id,
                addr,
                hostname: "test-peer".to_string(),
                last_seen: SystemTime::now(),
                sync_dirs: Default::default(),
            })
            .await;

        let ping_event = PresenceEvent::Ping {
            id: remote_id,
            addr,
            instance_id,
        };
        let adapter = MockPresenceAdapter::new(vec![ping_event]);
        let service = PresenceService::new(adapter, state, peer_manager, sender_tx);
        tokio::spawn(async move {
            let _ = service.run().await;
        });

        let received =
            tokio::time::timeout(tokio::time::Duration::from_millis(100), sender_rx.recv()).await;

        // Either timeout (Err) or channel closed (Ok(None)) means no handshake was sent
        assert!(
            matches!(received, Err(_) | Ok(None)),
            "Should not send handshake when peer already seen"
        );
    }

    #[tokio::test]
    async fn test_handle_disconnect_removes_peer() {
        use crate::domain::Peer;
        use std::time::SystemTime;

        let (state, peer_manager, sender_tx, _sender_rx) = create_test_components().await;

        let peer_id = Uuid::new_v4();
        let instance_id = Uuid::new_v4();
        let addr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));

        peer_manager
            .insert(Peer {
                id: peer_id,
                instance_id,
                addr,
                hostname: "test-peer".to_string(),
                last_seen: SystemTime::now(),
                sync_dirs: Default::default(),
            })
            .await;

        let peers_before = peer_manager.list().await;
        assert!(
            peers_before.iter().any(|p| p.id == peer_id),
            "Peer should exist before disconnect"
        );

        let disconnect_event = PresenceEvent::Disconnect(peer_id);

        let adapter = MockPresenceAdapter::new(vec![disconnect_event]);
        let peer_manager_clone = peer_manager.clone();
        let service = PresenceService::new(adapter, state, peer_manager, sender_tx);
        tokio::spawn(async move {
            let _ = service.run().await;
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let peers_after = peer_manager_clone.list().await;
        assert!(
            !peers_after.iter().any(|p| p.id == peer_id),
            "Peer should be removed after disconnect"
        );
    }

    #[tokio::test]
    async fn test_shutdown_calls_adapter_shutdown() {
        let (state, peer_manager, sender_tx, _sender_rx) = create_test_components().await;
        let adapter = MockPresenceAdapter::new(vec![]);
        let service = PresenceService::new(adapter, state, peer_manager, sender_tx);

        assert!(!service.adapter.shutdown_was_called().await);
        service.shutdown().await;
        assert!(service.adapter.shutdown_was_called().await);
    }

    #[tokio::test]
    async fn test_handle_multiple_events_sequentially() {
        let (state, peer_manager, sender_tx, mut sender_rx) = create_test_components().await;

        let peer1_id = Uuid::from_u128(u128::MAX);
        let peer2_id = Uuid::from_u128(u128::MAX - 1);
        let addr1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        let addr2 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 101));

        let events = vec![
            PresenceEvent::Disconnect(peer2_id),
            PresenceEvent::Ping {
                id: peer2_id,
                addr: addr2,
                instance_id: Uuid::new_v4(),
            },
            PresenceEvent::Ping {
                id: peer1_id,
                addr: addr1,
                instance_id: Uuid::new_v4(),
            },
        ];

        let adapter = MockPresenceAdapter::new(events);
        let service = PresenceService::new(adapter, state, peer_manager, sender_tx);
        tokio::spawn(async move {
            let _ = service.run().await;
        });

        let msg1 =
            tokio::time::timeout(tokio::time::Duration::from_millis(100), sender_rx.recv()).await;
        assert!(msg1.is_ok(), "Should receive first handshake");

        let msg2 =
            tokio::time::timeout(tokio::time::Duration::from_millis(100), sender_rx.recv()).await;
        assert!(msg2.is_ok(), "Should receive second handshake");
    }
}
