use crate::{
    application::{
        EntryManager, persistence::interface::PersistenceInterface, watcher::FileWatcherInterface,
    },
    domain::{
        EntryInfo, EntryKind,
        watcher::{WatcherEvent, WatcherEventPath},
    },
    utils::fs::compute_hash,
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
                info!("{event:?}");
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

    async fn handle_created_file(&self, path: WatcherEventPath) {
        if self.entry_manager.get_entry(&path.relative).is_some() {
            return self.handle_modified_file_content(path).await;
        }

        let disk_hash = match compute_hash(&path.absolute) {
            Ok(hash) => Some(hash),
            Err(err) => {
                error!("Failed to compute {} hash: {}", path.relative, err);
                return;
            }
        };

        let file = self
            .entry_manager
            .entry_created(&path.relative, EntryKind::File, disk_hash);

        self.send_metadata(file).await;
    }

    async fn handle_created_dir(&self, path: WatcherEventPath) {
        if self.entry_manager.get_entry(&path.relative).is_some() {
            return;
        }

        let dir = self
            .entry_manager
            .entry_created(&path.relative, EntryKind::Directory, None);

        self.send_metadata(dir).await;
    }

    async fn handle_modified_file_content(&self, path: WatcherEventPath) {
        let Some(file) = self.entry_manager.get_entry(&path.relative) else {
            return;
        };

        let disk_hash = match compute_hash(&path.absolute) {
            Ok(hash) => Some(hash),
            Err(err) => {
                error!("Failed to compute {} hash: {}", path.relative, err);
                return;
            }
        };

        if file.hash != disk_hash {
            if let Some(file) = self.entry_manager.entry_modified(&path.relative, disk_hash) {
                self.send_metadata(file).await;
            }
        }
    }

    async fn handle_renamed_file(&self, paths: (WatcherEventPath, WatcherEventPath)) {
        self.handle_removed_file(paths.0).await;
        self.handle_created_file(paths.1).await;
    }

    async fn handle_renamed_dir(&self, paths: (WatcherEventPath, WatcherEventPath)) {
        let (from_path, to_path) = paths;

        let removed_entries = self.entry_manager.remove_dir(&from_path.relative);

        for entry in removed_entries {
            let relative = entry.name.replace(&from_path.relative, &to_path.relative);
            let absolute = PathBuf::new().join(&self.base_dir_absolute).join(&relative);

            if absolute.exists() {
                self.handle_created_file(WatcherEventPath { absolute, relative })
                    .await;
            }

            self.send_metadata(entry).await;
        }
    }

    async fn handle_renamed_sync_dir(&self, _paths: (WatcherEventPath, WatcherEventPath)) {}

    async fn handle_removed(&self, path: WatcherEventPath) {
        if let Some(entry) = self.entry_manager.get_entry(&path.relative) {
            if entry.is_file() {
                if let Some(removed) = self.entry_manager.remove_entry(&path.relative) {
                    self.send_metadata(removed).await;
                }
            } else {
                let removed_entries = self.entry_manager.remove_dir(&path.relative);

                for file in removed_entries {
                    self.send_metadata(file).await;
                }
            }
        }
    }

    async fn handle_removed_file(&self, path: WatcherEventPath) {
        if let Some(removed) = self.entry_manager.remove_entry(&path.relative) {
            self.send_metadata(removed).await;
        }
    }

    async fn send_metadata(&self, file: EntryInfo) {
        if let Err(err) = self.watch_tx.send(file).await {
            error!("Failed to buffer metadata {}", err);
        }
    }
}
