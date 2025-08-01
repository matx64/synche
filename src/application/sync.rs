use crate::{
    application::{
        EntryManager, PeerManager,
        network::{
            PresenceInterface, TransportInterface,
            presence::PresenceService,
            transport::{TransportReceiver, TransportSender},
        },
        persistence::interface::PersistenceInterface,
        watcher::{FileWatcher, FileWatcherInterface},
    },
    config::Config,
    infra::{
        network::{tcp::TcpTransporter, udp::UdpBroadcaster},
        persistence::sqlite::SqliteDb,
        watcher::notify::NotifyFileWatcher,
    },
};
use std::sync::Arc;
use tokio::{io, sync};

pub struct Synchronizer<
    W: FileWatcherInterface,
    P: PresenceInterface,
    T: TransportInterface,
    D: PersistenceInterface,
> {
    file_watcher: FileWatcher<W, D>,
    presence_service: PresenceService<P>,
    transport_sender: TransportSender<T, D>,
    transport_receiver: TransportReceiver<T, D>,
    _shutdown_tx: sync::watch::Sender<bool>,
    _shutdown_rx: sync::watch::Receiver<bool>,
}

impl<W: FileWatcherInterface, P: PresenceInterface, T: TransportInterface, D: PersistenceInterface>
    Synchronizer<W, P, T, D>
{
    pub fn new(
        config: Config,
        watch_adapter: W,
        presence_adapter: P,
        transport_adapter: T,
        persistence_adapter: D,
    ) -> Self {
        let entry_manager = Arc::new(EntryManager::new(
            persistence_adapter,
            config.constants.local_id,
            config.directories,
            config.filesystem_entries,
        ));
        let peer_manager = Arc::new(PeerManager::new());
        let transport_adapter = Arc::new(transport_adapter);

        let (transport_sender, sender_channels) = TransportSender::new(
            transport_adapter.clone(),
            entry_manager.clone(),
            peer_manager.clone(),
            config.constants.base_dir.clone(),
        );

        let file_watcher = FileWatcher::new(
            watch_adapter,
            entry_manager.clone(),
            sender_channels.watch_tx.clone(),
            config.constants.base_dir.clone(),
        );

        let presence_service = PresenceService::new(
            presence_adapter,
            config.constants.local_id,
            peer_manager.clone(),
            sender_channels.handshake_tx.clone(),
            config.constants.broadcast_interval_secs,
        );

        let transport_receiver = TransportReceiver::new(
            transport_adapter,
            entry_manager,
            peer_manager,
            sender_channels,
            config.constants.base_dir,
            config.constants.tmp_dir,
        );

        let (_shutdown_tx, _shutdown_rx) = sync::watch::channel(false);

        Self {
            file_watcher,
            presence_service,
            transport_sender,
            transport_receiver,
            _shutdown_tx,
            _shutdown_rx,
        }
    }

    pub async fn run(&mut self) -> io::Result<()> {
        tokio::try_join!(
            self.transport_receiver.run(),
            self.transport_sender.run(),
            self.presence_service.run(),
            self.file_watcher.run(),
        )?;
        Ok(())
    }

    pub async fn shutdown(&mut self) -> io::Result<()> {
        self.presence_service.shutdown().await
    }
}

impl Synchronizer<NotifyFileWatcher, UdpBroadcaster, TcpTransporter, SqliteDb> {
    pub async fn new_default(config: Config) -> Self {
        let transporter = TcpTransporter::new(config.constants.local_id).await;
        Self::new(
            config,
            NotifyFileWatcher::new(),
            UdpBroadcaster::new().await,
            transporter,
            SqliteDb::new(".synche/db.db").unwrap(),
        )
    }
}
