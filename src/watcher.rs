use crate::{
    config::AppState,
    domain::file::FileInfo,
    services::file::FileService,
    utils::fs::{compute_hash, get_relative_path},
};
use notify::{
    Config, Error, Event, EventKind, RecommendedWatcher, Watcher,
    event::{CreateKind, ModifyKind},
};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io,
    sync::mpsc::{self, Receiver, Sender},
    time,
};
use tracing::{error, info};

pub struct FileWatcher {
    state: Arc<AppState>,
    watcher: RecommendedWatcher,
    watch_rx: Receiver<Result<Event, Error>>,
    sync_tx: Sender<FileInfo>,
    absolute_base_path: PathBuf,
}

pub struct FileWatcherSender {
    state: Arc<AppState>,
    file_service: Arc<FileService>,
    sync_rx: Receiver<FileInfo>,
}

impl FileWatcher {
    pub fn new(state: Arc<AppState>, file_service: Arc<FileService>) -> (Self, FileWatcherSender) {
        let (watch_tx, watch_rx) = mpsc::channel(100);
        let (sync_tx, sync_rx) = mpsc::channel::<FileInfo>(100);

        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                let _ = watch_tx.blocking_send(res);
            },
            Config::default(),
        )
        .unwrap();

        (
            Self {
                state: state.clone(),
                absolute_base_path: state.constants.base_dir.canonicalize().unwrap(),
                watcher,
                watch_rx,
                sync_tx,
            },
            FileWatcherSender {
                state,
                file_service,
                sync_rx,
            },
        )
    }

    pub async fn watch(&mut self) -> io::Result<()> {
        for dir in self.state.entry_manager.list_dirs() {
            let path = self.state.constants.base_dir.join(&dir);

            self.watcher
                .watch(&path, notify::RecursiveMode::Recursive)
                .unwrap();

            info!("Watching for file changes in /{}", dir);
        }

        while let Some(res) = self.watch_rx.recv().await {
            match res {
                Ok(event) => self.handle_event(event).await,
                Err(err) => error!("Watch error: {}", err),
            };
        }
        Ok(())
    }

    async fn handle_event(&self, e: Event) {
        match e.kind {
            EventKind::Create(CreateKind::File) => self.handle_creation(e).await,
            EventKind::Modify(ModifyKind::Data(_)) => self.handle_modify(e).await,
            EventKind::Modify(ModifyKind::Name(_)) | EventKind::Remove(_) => {
                self.handle_removal(e).await
            }
            _ => {}
        };
    }

    async fn handle_modify(&self, e: Event) {
        for path in e.paths {
            if path.is_dir() {
                continue;
            }

            let Ok(relative_path) = get_relative_path(&path, &self.absolute_base_path) else {
                continue;
            };

            let Some(file) = self.state.entry_manager.get_file(&relative_path) else {
                continue;
            };

            let disk_hash = match compute_hash(&path) {
                Ok(hash) => hash,
                Err(err) => {
                    error!("Failed to compute {} hash: {}", relative_path, err);
                    continue;
                }
            };

            if file.hash != disk_hash {
                let file = FileInfo {
                    name: relative_path.clone(),
                    hash: disk_hash,
                    version: file.version + 1,
                    last_modified_by: None,
                };

                self.state.entry_manager.insert_file(file.clone());

                self.send_metadata(file).await;
            }
        }
    }

    async fn handle_creation(&self, e: Event) {
        for path in e.paths {
            let Ok(relative_path) = get_relative_path(&path, &self.absolute_base_path) else {
                continue;
            };

            if self.state.entry_manager.get_file(&relative_path).is_some() {
                continue;
            }

            let disk_hash = match compute_hash(&path) {
                Ok(hash) => hash,
                Err(err) => {
                    error!("Failed to compute {} hash: {}", relative_path, err);
                    continue;
                }
            };

            let file = FileInfo {
                name: relative_path.clone(),
                hash: disk_hash,
                version: 0,
                last_modified_by: None,
            };

            self.state.entry_manager.insert_file(file.clone());

            self.send_metadata(file).await;
        }
    }

    async fn handle_removal(&self, e: Event) {
        for path in e.paths {
            let Ok(relative_path) = get_relative_path(&path, &self.absolute_base_path) else {
                continue;
            };

            if path.exists() {
                continue;
            }

            if self.state.entry_manager.is_dir(&relative_path) {
                let removed_files = self.state.entry_manager.remove_dir(&relative_path);

                for file in removed_files {
                    self.send_metadata(file).await;
                }
            } else if self.state.entry_manager.get_file(&relative_path).is_some() {
                let removed = self.state.entry_manager.remove_file(&relative_path);
                self.send_metadata(removed).await;
            }
        }
    }

    async fn send_metadata(&self, file: FileInfo) {
        if let Err(err) = self.sync_tx.send(file).await {
            error!("sync_tx send error: {}", err);
        }
    }
}

impl FileWatcherSender {
    pub async fn send_changes(&mut self) -> io::Result<()> {
        let mut buffer = HashMap::<String, FileInfo>::new();
        let mut interval = time::interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                Some(file) = self.sync_rx.recv() => {
                    info!("File added to buffer: {}", file.name);
                    buffer.insert(file.name.clone(), file);
                }

                _ = interval.tick() => {
                    if buffer.is_empty() {
                        continue;
                    }

                    info!("Synching files: {:?}", buffer);

                    let sync_map = self.state.peer_manager.build_sync_map(&buffer);

                    for (addr, files) in sync_map {
                        for file in files {
                            self.file_service.send_metadata(file, addr).await?;
                        }
                    }

                    buffer.clear();
                }
            }
        }
    }
}
