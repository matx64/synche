use crate::{
    application::{
        EntryManager, persistence::interface::PersistenceInterface, watcher::FileWatcherInterface,
    },
    domain::{
        FileInfo,
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
    watch_tx: Sender<FileInfo>,
    base_dir: PathBuf,
    base_dir_absolute: PathBuf,
}

impl<T: FileWatcherInterface, D: PersistenceInterface> FileWatcher<T, D> {
    pub fn new(
        watch_adapter: T,
        entry_manager: Arc<EntryManager<D>>,
        watch_tx: Sender<FileInfo>,
        base_dir: PathBuf,
    ) -> Self {
        Self {
            watch_adapter,
            entry_manager,
            watch_tx,
            base_dir_absolute: base_dir.canonicalize().unwrap(),
            base_dir,
        }
    }

    pub async fn run(&mut self) -> io::Result<()> {
        let dirs = self
            .entry_manager
            .list_dirs()
            .iter()
            .map(|dir| self.base_dir.join(dir))
            .collect();

        self.watch_adapter.watch(dirs).await?;

        loop {
            if let Some(event) = self.watch_adapter.next().await {
                info!("File Change Event: {event:?}");
                match event {
                    WatcherEvent::CreatedFile(path) => {
                        self.handle_created_file(path).await;
                    }
                    WatcherEvent::ModifiedContent(path) => {
                        self.handle_modified_content(path).await;
                    }
                    WatcherEvent::ModifiedFileName(paths) => {
                        self.handle_modified_file_name(paths).await;
                    }
                    WatcherEvent::ModifiedDirName(paths) => {
                        self.handle_modified_dir_name(paths).await;
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

        if self.entry_manager.get_file(&relative_path).is_some() {
            return;
        }

        let disk_hash = match compute_hash(&path) {
            Ok(hash) => hash,
            Err(err) => {
                error!("Failed to compute {} hash: {}", relative_path, err);
                return;
            }
        };

        let file = self.entry_manager.file_created(&relative_path, disk_hash);

        self.send_metadata(file).await;
    }

    async fn handle_modified_content(&self, path: PathBuf) {
        let Ok(relative_path) = get_relative_path(&path, &self.base_dir_absolute) else {
            return;
        };

        let Some(file) = self.entry_manager.get_file(&relative_path) else {
            return;
        };

        let disk_hash = match compute_hash(&path) {
            Ok(hash) => hash,
            Err(err) => {
                error!("Failed to compute {} hash: {}", relative_path, err);
                return;
            }
        };

        if file.hash != disk_hash {
            if let Some(file) = self.entry_manager.file_modified(&relative_path, disk_hash) {
                self.send_metadata(file).await;
            }
        }
    }

    async fn handle_modified_file_name(&self, paths: ModifiedNamePaths) {
        self.handle_removed_file(paths.from).await;
        self.handle_created_file(paths.to).await;
    }

    async fn handle_modified_dir_name(&self, paths: ModifiedNamePaths) {
        let Ok(removed_relative_path) = get_relative_path(&paths.from, &self.base_dir_absolute)
        else {
            return;
        };
        let Ok(created_relative_path) = get_relative_path(&paths.to, &self.base_dir_absolute)
        else {
            return;
        };

        let removed_files = self.entry_manager.remove_dir(&removed_relative_path);

        for file in removed_files {
            let new_name = file
                .name
                .replace(&removed_relative_path, &created_relative_path);

            let new_path = PathBuf::new().join(&self.base_dir_absolute).join(new_name);

            if new_path.exists() {
                self.handle_created_file(new_path).await;
            }

            self.send_metadata(file).await;
        }
    }

    async fn handle_removed(&self, path: PathBuf) {
        let Ok(relative_path) = get_relative_path(&path, &self.base_dir_absolute) else {
            return;
        };

        if self.entry_manager.get_file(&relative_path).is_some() {
            if let Some(removed) = self.entry_manager.remove_file(&relative_path) {
                self.send_metadata(removed).await;
            }
        } else {
            let removed_files = self.entry_manager.remove_dir(&relative_path);

            for file in removed_files {
                self.send_metadata(file).await;
            }
        }
    }

    async fn handle_removed_file(&self, path: PathBuf) {
        let Ok(relative_path) = get_relative_path(&path, &self.base_dir_absolute) else {
            return;
        };

        if let Some(removed) = self.entry_manager.remove_file(&relative_path) {
            self.send_metadata(removed).await;
        }
    }

    async fn send_metadata(&self, file: FileInfo) {
        if let Err(err) = self.watch_tx.send(file).await {
            error!("Failed to buffer metadata {}", err);
        }
    }
}
