use crate::{application::watcher::FileWatcherInterface, domain::filesystem::FileChangeEvent};
use notify::{
    Config, Error, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{CreateKind, ModifyKind},
};
use std::path::PathBuf;
use tokio::{
    io,
    sync::mpsc::{self, Receiver},
};
use tracing::error;

pub struct NotifyFileWatcher {
    watcher: RecommendedWatcher,
    watch_rx: Receiver<Result<Event, Error>>,
}

impl NotifyFileWatcher {
    pub async fn new() -> Self {
        let (watch_tx, watch_rx) = mpsc::channel(100);

        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                let _ = watch_tx.blocking_send(res);
            },
            Config::default(),
        )
        .unwrap();

        Self { watcher, watch_rx }
    }
}

impl FileWatcherInterface for NotifyFileWatcher {
    async fn watch(&mut self, dirs: Vec<PathBuf>) -> io::Result<()> {
        for dir in dirs {
            self.watcher.watch(&dir, RecursiveMode::Recursive).unwrap();
        }
        Ok(())
    }

    async fn next(&mut self) -> Option<FileChangeEvent> {
        let event = match self.watch_rx.recv().await {
            Some(Ok(event)) => event,
            Some(Err(e)) => {
                error!("File Watcher error: {}", e);
                return None;
            }
            None => {
                return None;
            }
        };

        let path = event.paths.first()?.to_path_buf();

        match event.kind {
            EventKind::Create(CreateKind::File) => Some(FileChangeEvent::Created(path)),
            EventKind::Modify(ModifyKind::Data(_)) => Some(FileChangeEvent::Modified(path)),
            EventKind::Modify(ModifyKind::Name(_)) | EventKind::Remove(_) => {
                Some(FileChangeEvent::Deleted(path))
            }
            _ => None,
        }
    }
}
