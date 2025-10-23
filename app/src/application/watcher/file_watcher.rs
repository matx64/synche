use crate::{
    application::{
        EntryManager,
        persistence::interface::PersistenceInterface,
        watcher::{buffer::WatcherBuffer, interface::FileWatcherInterface},
    },
    domain::{
        AppState, EntryInfo, EntryKind, TransportChannelData, WatcherEvent, WatcherEventKind,
        WatcherEventPath,
    },
    utils::fs::compute_hash,
};
use std::{io, sync::Arc};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tracing::{error, info};

pub struct FileWatcher<T: FileWatcherInterface, P: PersistenceInterface> {
    adapter: T,
    state: Arc<AppState>,
    buffer: WatcherBuffer,
    watch_rx: Receiver<WatcherEvent>,
    entry_manager: Arc<EntryManager<P>>,
    sender_tx: Sender<TransportChannelData>,
}

impl<T: FileWatcherInterface, P: PersistenceInterface> FileWatcher<T, P> {
    pub fn new(
        adapter: T,
        state: Arc<AppState>,
        entry_manager: Arc<EntryManager<P>>,
        sender_tx: Sender<TransportChannelData>,
    ) -> Self {
        let (watch_tx, watch_rx) = mpsc::channel(1000);

        Self {
            state,
            adapter,
            watch_rx,
            sender_tx,
            entry_manager,
            buffer: WatcherBuffer::new(watch_tx),
        }
    }

    pub async fn run(&mut self) -> io::Result<()> {
        self.adapter.watch().await?;

        loop {
            tokio::select! {
                Some(event) = self.adapter.next() => {
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
        match self.entry_manager.get_entry(&path.relative).await {
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
        let disk_hash = Some(compute_hash(&path.canonical).await.unwrap());

        let file = self
            .entry_manager
            .entry_created(&path.relative, EntryKind::File, disk_hash)
            .await;

        self.send_metadata(file).await;

        if path.relative.ends_with(".gitignore") {
            self.entry_manager.insert_gitignore(&path.canonical).await;
        }
    }

    async fn handle_create_dir(&self, path: WatcherEventPath) {
        let dir_entries = self.entry_manager.build_dir(path.canonical).await.unwrap();

        for (relative, info) in dir_entries {
            self.entry_manager
                .entry_created(&relative, info.kind.clone(), info.hash.clone())
                .await;
            self.send_metadata(info).await;
        }
    }

    async fn handle_modify_file(&self, path: WatcherEventPath, file: EntryInfo) {
        let disk_hash = Some(compute_hash(&path.canonical).await.unwrap());

        if file.hash != disk_hash {
            let file = self.entry_manager.entry_modified(file, disk_hash).await;
            self.send_metadata(file).await;

            if path.relative.ends_with(".gitignore") {
                self.entry_manager.insert_gitignore(&path.canonical).await;
            }
        }
    }

    async fn handle_remove(&self, path: WatcherEventPath) {
        if let Some(removed) = self.entry_manager.remove_entry(&path.relative).await {
            if !removed.is_file() {
                let removed_entries = self.entry_manager.remove_dir(&path.relative).await;

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
        if let Err(err) = self
            .sender_tx
            .send(TransportChannelData::Metadata(file))
            .await
        {
            error!("Failed to buffer metadata {}", err);
        }
    }
}
