use crate::{
    application::{
        EntryManager, HttpService, PeerManager,
        network::{
            presence::{interface::PresenceInterface, service::PresenceService},
            transport::{TransportService, interface::TransportInterface},
        },
        persistence::interface::PersistenceInterface,
        watcher::{FileWatcher, interface::FileWatcherInterface},
    },
    domain::AppState,
    infra::{
        self,
        network::{mdns::MdnsAdapter, tcp::TcpAdapter},
        persistence::sqlite::SqliteDb,
        watcher::notify::NotifyFileWatcher,
    },
    utils::fs::get_os_config_dir,
};
use std::sync::Arc;
use tokio::io;

pub struct Synchronizer<
    W: FileWatcherInterface,
    T: TransportInterface,
    P: PersistenceInterface,
    R: PresenceInterface,
> {
    state: Arc<AppState>,
    file_watcher: FileWatcher<W, P>,
    http_service: Arc<HttpService<P>>,
    presence_service: PresenceService<R>,
    transport_service: TransportService<T, P>,
}

impl Synchronizer<NotifyFileWatcher, TcpAdapter, SqliteDb, MdnsAdapter> {
    pub async fn new_default() -> Self {
        let state = AppState::new().await;

        let notify = NotifyFileWatcher::new(state.clone());
        let mdns_adapter = MdnsAdapter::new(state.clone());
        let tcp_adapter = TcpAdapter::new(state.clone()).await;
        let sqlite_adapter = SqliteDb::new(get_os_config_dir().await.unwrap().join("db.db"))
            .await
            .unwrap();

        Self::new(state, notify, mdns_adapter, tcp_adapter, sqlite_adapter).await
    }
}

impl<W: FileWatcherInterface, T: TransportInterface, P: PersistenceInterface, D: PresenceInterface>
    Synchronizer<W, T, P, D>
{
    pub async fn new(
        state: Arc<AppState>,
        watch_adapter: W,
        presence_adapter: D,
        transport_adapter: T,
        persistence_adapter: P,
    ) -> Self {
        let entry_manager = EntryManager::new(persistence_adapter, state.clone());
        entry_manager.init().await.unwrap();

        let peer_manager = PeerManager::new();

        let (transport_service, sender_tx) = TransportService::new(
            transport_adapter,
            state.clone(),
            peer_manager.clone(),
            entry_manager.clone(),
        );

        let file_watcher = FileWatcher::new(
            watch_adapter,
            state.clone(),
            entry_manager.clone(),
            sender_tx.clone(),
        );

        let presence_service = PresenceService::new(
            presence_adapter,
            state.clone(),
            peer_manager.clone(),
            sender_tx.clone(),
        );

        let http_service = HttpService::new(state.clone(), peer_manager, entry_manager, sender_tx);

        Self {
            state,
            file_watcher,
            http_service,
            presence_service,
            transport_service,
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
            infra::http::server::run(self.state.ports.http, self.http_service.clone()),
            self.transport_service.run(),
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
