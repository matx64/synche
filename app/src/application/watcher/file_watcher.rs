use crate::{
    application::{
        EntryManager,
        persistence::interface::PersistenceInterface,
        watcher::{FileWatcherInterface, buffer::WatcherBuffer},
    },
    domain::{
        CanonicalPath, EntryInfo, EntryKind, WatcherEvent, WatcherEventKind, WatcherEventPath,
    },
    utils::fs::compute_hash,
};
use std::{io, sync::Arc};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tracing::{error, info};

pub struct FileWatcher<T: FileWatcherInterface, D: PersistenceInterface> {
    watch_adapter: T,
    entry_manager: Arc<EntryManager<D>>,
    buffer: WatcherBuffer,
    watch_rx: Receiver<WatcherEvent>,
    metadata_tx: Sender<EntryInfo>,
    base_dir_path: CanonicalPath,
}

impl<T: FileWatcherInterface, D: PersistenceInterface> FileWatcher<T, D> {
    pub fn new(
        watch_adapter: T,
        entry_manager: Arc<EntryManager<D>>,
        metadata_tx: Sender<EntryInfo>,
        base_dir_path: CanonicalPath,
    ) -> Self {
        let (watch_tx, watch_rx) = mpsc::channel(1000);

        Self {
            watch_adapter,
            entry_manager,
            watch_rx,
            metadata_tx,
            base_dir_path,
            buffer: WatcherBuffer::new(watch_tx),
        }
    }

    pub async fn run(&mut self) -> io::Result<()> {
        self.set_watch_dirs().await?;

        loop {
            tokio::select! {
                Some(event) = self.watch_adapter.next() => {
                    if !self.entry_manager.is_ignored(&event.path.canonical, &event.path.relative).await {
                        self.buffer.insert(event).await;
                    }
                }

                Some(event) = self.watch_rx.recv() => {
                    info!("📁  {event:?}");
                    match event.kind {
                        WatcherEventKind::CreateOrModify => {
                            self.handle_create_or_modify(event.path).await;
                        }
                        WatcherEventKind::Remove => {
                            self.handle_remove(event.path).await;
                        }
                    }
                }
            }
        }
    }

    async fn set_watch_dirs(&mut self) -> io::Result<()> {
        let dirs = self
            .entry_manager
            .list_dirs()
            .await
            .keys()
            .map(|dir| self.base_dir_path.join(dir))
            .collect();

        self.watch_adapter
            .watch(self.base_dir_path.clone(), dirs)
            .await
    }

    fn add_sync_dir(&mut self, dir_path: CanonicalPath) {
        self.watch_adapter.add_sync_dir(dir_path);
    }

    async fn handle_create_or_modify(&self, path: WatcherEventPath) {
        match self.entry_manager.get_entry(&path.relative) {
            None => self.handle_create(path).await,

            Some(entry) if path.is_file() && entry.is_file() => {
                self.handle_modify_file(path, entry).await
            }

            _ => {}
        }
    }

    async fn handle_create(&self, path: WatcherEventPath) {
        if path.is_file() {
            self.handle_create_file(path).await;
        } else {
            self.handle_create_dir(path).await;
        }
    }

    async fn handle_create_file(&self, path: WatcherEventPath) {
        let disk_hash = Some(compute_hash(&path.canonical).unwrap());

        let file = self
            .entry_manager
            .entry_created(&path.relative, EntryKind::File, disk_hash);

        self.send_metadata(file).await;

        if path.relative.ends_with(".gitignore") {
            self.entry_manager.insert_gitignore(&path.canonical).await;
        }
    }

    async fn handle_create_dir(&self, path: WatcherEventPath) {
        let dir_entries = self.entry_manager.build_dir(path.canonical).await.unwrap();

        for (relative, info) in dir_entries {
            self.entry_manager
                .entry_created(&relative, info.kind.clone(), info.hash.clone());
            self.send_metadata(info).await;
        }
    }

    async fn handle_modify_file(&self, path: WatcherEventPath, file: EntryInfo) {
        let disk_hash = Some(compute_hash(&path.canonical).unwrap());

        if file.hash != disk_hash {
            let file = self.entry_manager.entry_modified(file, disk_hash);
            self.send_metadata(file).await;

            if path.relative.ends_with(".gitignore") {
                self.entry_manager.insert_gitignore(&path.canonical).await;
            }
        }
    }

    async fn handle_remove(&self, path: WatcherEventPath) {
        if let Some(removed) = self.entry_manager.remove_entry(&path.relative) {
            if !removed.is_file() {
                let removed_entries = self.entry_manager.remove_dir(&path.relative);

                for entry in removed_entries {
                    if entry.name.ends_with(".gitignore") {
                        self.entry_manager.remove_gitignore(&entry.name).await;
                    }

                    self.send_metadata(entry).await;
                }
            }

            if removed.name.ends_with(".gitignore") {
                self.entry_manager.remove_gitignore(&removed.name).await;
            }

            self.send_metadata(removed).await;
        }
    }

    async fn send_metadata(&self, file: EntryInfo) {
        if let Err(err) = self.metadata_tx.send(file).await {
            error!("Failed to buffer metadata {}", err);
        }
    }
}
