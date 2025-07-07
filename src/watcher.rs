use crate::{config::AppState, models::file::SynchedFile};
use notify::{Config, Error, Event, EventKind, RecommendedWatcher, Watcher};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};
use tokio::{
    io,
    sync::mpsc::{self, Receiver, Sender},
};
use tracing::{error, info, warn};

pub struct FileWatcher {
    state: Arc<AppState>,
    watcher: RecommendedWatcher,
    watch_rx: Receiver<Result<Event, Error>>,
    sync_tx: Sender<SynchedFile>,
}

impl FileWatcher {
    pub fn new(state: Arc<AppState>, sync_tx: Sender<SynchedFile>) -> Self {
        let (watch_tx, watch_rx) = mpsc::channel(100);

        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                let _ = watch_tx.blocking_send(res);
            },
            Config::default(),
        )
        .unwrap();

        Self {
            state,
            watcher,
            watch_rx,
            sync_tx,
        }
    }

    pub async fn watch(&mut self) -> io::Result<()> {
        let path = Path::new(&self.state.constants.files_dir);
        self.watcher
            .watch(path, notify::RecursiveMode::Recursive)
            .unwrap();

        info!(
            "Watching for file changes in /{}",
            self.state.constants.files_dir
        );

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
            let Some(file_name) = path.file_name().and_then(|f| f.to_str()) else {
                warn!("Couldn't extract file name from path: {:?}", path);
                continue;
            };

            // check if is a synched file
            let synched_file = if let Ok(files) = self.state.synched_files.read() {
                if let Some(file) = files.get(file_name) {
                    file.clone()
                } else {
                    continue;
                }
            } else {
                continue;
            };

            if !path.exists() {
                if let Ok(mut files) = self.state.synched_files.write() {
                    files.insert(file_name.to_owned(), SynchedFile::absent(file_name));
                }
                continue;
            }

            let (hash, on_disk_modified) = match self.get_file_data(&path) {
                Ok(data) => data,
                Err(err) => {
                    error!("Failed to get file data: {}", err);
                    continue;
                }
            };

            if synched_file.hash != hash && synched_file.last_modified_at < on_disk_modified {
                let file_name = file_name.to_owned();
                let file = SynchedFile {
                    name: file_name.clone(),
                    exists: true,
                    last_modified_at: on_disk_modified,
                    hash,
                };
                if let Ok(mut files) = self.state.synched_files.write() {
                    files.insert(file_name, file.clone());
                }
                if let Err(err) = self.sync_tx.send(file).await {
                    error!("sync_tx send error: {}", err);
                }
            }
        }
    }

    fn get_file_data(&self, path: &PathBuf) -> io::Result<(String, SystemTime)> {
        let mut file = File::open(path)?;

        let mut content = Vec::new();
        file.read_to_end(&mut content)?;

        let hash = format!("{:x}", Sha256::digest(&content));
        let on_disk_modified = path.metadata()?.modified().unwrap_or(SystemTime::now());

        Ok((hash, on_disk_modified))
    }
}
