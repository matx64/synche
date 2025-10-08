use crate::{
    application::{
        EntryManager,
        persistence::interface::PersistenceInterface,
        watcher::{FileWatcherInterface, buffer::WatcherBuffer},
    },
    domain::{
        CanonicalPath, EntryInfo, EntryKind, RelativePath,
        watcher::{WatcherEvent, WatcherEventKind, WatcherEventPath},
    },
    utils::fs::{compute_hash, is_ds_store},
};
use std::{collections::HashSet, io, sync::Arc};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tracing::{error, info};
use walkdir::WalkDir;

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
        let dirs = self
            .entry_manager
            .list_dirs()
            .await
            .keys()
            .map(|dir| self.base_dir_path.join(dir))
            .collect();

        self.watch_adapter
            .watch(self.base_dir_path.clone(), dirs)
            .await?;

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
        let mut stack = vec![path];
        let mut visited = HashSet::new();

        while let Some(dir_path) = stack.pop() {
            if !visited.insert(dir_path.relative.clone()) {
                continue;
            }

            if self.entry_manager.get_entry(&dir_path.relative).is_some() {
                continue;
            }

            let dir =
                self.entry_manager
                    .entry_created(&dir_path.relative, EntryKind::Directory, None);

            self.send_metadata(dir).await;

            let gitignore_path = dir_path.canonical.join(".gitignore");
            if gitignore_path.exists() {
                self.entry_manager.insert_gitignore(&gitignore_path).await;
            }

            for item in WalkDir::new(&dir_path.canonical)
                .min_depth(1)
                .max_depth(1)
                .into_iter()
                .filter_map(Result::ok)
            {
                let canonical = CanonicalPath::new(item.path()).unwrap();
                let relative = RelativePath::new(&canonical, &self.base_dir_path);

                if is_ds_store(&canonical)
                    || self.entry_manager.is_ignored(&canonical, &relative).await
                {
                    continue;
                }

                if canonical.is_file() {
                    self.handle_create_file(WatcherEventPath {
                        canonical,
                        relative,
                    })
                    .await;
                } else if canonical.is_dir() {
                    stack.push(WatcherEventPath {
                        canonical,
                        relative,
                    });
                }
            }
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
