use crate::{
    config::AppState,
    models::{entry::File, sync::SyncFileKind},
    utils::fs::{get_file_data, get_relative_path},
};
use notify::{
    Config, Error, Event, EventKind, RecommendedWatcher, Watcher,
    event::{CreateKind, ModifyKind},
};
use std::{path::PathBuf, sync::Arc};
use tokio::{
    io,
    sync::mpsc::{self, Receiver, Sender},
};
use tracing::{error, info};

pub struct FileWatcher {
    state: Arc<AppState>,
    watcher: RecommendedWatcher,
    watch_rx: Receiver<Result<Event, Error>>,
    sync_tx: Sender<(SyncFileKind, File)>,
    absolute_base_path: PathBuf,
}

impl FileWatcher {
    pub fn new(state: Arc<AppState>, sync_tx: Sender<(SyncFileKind, File)>) -> Self {
        let (watch_tx, watch_rx) = mpsc::channel(100);

        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                let _ = watch_tx.blocking_send(res);
            },
            Config::default(),
        )
        .unwrap();

        Self {
            absolute_base_path: state.constants.base_dir.canonicalize().unwrap(),
            state,
            watcher,
            watch_rx,
            sync_tx,
        }
    }

    pub async fn watch(&mut self) -> io::Result<()> {
        for dir in self.state.entry_manager.dirs() {
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

            let (hash, on_disk_modified) = match get_file_data(&path) {
                Ok(data) => data,
                Err(err) => {
                    error!("Failed to get file data: {}", err);
                    continue;
                }
            };

            if file.hash != hash && file.last_modified_at < on_disk_modified {
                let file = File {
                    name: relative_path.clone(),
                    last_modified_at: on_disk_modified,
                    hash,
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

            let (hash, on_disk_modified) = match get_file_data(&path) {
                Ok(data) => data,
                Err(err) => {
                    error!("Failed to get file data: {}", err);
                    continue;
                }
            };

            let file = File {
                name: relative_path.clone(),
                last_modified_at: on_disk_modified,
                hash,
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

            if path.is_dir() {
                let removed_files = self.state.entry_manager.remove_dir(&relative_path);

                for file in removed_files {
                    self.send_metadata(file).await;
                }
            } else {
                self.state.entry_manager.remove_file(&relative_path);
                self.send_metadata(File::absent(relative_path)).await;
            }
        }
    }

    async fn send_metadata(&self, file: File) {
        if let Err(err) = self.sync_tx.send((SyncFileKind::Metadata, file)).await {
            error!("sync_tx send error: {}", err);
        }
    }
}
