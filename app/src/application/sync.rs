use crate::{
    application::{
        AppState, EntryManager, HttpService, PeerManager,
        network::{
            TransportInterface,
            presence::{PresenceService, interface::PresenceInterface},
            transport::{TransportReceiver, TransportSender},
        },
        persistence::interface::PersistenceInterface,
        watcher::{FileWatcher, interface::FileWatcherInterface},
    },
    infra::{
        self,
        network::{mdns::MdnsAdapter, tcp::TcpTransporter},
        persistence::sqlite::SqliteDb,
        watcher::notify::NotifyFileWatcher,
    },
};
use std::sync::Arc;
use tokio::io;

pub struct Synchronizer<
    W: FileWatcherInterface,
    T: TransportInterface,
    P: PersistenceInterface,
    R: PresenceInterface,
> {
    file_watcher: FileWatcher<W, P>,
    presence_service: PresenceService<R>,
    transport_sender: TransportSender<T, P>,
    transport_receiver: TransportReceiver<T, P>,
    http_service: Arc<HttpService<P>>,
}

impl Synchronizer<NotifyFileWatcher, TcpTransporter, SqliteDb, MdnsAdapter> {
    pub async fn new_default() -> Self {
        let state = AppState::new();

        let notify = NotifyFileWatcher::new();
        let mdns_adapter = MdnsAdapter::new(state.local_id);
        let tcp_transporter = TcpTransporter::new(state.local_id).await;
        let sqlite_adapter = SqliteDb::new(state.cfg_path.join("db.db")).await.unwrap();

        Self::new(state, notify, mdns_adapter, tcp_transporter, sqlite_adapter).await
    }
}

impl<W: FileWatcherInterface, T: TransportInterface, P: PersistenceInterface, D: PresenceInterface>
    Synchronizer<W, T, P, D>
{
    pub async fn new(
        state: AppState,
        watch_adapter: W,
        presence_adapter: D,
        transport_adapter: T,
        persistence_adapter: P,
    ) -> Self {
        let dirs = { state.sync_dirs.read().unwrap().clone() };

        let entry_manager = Arc::new(EntryManager::new(
            persistence_adapter,
            state.local_id,
            dirs,
            state.home_path.clone(),
        ));
        entry_manager.init().await.unwrap();

        let peer_manager = Arc::new(PeerManager::new());
        let transport_adapter = Arc::new(transport_adapter);

        let (transport_sender, sender_channels) = TransportSender::new(
            transport_adapter.clone(),
            entry_manager.clone(),
            peer_manager.clone(),
            state.home_path.clone(),
        );

        let (file_watcher, dirs_updates_tx) = FileWatcher::new(
            watch_adapter,
            entry_manager.clone(),
            sender_channels.metadata_tx.clone(),
            state.home_path.clone(),
        );

        let presence_service = PresenceService::new(
            presence_adapter,
            state.local_id,
            peer_manager.clone(),
            sender_channels.handshake_tx.clone(),
        );

        let http_service = HttpService::new(
            state.local_id,
            entry_manager.clone(),
            peer_manager.clone(),
            dirs_updates_tx,
            sender_channels.handshake_tx.clone(),
        );

        let transport_receiver = TransportReceiver::new(
            transport_adapter,
            entry_manager,
            peer_manager,
            sender_channels,
            state.home_path,
        );

        Self {
            file_watcher,
            presence_service,
            transport_sender,
            transport_receiver,
            http_service,
        }
    }

    pub async fn run(&mut self) -> io::Result<()> {
        #[cfg(unix)]
        {
            use tokio::signal::{
                self,
                unix::{SignalKind, signal},
            };
            let ctrl_c = signal::ctrl_c();
            let mut sigterm = signal(SignalKind::terminate()).expect("bind SIGTERM");
            let mut sighup = signal(SignalKind::hangup()).expect("bind SIGHUP");

            tokio::select! {
                res = self._run() => res?,

                _ = ctrl_c => {
                    tracing::info!("🛑 SIGINT"); self.shutdown().await?;
                }

                _ = sigterm.recv() => {
                    tracing::info!("🛑 SIGTERM"); self.shutdown().await?;
                }

                _ = sighup.recv() => {
                    tracing::info!("🛑 SIGHUP"); self.shutdown().await?;
                }
            }
        }

        #[cfg(not(unix))]
        {
            use tokio::signal;

            let ctrl_c = signal::ctrl_c();

            tokio::select! {
                res = self._run() => res?,

                _ = ctrl_c => {
                    tracing::info!("🛑 SIGINT"); self.shutdown().await?;
                }
            }
        }
        Ok(())
    }

    async fn _run(&mut self) -> io::Result<()> {
        tokio::try_join!(
            infra::http::server::run(self.http_service.clone()),
            self.transport_receiver.run(),
            self.transport_sender.run(),
            self.presence_service.run(),
            self.file_watcher.run(),
        )?;
        Ok(())
    }

    pub async fn shutdown(&mut self) -> io::Result<()> {
        self.presence_service.shutdown().await;
        tracing::info!("✅ Synche gracefully shutdown");
        Ok(())
    }
}
