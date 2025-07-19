use crate::{
    application::watcher::FileWatcherInterface,
    domain::{EntryManager, FileInfo, filesystem::FileChangeEvent},
    utils::fs::{compute_hash, get_relative_path},
};
use std::{io, path::PathBuf, sync::Arc};
use tokio::sync::mpsc::Sender;
use tracing::error;

pub struct FileWatcher<T: FileWatcherInterface> {
    watch_adapter: T,
    entry_manager: Arc<EntryManager>,
    meta_tx: Sender<FileInfo>,
    base_dir: PathBuf,
    base_dir_absolute: PathBuf,
}

impl<T: FileWatcherInterface> FileWatcher<T> {
    pub async fn new(
        watch_adapter: T,
        entry_manager: Arc<EntryManager>,
        meta_tx: Sender<FileInfo>,
        base_dir: PathBuf,
    ) -> Self {
        Self {
            watch_adapter,
            entry_manager,
            meta_tx,
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
                match event {
                    FileChangeEvent::Created(path) => {
                        self.handle_created(path).await;
                    }
                    FileChangeEvent::Modified(path) => {
                        self.handle_modified(path).await;
                    }
                    FileChangeEvent::Deleted(path) => {
                        self.handle_deleted(path).await;
                    }
                }
            }
        }
    }

    async fn handle_created(&self, path: PathBuf) {
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

        let file = FileInfo {
            name: relative_path.clone(),
            hash: disk_hash,
            version: 0,
            last_modified_by: None,
        };

        self.entry_manager.insert_file(file.clone());

        self.send_metadata(file).await;
    }

    async fn handle_modified(&self, path: PathBuf) {
        if path.is_dir() {
            return;
        }

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
            let file = FileInfo {
                name: relative_path.clone(),
                hash: disk_hash,
                version: file.version + 1,
                last_modified_by: None,
            };

            self.entry_manager.insert_file(file.clone());

            self.send_metadata(file).await;
        }
    }

    async fn handle_deleted(&self, path: PathBuf) {
        let Ok(relative_path) = get_relative_path(&path, &self.base_dir_absolute) else {
            return;
        };

        if path.exists() {
            return;
        }

        if self.entry_manager.is_dir(&relative_path) {
            let removed_files = self.entry_manager.remove_dir(&relative_path);

            for file in removed_files {
                self.send_metadata(file).await;
            }
        } else if self.entry_manager.get_file(&relative_path).is_some() {
            let removed = self.entry_manager.remove_file(&relative_path);
            self.send_metadata(removed).await;
        }
    }

    async fn send_metadata(&self, file: FileInfo) {
        if let Err(err) = self.meta_tx.send(file).await {
            error!("Failed to buffer metadata {}", err);
        }
    }
}
