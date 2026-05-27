use crate::{
    application::AppState,
    application::{
        EntryManager, PeerManager,
        network::{
            presence::{interface::PresenceInterface, service::PresenceService},
            transport::{TransportService, interface::TransportInterface},
        },
        persistence::interface::PersistenceInterface,
        state::default_ports,
        watcher::{FileWatcher, interface::FileWatcherInterface},
    },
    domain::ServerEvent,
    infra::{
        self,
        network::{mdns::MdnsAdapter, tcp::TcpAdapter},
        persistence::sqlite::SqliteDb,
        watcher::notify::NotifyFileWatcher,
    },
    utils::dirs::SyncheDirs,
};
use std::sync::Arc;
use tokio::io;

/// Parses the `HOME_PATH_CHANGED:<old>:<new>` sentinel emitted by the
/// HTTP layer when the user changes `home_path` through the GUI.
///
/// Uses `strip_prefix` + `split_once` so a colon inside `new` (e.g.
/// a Windows drive letter like `C:\Users\...`) does not corrupt
/// parsing. Returns `None` for any other error.
fn parse_home_path_changed_sentinel(err: &io::Error) -> Option<(String, String)> {
    let msg = err.to_string();
    let rest = msg.strip_prefix("HOME_PATH_CHANGED:")?;
    let (old, new) = rest.split_once(':')?;
    Some((old.to_string(), new.to_string()))
}

/// Top-level orchestrator that wires the application's four concurrent
/// subsystems — transport, presence, file watcher, and HTTP server —
/// around a shared `AppState`.
///
/// Generic over each port so tests can inject in-memory adapters; the
/// production wiring is `Synchronizer<NotifyFileWatcher, TcpAdapter,
/// SqliteDb, MdnsAdapter>` (see `new_default_with_dirs`).
pub struct Synchronizer<
    W: FileWatcherInterface,
    T: TransportInterface,
    P: PersistenceInterface,
    R: PresenceInterface,
> {
    state: Arc<AppState>,
    file_watcher: FileWatcher<W, P>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
    presence_service: PresenceService<R>,
    transport_service: TransportService<T, P>,
}

impl Synchronizer<NotifyFileWatcher, TcpAdapter, SqliteDb, MdnsAdapter> {
    /// Builds a `Synchronizer` wired with the production adapters and
    /// the supplied `SyncheDirs` (so the binary uses OS dirs and tests
    /// can inject isolated temporary ones).
    pub async fn new_default_with_dirs(dirs: SyncheDirs) -> Self {
        let state = AppState::new(dirs, default_ports()).await;

        let notify = NotifyFileWatcher::new(state.clone());
        let mdns_adapter = MdnsAdapter::new(state.clone());
        let tcp_adapter = TcpAdapter::new(state.clone()).await;
        let sqlite_adapter = SqliteDb::new(state.dirs().data_db_file()).await.unwrap();

        Self::new(state, notify, mdns_adapter, tcp_adapter, sqlite_adapter).await
    }

    /// Runs the synchronizer in a loop, rebuilding the entire
    /// `Synchronizer` whenever `run` returns the sentinel
    /// `HOME_PATH_CHANGED:<old>:<new>` error so an in-flight
    /// `home_path` change from the GUI is applied without restarting
    /// the process. Any other error propagates and exits the loop.
    ///
    /// Anything touching shutdown or restart paths must preserve this
    /// sentinel contract.
    pub async fn run_default_with_restart(dirs: SyncheDirs) -> io::Result<()> {
        loop {
            let mut synchronizer = Self::new_default_with_dirs(dirs.clone()).await;

            match synchronizer.run().await {
                Ok(()) => break,
                Err(e) => match parse_home_path_changed_sentinel(&e) {
                    Some((old_path, new_path)) => {
                        tracing::info!(
                            "home_path changed from {} to {}. Restarting synchronizer...",
                            old_path,
                            new_path
                        );
                        continue;
                    }
                    None => return Err(e),
                },
            }
        }

        Ok(())
    }
}

impl<W: FileWatcherInterface, T: TransportInterface, P: PersistenceInterface, D: PresenceInterface>
    Synchronizer<W, T, P, D>
{
    /// Wires the synchronizer with explicit adapters for every port —
    /// the seam tests use to inject in-memory or fake implementations.
    pub async fn new(
        state: Arc<AppState>,
        watch_adapter: W,
        presence_adapter: D,
        transport_adapter: T,
        persistence_adapter: P,
    ) -> Self {
        let entry_manager = EntryManager::new(persistence_adapter, state.clone());
        entry_manager.init().await.unwrap();

        let peer_manager = PeerManager::new(state.clone());

        let (transport_service, sender_tx) = TransportService::new(
            transport_adapter,
            state.clone(),
            peer_manager.clone(),
            entry_manager.clone(),
        );

        let file_watcher = FileWatcher::new(
            watch_adapter,
            state.clone(),
            peer_manager.clone(),
            entry_manager.clone(),
            sender_tx.clone(),
        );

        let presence_service = PresenceService::new(
            presence_adapter,
            state.clone(),
            peer_manager.clone(),
            sender_tx,
        );

        Self {
            state,
            file_watcher,
            peer_manager,
            entry_manager,
            presence_service,
            transport_service,
        }
    }

    /// Runs the four subsystems concurrently until any one exits or a
    /// shutdown signal arrives (`SIGINT`/`SIGTERM`/`SIGHUP` on Unix,
    /// `Ctrl+C` elsewhere). Returns the `HOME_PATH_CHANGED:` sentinel
    /// untouched so `run_default_with_restart` can rebuild.
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
                res = self._run() => {
                    if let Err(e) = res {
                        if parse_home_path_changed_sentinel(&e).is_some() {
                            let _ = self.state.sse_sender().send(ServerEvent::ServerRestart);

                            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                            self.shutdown().await?;
                            return Err(e);
                        }
                        return Err(e);
                    }
                }

                _ = ctrl_c => {
                    tracing::info!("received SIGINT, shutting down"); self.shutdown().await?;
                }

                _ = sigterm.recv() => {
                    tracing::info!("received SIGTERM, shutting down"); self.shutdown().await?;
                }

                _ = sighup.recv() => {
                    tracing::info!("received SIGHUP, shutting down"); self.shutdown().await?;
                }
            }
        }

        #[cfg(not(unix))]
        {
            use tokio::signal;

            let ctrl_c = signal::ctrl_c();

            tokio::select! {
                res = self._run() => {
                    if let Err(e) = res {
                        if parse_home_path_changed_sentinel(&e).is_some() {
                            let _ = self.state.sse_sender().send(ServerEvent::ServerRestart);

                            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                            self.shutdown().await?;
                            return Err(e);
                        }
                        return Err(e);
                    }
                }

                _ = ctrl_c => {
                    tracing::info!("received SIGINT, shutting down"); self.shutdown().await?;
                }
            }
        }
        Ok(())
    }

    #[tracing::instrument(
        name = "synche",
        skip_all,
        fields(
            device = %self.state.local_id(),
            instance = %self.state.instance_id(),
            version = env!("CARGO_PKG_VERSION"),
        ),
    )]
    async fn _run(&mut self) -> io::Result<()> {
        tokio::select!(
            res = self.transport_service.run() => res,
            res = self.presence_service.run() => res,
            res = self.file_watcher.run() => res,
            res = infra::http::run(
                self.state.clone(),
                self.peer_manager.clone(),
                self.entry_manager.clone(),
            ) => res,
        )
    }

    /// Cleanly stops background services (currently presence).
    pub async fn shutdown(&mut self) -> io::Result<()> {
        self.presence_service.shutdown().await;
        tracing::info!("Synche gracefully shutdown");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_home_path_changed_sentinel_extracts_old_and_new_paths() {
        let err = io::Error::other("HOME_PATH_CHANGED:/old/home:/new/home");
        let parsed = parse_home_path_changed_sentinel(&err);
        assert_eq!(
            parsed,
            Some(("/old/home".to_string(), "/new/home".to_string()))
        );
    }

    #[test]
    fn parse_home_path_changed_sentinel_returns_none_for_unrelated_error() {
        let err = io::Error::other("something else went wrong");
        assert_eq!(parse_home_path_changed_sentinel(&err), None);
    }

    #[test]
    fn parse_home_path_changed_sentinel_returns_none_when_separator_missing() {
        let err = io::Error::other("HOME_PATH_CHANGED:/onlyone");
        assert_eq!(parse_home_path_changed_sentinel(&err), None);
    }

    /// Regression test for the bug the refactor fixes: a colon in the
    /// new path (e.g. a Windows drive letter) must not corrupt parsing.
    #[test]
    fn parse_home_path_changed_sentinel_preserves_colon_in_new_path() {
        let err = io::Error::other("HOME_PATH_CHANGED:/old:C:\\Users\\new");
        let parsed = parse_home_path_changed_sentinel(&err);
        assert_eq!(
            parsed,
            Some(("/old".to_string(), "C:\\Users\\new".to_string()))
        );
    }
}
