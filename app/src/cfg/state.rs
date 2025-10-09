use crate::{
    application::{
        EntryManager,
        network::{
            TransportInterface,
            presence::PresenceService,
            transport::{TransportReceiver, TransportSender},
        },
        persistence::interface::PersistenceInterface,
        watcher::{FileWatcher, FileWatcherInterface},
    },
    cfg::config::Config,
    domain::CanonicalPath,
    infra::{
        network::tcp::TcpTransporter, persistence::sqlite::SqliteDb,
        watcher::notify::NotifyFileWatcher,
    },
};
use std::{io, sync::Arc};
use uuid::Uuid;

pub struct AppState<W: FileWatcherInterface, T: TransportInterface, D: PersistenceInterface> {
    pub local_id: Uuid,
    pub paths: AppStatePaths,
    pub entry_manager: Arc<EntryManager<D>>,
    pub file_watcher: FileWatcher<W, D>,
    pub presence_service: PresenceService,
    pub transport_sender: TransportSender<T, D>,
    pub transport_receiver: TransportReceiver<T, D>,
}

pub struct AppStatePaths {
    pub base_dir_path: CanonicalPath,
    pub tmp_dir_path: CanonicalPath,
    pub cfg_dir_path: CanonicalPath,
    pub cfg_file_path: CanonicalPath,
}

impl<W: FileWatcherInterface, T: TransportInterface, D: PersistenceInterface> AppState<W, T, D> {
    pub fn new(
        cfg: Config,
        watch_adapter: W,
        transport_adapter: T,
        persistence_adapter: D,
    ) -> Self {
        let (local_id, sync_dirs) = cfg.init();
        let paths = Self::create_required_paths(&cfg).unwrap();
        todo!()
    }

    fn create_required_paths(cfg: &Config) -> io::Result<AppStatePaths> {
        let base_dir_path = CanonicalPath::new(cfg.base_dir_path)?;
        let tmp_dir_path = CanonicalPath::new(cfg.tmp_dir_path)?;
        let cfg_dir_path = CanonicalPath::new(cfg.cfg_dir_path)?;
        let cfg_file_path = CanonicalPath::from_canonical(&cfg_dir_path).join(cfg.cfg_file);

        Ok(AppStatePaths {
            base_dir_path,
            tmp_dir_path,
            cfg_dir_path,
            cfg_file_path,
        })
    }
}

impl AppState<NotifyFileWatcher, TcpTransporter, SqliteDb> {
    pub fn new_default(cfg: Config) -> Self {
        todo!()
    }
}
