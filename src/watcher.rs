use crate::config::SynchedFile;
use notify::{Config, Error, Event, RecommendedWatcher, Watcher};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, RwLock},
    time::SystemTime,
};
use tokio::{
    io,
    sync::mpsc::{self, Receiver, Sender},
};

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

        println!("Watching for file changes...");

        while let Some(res) = self.watch_rx.recv().await {
            match res {
                Ok(event) if event.kind.is_modify() => self.handle_event(event).await,
                Ok(_) => {}
                Err(err) => {
                    eprintln!("Watch error: {}", err);
                }
            }
        }
        Ok(())
    }

    async fn handle_event(&self, e: Event) {
        for path in e.paths {
            let Some(file_name) = path.file_name().and_then(|f| f.to_str()) else {
                println!("Couldn't extract file name from path: {:?}", path);
                continue;
            };

            println!("File changed: {}", file_name);

            let file = match self.synched_files.write() {
                Ok(mut files) => {
                    if let Some(file) = files.get_mut(file_name) {
                        file.last_modified_at = path
                            .metadata()
                            .and_then(|m| m.modified())
                            .unwrap_or(SystemTime::now());
                        Some(file.clone())
                    } else {
                        None
                    }
                }
                Err(err) => {
                    eprintln!("Failed to read synched_files: {}", err);
                    None
                }
            };

            if let Some(file) = file {
                if let Err(err) = self.sync_tx.send(file).await {
                    eprintln!("sync_tx send error: {}", err);
                }
            }
        }
    }
}
