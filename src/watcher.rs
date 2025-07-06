use crate::config::SynchedFile;
use notify::{Config, Error, Event, RecommendedWatcher, Watcher};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::Path,
    sync::{Arc, RwLock},
    time::SystemTime,
};
use tokio::{
    io,
    sync::mpsc::{self, Receiver, Sender},
};
use tracing::{error, info};

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    watch_rx: Receiver<Result<Event, Error>>,
    sync_tx: Sender<SynchedFile>,
    synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
}

impl FileWatcher {
    pub fn new(
        sync_tx: Sender<SynchedFile>,
        synched_files: Arc<RwLock<HashMap<String, SynchedFile>>>,
    ) -> Self {
        let (watch_tx, watch_rx) = mpsc::channel(100);

        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                let _ = watch_tx.blocking_send(res);
            },
            Config::default(),
        )
        .unwrap();

        Self {
            watcher,
            watch_rx,
            sync_tx,
            synched_files,
        }
    }

    pub async fn watch(&mut self) -> io::Result<()> {
        let path = Path::new("synche-files");
        self.watcher
            .watch(path, notify::RecursiveMode::Recursive)
            .unwrap();

        info!("Watching for file changes...");

        while let Some(res) = self.watch_rx.recv().await {
            match res {
                Ok(event) if event.kind.is_modify() => self.handle_event(event).await,
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
                info!("Couldn't extract file name from path: {:?}", path);
                continue;
            };

            info!("File changed: {}", file_name);

            // Read file content and compute hash
            let mut file = match File::open(&path) {
                Ok(f) => f,
                Err(err) => {
                    error!("Failed to open file {}: {}", file_name, err);
                    continue;
                }
            };

            let mut content = Vec::new();
            if let Err(err) = file.read_to_end(&mut content) {
                error!("Failed to read file {}: {}", file_name, err);
                continue;
            };

            let hash = format!("{:x}", Sha256::digest(&content));

            let metadata = match path.metadata() {
                Ok(m) => m,
                Err(err) => {
                    error!("Failed to get metadata for {}: {}", file_name, err);
                    continue;
                }
            };

            let on_disk_modified = metadata.modified().unwrap_or(SystemTime::now());

            let should_update = if let Ok(files) = self.synched_files.read() {
                if let Some(file) = files.get(file_name) {
                    on_disk_modified > file.last_modified_at || hash != file.hash
                } else {
                    true
                }
            } else {
                true
            };

            if should_update {
                let file_name = file_name.to_owned();
                let file = SynchedFile {
                    name: file_name.clone(),
                    last_modified_at: on_disk_modified,
                    hash,
                };
                if let Ok(mut files) = self.synched_files.write() {
                    files.insert(file_name, file.clone());
                }
                if let Err(err) = self.sync_tx.send(file).await {
                    error!("sync_tx send error: {}", err);
                }
            }
        }
    }
}
