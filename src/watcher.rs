use crate::{
    config::AppState,
    models::entry::Entry,
    utils::fs::{get_file_data, get_relative_path},
};
use notify::{Config, Error, Event, EventKind, RecommendedWatcher, Watcher};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};
use tokio::{
    io,
    sync::mpsc::{self, Receiver, Sender},
};
use tracing::{error, info};

pub struct FileWatcher {
    state: Arc<AppState>,
    watcher: RecommendedWatcher,
    watch_rx: Receiver<Result<Event, Error>>,
    sync_tx: Sender<Entry>,
    absolute_base_path: PathBuf,
}

impl FileWatcher {
    pub fn new(state: Arc<AppState>, sync_tx: Sender<Entry>) -> Self {
        let (watch_tx, watch_rx) = mpsc::channel(100);

        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                let _ = watch_tx.blocking_send(res);
            },
            Config::default(),
        )
        .unwrap();

        Self {
            absolute_base_path: state.constants.entries_dir.canonicalize().unwrap(),
            state,
            watcher,
            watch_rx,
            sync_tx,
        }
    }

    pub async fn watch(&mut self) -> io::Result<()> {
        let path = self.state.constants.entries_dir.to_owned();
        self.watcher
            .watch(&path, notify::RecursiveMode::Recursive)
            .unwrap();

        info!("Watching for entry changes in /{}", path.to_string_lossy());

        while let Some(res) = self.watch_rx.recv().await {
            match res {
                Ok(event)
                    if matches!(
                        event.kind,
                        EventKind::Modify(_) | EventKind::Remove(_) | EventKind::Create(_)
                    ) =>
                {
                    self.handle_event(event).await
                }
                Ok(_) => {}
                Err(err) => {
                    error!("Watch error: {}", err);
                }
            }
        }
        Ok(())
    }

    async fn handle_event(&self, e: Event) {
        for path in e.paths {
            let Ok(relative_path) = get_relative_path(&path, &self.absolute_base_path) else {
                continue;
            };

            let Some(entry) = self.state.entry_manager.get(&relative_path) else {
                continue;
            };

            if !path.exists() {
                self.state.entry_manager.handle_deletion(&entry);
                continue;
            }

            if path.is_dir() {
                self.update_dir_date(&path, entry);
                continue;
            }

            let (hash, on_disk_modified) = match get_file_data(&path) {
                Ok(data) => data,
                Err(err) => {
                    error!("Failed to get file data: {}", err);
                    continue;
                }
            };

            if entry.hash != hash && entry.last_modified_at < on_disk_modified {
                let file = Entry {
                    name: relative_path.clone(),
                    exists: true,
                    is_dir: false,
                    last_modified_at: on_disk_modified,
                    hash,
                };

                self.state.entry_manager.insert(file.clone());

                if let Err(err) = self.sync_tx.send(file).await {
                    error!("sync_tx send error: {}", err);
                }
            }
        }
    }

    fn update_dir_date(&self, path: &Path, entry: Entry) {
        let last_modified_at = path
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::now());

        self.state.entry_manager.insert(Entry {
            last_modified_at,
            ..entry
        });
    }
}
