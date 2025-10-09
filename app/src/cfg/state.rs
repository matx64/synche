use crate::{
    application::{EntryManager, persistence::interface::PersistenceInterface},
    cfg::config::Config,
    domain::{CanonicalPath, SyncDirectory},
    infra::persistence::sqlite::SqliteDb,
};
use std::{collections::HashMap, io, path::PathBuf, sync::Arc};
use uuid::Uuid;

pub struct AppState<D: PersistenceInterface> {
    pub local_id: Uuid,
    pub paths: AppStatePaths,
    pub entry_manager: Arc<EntryManager<D>>,
}

pub struct AppStatePaths {
    pub base_dir_path: CanonicalPath,
    pub tmp_dir_path: CanonicalPath,
    pub cfg_dir_path: CanonicalPath,
    pub cfg_file_path: CanonicalPath,
}

impl AppState<SqliteDb> {
    pub fn new_default(cfg: Config) -> Self {
        let db =
            SqliteDb::new(PathBuf::from(&cfg.cfg_dir_path).join(cfg.persistence_file)).unwrap();
        Self::new(cfg, db)
    }
}

impl<D: PersistenceInterface> AppState<D> {
    pub fn new(cfg: Config, persistence_adapter: D) -> Self {
        let (local_id, cfg_file_data) = cfg.init();
        let paths = Self::create_required_paths(&cfg).unwrap();

        let sync_dirs = cfg_file_data
            .sync_directories
            .iter()
            .map(|d| (d.name.clone(), d.to_owned()))
            .collect::<HashMap<String, SyncDirectory>>();

        let entry_manager = Arc::new(EntryManager::new(
            persistence_adapter,
            local_id,
            sync_dirs,
            paths.base_dir_path.clone(),
        ));

        Self {
            local_id,
            paths,
            entry_manager,
        }
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
