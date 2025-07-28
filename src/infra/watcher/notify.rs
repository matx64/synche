use crate::{
    application::watcher::FileWatcherInterface,
    domain::filesystem::{ModifiedNamePaths, WatcherEvent},
};
use notify::{
    Config, Error, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{CreateKind, DataChange, ModifyKind, RemoveKind, RenameMode},
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
    pub fn new() -> Self {
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

    async fn next(&mut self) -> Option<WatcherEvent> {
        let event = match self.watch_rx.recv().await {
            Some(Ok(event)) => event,
            Some(Err(e)) => {
                error!("File Watcher error: {}", e);
                return None;
            }
            None => return None,
        };

        let from = event.paths.first().cloned()?;

        match event.kind {
            EventKind::Create(CreateKind::File) => Some(WatcherEvent::CreatedFile(from)),

            EventKind::Modify(ModifyKind::Data(DataChange::Content)) => {
                Some(WatcherEvent::ModifiedContent(from))
            }

            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                let to = event.paths.get(1).cloned()?;

                let modified = if to.is_file() {
                    WatcherEvent::ModifiedFileName(ModifiedNamePaths { from, to })
                } else if to.is_dir() {
                    WatcherEvent::ModifiedDirName(ModifiedNamePaths { from, to })
                } else {
                    return None;
                };

                Some(modified)
            }

            EventKind::Remove(kind) => {
                if from.exists() {
                    return None;
                }

                match kind {
                    RemoveKind::File => Some(WatcherEvent::RemovedFile(from)),
                    RemoveKind::Folder => Some(WatcherEvent::RemovedDir(from)),
                    _ => None,
                }
            }

            _ => None,
        }
    }
}
