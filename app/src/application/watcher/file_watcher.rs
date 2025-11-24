use crate::{
    application::{
        EntryManager,
        persistence::interface::PersistenceInterface,
        watcher::{buffer::WatcherBuffer, interface::FileWatcherInterface},
    },
    domain::{
        AppState, ConfigWatcherEvent, EntryInfo, EntryKind, HomeWatcherEvent, TransportChannelData,
        WatcherEventPath,
    },
    utils::fs::compute_hash,
};
use std::sync::Arc;
use tokio::{io, sync::mpsc::Sender};
use tracing::{error, info, warn};

pub struct FileWatcher<T: FileWatcherInterface, P: PersistenceInterface> {
    adapter: T,
    buffer: WatcherBuffer,
    _state: Arc<AppState>,
    entry_manager: Arc<EntryManager<P>>,
    sender_tx: Sender<TransportChannelData>,
}

impl<T: FileWatcherInterface, P: PersistenceInterface> FileWatcher<T, P> {
    pub fn new(
        adapter: T,
        _state: Arc<AppState>,
        entry_manager: Arc<EntryManager<P>>,
        sender_tx: Sender<TransportChannelData>,
    ) -> Self {
        Self {
            _state,
            adapter,
            sender_tx,
            entry_manager,
            buffer: WatcherBuffer::default(),
        }
    }

    pub async fn run(&mut self) -> io::Result<()> {
        self.adapter.watch_home().await?;
        self.adapter.watch_config().await?;

        tokio::select! {
            res = self.buffer.run() => res,
            res = self.recv_buffer_events() => res,
            res = self.recv_adapter_home_events() => res,
            res = self.recv_adapter_config_events() => res,
        }
    }

    async fn recv_adapter_home_events(&self) -> io::Result<()> {
        while let Some(event) = self.adapter.next_home_event().await? {
            let path = event.path();
            if !self
                .entry_manager
                .is_ignored(&path.canonical, &path.relative)
                .await
            {
                self.buffer.insert_home_event(event).await;
            }
        }
        warn!("Watcher Adapter home channel closed");
        Ok(())
    }

    async fn recv_adapter_config_events(&self) -> io::Result<()> {
        while let Some(event) = self.adapter.next_config_event().await? {
            if matches!(event, ConfigWatcherEvent::Remove) {
                return Err(io::Error::other("config.toml removed or moved"));
            }
            self.buffer.insert_config_event(event).await;
        }
        warn!("Watcher Adapter config channel closed");
        Ok(())
    }

    async fn recv_buffer_events(&self) -> io::Result<()> {
        while let Some(event) = self.buffer.next_home_event().await {
            info!("📁  {event:?}");
            match event {
                HomeWatcherEvent::CreateOrModify(path) => {
                    self.handle_create_or_modify(path).await?;
                }
                HomeWatcherEvent::Remove(path) => {
                    self.handle_remove(path).await?;
                }
            }
        }
        warn!("Watcher Buffer channel closed");
        Ok(())
    }

    async fn handle_create_or_modify(&self, path: WatcherEventPath) -> io::Result<()> {
        match self.entry_manager.get_entry(&path.relative).await? {
            None => self.handle_create(path).await,

            Some(entry) if path.is_file() && entry.is_file() => {
                self.handle_modify_file(path, entry).await
            }

            _ => Ok(()),
        }
    }

    async fn handle_create(&self, path: WatcherEventPath) -> io::Result<()> {
        if path.is_file() {
            self.handle_create_file(path).await
        } else {
            self.handle_create_dir(path).await
        }
    }

    async fn handle_create_file(&self, path: WatcherEventPath) -> io::Result<()> {
        let disk_hash = Some(compute_hash(&path.canonical).await?);

        let file = self
            .entry_manager
            .entry_created(&path.relative, EntryKind::File, disk_hash)
            .await?;

        self.send_metadata(file).await;

        if path.relative.ends_with(".gitignore") {
            self.entry_manager.insert_gitignore(&path.canonical).await;
        }
        Ok(())
    }

    async fn handle_create_dir(&self, path: WatcherEventPath) -> io::Result<()> {
        let dir_entries = self.entry_manager.build_dir(path.canonical).await?;

        for (relative, info) in dir_entries {
            self.entry_manager
                .entry_created(&relative, info.kind.clone(), info.hash.clone())
                .await?;
            self.send_metadata(info).await;
        }
        Ok(())
    }

    async fn handle_modify_file(&self, path: WatcherEventPath, file: EntryInfo) -> io::Result<()> {
        let disk_hash = Some(compute_hash(&path.canonical).await?);

        if file.hash != disk_hash {
            let file = self.entry_manager.entry_modified(file, disk_hash).await?;
            self.send_metadata(file).await;

            if path.relative.ends_with(".gitignore") {
                self.entry_manager.insert_gitignore(&path.canonical).await;
            }
        }
        Ok(())
    }

    async fn handle_remove(&self, path: WatcherEventPath) -> io::Result<()> {
        if let Some(removed) = self.entry_manager.remove_entry(&path.relative).await? {
            if !removed.is_file() {
                let removed_entries = self.entry_manager.remove_dir(&path.relative).await?;

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
        Ok(())
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
