use crate::{
    application::{
        network::{
            PresenceInterface, TransportInterface,
            presence::PresenceService,
            transport::{TransportReceiver, TransportSender},
        },
        watcher::{FileWatcher, FileWatcherInterface},
    },
    config::AppState,
    infra::{
        network::{tcp::TcpTransporter, udp::UdpBroadcaster},
        watcher::notify::NotifyFileWatcher,
    },
};
use std::sync::Arc;
use tokio::io;

pub struct Synchronizer<W: FileWatcherInterface, P: PresenceInterface, T: TransportInterface> {
    file_watcher: FileWatcher<W>,
    presence_service: PresenceService<P>,
    transport_sender: TransportSender<T>,
    transport_receiver: TransportReceiver<T>,
}

impl<W: FileWatcherInterface, P: PresenceInterface, T: TransportInterface> Synchronizer<W, P, T> {
    pub fn new(
        state: AppState,
        watch_adapter: W,
        presence_adapter: P,
        transport_adapter: T,
    ) -> Self {
        let entry_manager = Arc::new(state.entry_manager);
        let peer_manager = Arc::new(state.peer_manager);
        let transport_adapter = Arc::new(transport_adapter);

        let (transport_sender, sender_channels) = TransportSender::new(
            transport_adapter.clone(),
            entry_manager.clone(),
            peer_manager.clone(),
            state.constants.base_dir.clone(),
        );

        let file_watcher = FileWatcher::new(
            watch_adapter,
            entry_manager.clone(),
            sender_channels.watch_tx.clone(),
            state.constants.base_dir.clone(),
        );

        let presence_service = PresenceService::new(
            presence_adapter,
            peer_manager.clone(),
            sender_channels.handshake_tx.clone(),
            state.constants.broadcast_interval_secs,
        );

        let transport_receiver = TransportReceiver::new(
            transport_adapter,
            entry_manager,
            peer_manager,
            sender_channels,
            state.constants.base_dir,
            state.constants.tmp_dir,
        );

        Self {
            file_watcher,
            presence_service,
            transport_sender,
            transport_receiver,
        }
    }

    pub async fn run(&mut self) -> io::Result<()> {
        tokio::try_join!(
            self.presence_service.monitor_peers(),
            self.presence_service.run_broadcast(),
            self.presence_service.run_recv(),
            self.file_watcher.run(),
            self.transport_receiver.recv(),
            self.transport_sender.send_file_changes(),
            self.transport_sender.send_handshakes(),
            self.transport_sender.send_requests(),
            self.transport_sender.send_files(),
        )?;
        Ok(())
    }
}

impl Synchronizer<NotifyFileWatcher, UdpBroadcaster, TcpTransporter> {
    pub async fn new_default(state: AppState) -> Self {
        Self::new(
            state,
            NotifyFileWatcher::new(),
            UdpBroadcaster::new().await,
            TcpTransporter::new().await,
        )
    }
}
