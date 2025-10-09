use crate::{
    application::{
        PeerManager,
        network::{
            TransportInterface,
            presence::PresenceService,
            transport::{TransportReceiver, TransportSender},
        },
        persistence::interface::PersistenceInterface,
        watcher::{FileWatcher, FileWatcherInterface},
    },
    cfg::AppState,
    infra::{
        network::tcp::TcpTransporter, persistence::sqlite::SqliteDb,
        watcher::notify::NotifyFileWatcher,
    },
};
use std::sync::Arc;
use tokio::io;

pub struct Synchronizer<W: FileWatcherInterface, T: TransportInterface, D: PersistenceInterface> {
    file_watcher: FileWatcher<W, D>,
    presence_service: PresenceService,
    transport_sender: TransportSender<T, D>,
    transport_receiver: TransportReceiver<T, D>,
}

impl Synchronizer<NotifyFileWatcher, TcpTransporter, SqliteDb> {
    pub async fn new_default(state: AppState<SqliteDb>) -> Self {
        let transporter = TcpTransporter::new(state.local_id).await;

        Self::new(state, NotifyFileWatcher::new(), transporter).await
    }
}

impl<W: FileWatcherInterface, T: TransportInterface, D: PersistenceInterface>
    Synchronizer<W, T, D>
{
    pub async fn new(state: AppState<D>, watch_adapter: W, transport_adapter: T) -> Self {
        state.entry_manager.init().await.unwrap();

        let peer_manager = Arc::new(PeerManager::new());
        let transport_adapter = Arc::new(transport_adapter);

        let (transport_sender, sender_channels) = TransportSender::new(
            transport_adapter.clone(),
            state.entry_manager.clone(),
            peer_manager.clone(),
            state.paths.base_dir_path.clone(),
        );

        let file_watcher = FileWatcher::new(
            watch_adapter,
            state.entry_manager.clone(),
            sender_channels.metadata_tx.clone(),
            state.paths.base_dir_path.clone(),
        );

        let presence_service = PresenceService::new(
            state.local_id,
            peer_manager.clone(),
            sender_channels.handshake_tx.clone(),
        );

        let transport_receiver = TransportReceiver::new(
            transport_adapter,
            state.entry_manager,
            peer_manager,
            sender_channels,
            state.paths.base_dir_path,
            state.paths.tmp_dir_path,
        );

        Self {
            file_watcher,
            presence_service,
            transport_sender,
            transport_receiver,
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
            self.transport_receiver.run(),
            self.transport_sender.run(),
            self.presence_service.run(),
            self.file_watcher.run(),
        )?;
        Ok(())
    }

    pub async fn shutdown(&mut self) -> io::Result<()> {
        self.presence_service.shutdown();
        tracing::info!("✅ Synche gracefully shutdown");
        Ok(())
    }
}
