use crate::{
    application::{
        EntryManager, persistence::interface::PersistenceInterface, watcher::FileWatcherInterface,
    },
    domain::{
        EntryInfo, EntryKind,
        filesystem::{ModifiedNamePaths, WatcherEvent},
    },
    utils::fs::{compute_hash, get_relative_path},
};
use std::{io, path::PathBuf, sync::Arc};
use tokio::sync::mpsc::Sender;
use tracing::{error, info};

pub struct FileWatcher<T: FileWatcherInterface, D: PersistenceInterface> {
    watch_adapter: T,
    entry_manager: Arc<EntryManager<D>>,
    watch_tx: Sender<EntryInfo>,
    base_dir_absolute: PathBuf,
}

impl<T: FileWatcherInterface, D: PersistenceInterface> FileWatcher<T, D> {
    pub fn new(
        watch_adapter: T,
        entry_manager: Arc<EntryManager<D>>,
        watch_tx: Sender<EntryInfo>,
        base_dir: PathBuf,
    ) -> Self {
        Self {
            watch_adapter,
            entry_manager,
            watch_tx,
            base_dir_absolute: base_dir.canonicalize().unwrap(),
        }
    }

    pub async fn run(&mut self) -> io::Result<()> {
        let dirs = self
            .entry_manager
            .list_dirs()
            .keys()
            .map(|dir| self.base_dir_absolute.join(dir))
            .collect();

        self.watch_adapter
            .watch(self.base_dir_absolute.clone(), dirs)
            .await?;

        loop {
            if let Some(event) = self.watch_adapter.next().await {
                info!("File Change Event: {event:?}");
                match event {
                    WatcherEvent::CreatedFile(path) => {
                        self.handle_created_file(path).await;
                    }
                    WatcherEvent::CreatedDir(path) => {
                        self.handle_created_dir(path).await;
                    }
                    WatcherEvent::ModifiedFileContent(path) => {
                        self.handle_modified_file_content(path).await;
                    }
                    WatcherEvent::RenamedFile(paths) => {
                        self.handle_renamed_file(paths).await;
                    }
                    WatcherEvent::RenamedDir(paths) => {
                        self.handle_renamed_dir(paths).await;
                    }
                    WatcherEvent::RenamedSyncDir(paths) => {
                        self.handle_renamed_sync_dir(paths).await;
                    }
                    WatcherEvent::Removed(path) => {
                        self.handle_removed(path).await;
                    }
                }
            }
        }
    }

    async fn handle_created_file(&self, path: PathBuf) {
        let Ok(relative_path) = get_relative_path(&path, &self.base_dir_absolute) else {
            return;
        };

        if self.entry_manager.get_entry(&relative_path).is_some() {
            return;
        }

        let disk_hash = match compute_hash(&path) {
            Ok(hash) => Some(hash),
            Err(err) => {
                error!("Failed to compute {} hash: {}", relative_path, err);
                return;
            }
        };

        let file = self
            .entry_manager
            .entry_created(&relative_path, EntryKind::File, disk_hash);

        self.send_metadata(file).await;
    }

    async fn handle_created_dir(&self, path: PathBuf) {
        let Ok(relative_path) = get_relative_path(&path, &self.base_dir_absolute) else {
            return;
        };

        if self.entry_manager.get_entry(&relative_path).is_some() {
            return;
        }

        let dir = self
            .entry_manager
            .entry_created(&relative_path, EntryKind::Directory, None);

        self.send_metadata(dir).await;
    }

    async fn handle_modified_file_content(&self, path: PathBuf) {
        let Ok(relative_path) = get_relative_path(&path, &self.base_dir_absolute) else {
            return;
        };

        let Some(file) = self.entry_manager.get_entry(&relative_path) else {
            return;
        };

        let disk_hash = match compute_hash(&path) {
            Ok(hash) => Some(hash),
            Err(err) => {
                error!("Failed to compute {} hash: {}", relative_path, err);
                return;
            }
        };

        if file.hash != disk_hash {
            if let Some(file) = self.entry_manager.entry_modified(&relative_path, disk_hash) {
                self.send_metadata(file).await;
            }
        }
    }

    async fn handle_renamed_file(&self, paths: ModifiedNamePaths) {
        self.handle_removed_file(paths.from).await;
        self.handle_created_file(paths.to).await;
    }

    async fn handle_renamed_dir(&self, paths: ModifiedNamePaths) {
        let Ok(removed_relative_path) = get_relative_path(&paths.from, &self.base_dir_absolute)
        else {
            return;
        };
        let Ok(created_relative_path) = get_relative_path(&paths.to, &self.base_dir_absolute)
        else {
            return;
        };

        let removed_entries = self.entry_manager.remove_dir(&removed_relative_path);

        for entry in removed_entries {
            let new_name = entry
                .name
                .replace(&removed_relative_path, &created_relative_path);

            let new_path = PathBuf::new().join(&self.base_dir_absolute).join(new_name);

            if new_path.exists() {
                self.handle_created_file(new_path).await;
            }

            self.send_metadata(entry).await;
        }
    }

    async fn handle_renamed_sync_dir(&self, _paths: ModifiedNamePaths) {}

    async fn handle_removed(&self, path: PathBuf) {
        let Ok(relative_path) = get_relative_path(&path, &self.base_dir_absolute) else {
            return;
        };

        if let Some(entry) = self.entry_manager.get_entry(&relative_path) {
            if entry.is_file() {
                if let Some(removed) = self.entry_manager.remove_entry(&relative_path) {
                    self.send_metadata(removed).await;
                }
            } else {
                let removed_entries = self.entry_manager.remove_dir(&relative_path);

                for file in removed_entries {
                    self.send_metadata(file).await;
                }
            }
        }
    }

    async fn handle_removed_file(&self, path: PathBuf) {
        let Ok(relative_path) = get_relative_path(&path, &self.base_dir_absolute) else {
            return;
        };

        if let Some(removed) = self.entry_manager.remove_entry(&relative_path) {
            self.send_metadata(removed).await;
        }
    }

    async fn send_metadata(&self, file: EntryInfo) {
        if let Err(err) = self.watch_tx.send(file).await {
            error!("Failed to buffer metadata {}", err);
        }
    }
}
