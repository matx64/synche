use crate::{
    application::watcher::FileWatcherInterface,
    domain::filesystem::{ModifiedNamePaths, WatcherEvent},
};
use notify::{
    Config, Error, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{CreateKind, ModifyKind, RenameMode},
};
use std::{collections::HashSet, path::PathBuf};
use tokio::{
    io,
    sync::mpsc::{self, Receiver},
};
use tracing::{error, info};

pub struct NotifyFileWatcher {
    watcher: RecommendedWatcher,
    watch_rx: Receiver<Result<Event, Error>>,
    sync_dirs: HashSet<PathBuf>,
    base_dir_absolute: PathBuf,
}

impl FileWatcherInterface for NotifyFileWatcher {
    async fn watch(&mut self, base_dir: PathBuf, dirs: Vec<PathBuf>) -> io::Result<()> {
        self.base_dir_absolute = base_dir;
        self.sync_dirs.clear();

        self.watcher
            .watch(&self.base_dir_absolute, RecursiveMode::Recursive)
            .unwrap();

        for dir in dirs {
            self.sync_dirs.insert(dir);
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

        if self.sync_dirs.contains(&from) {
            self.handle_sync_dir_event(event, from)
        } else {
            self.handle_event(event, from)
        }
    }
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

        Self {
            watcher,
            watch_rx,
            sync_dirs: HashSet::new(),
            base_dir_absolute: PathBuf::new(),
        }
    }

    fn handle_sync_dir_event(&mut self, event: Event, from: PathBuf) -> Option<WatcherEvent> {
        if !matches!(event.kind, EventKind::Access(_)) {
            info!("Notify Event in Sync Directory: {event:?}");
        }

        match event.kind {
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                let to = event.paths.get(1).cloned()?;

                if !to.exists() {
                    return None;
                }

                if to.is_dir() {
                    self.sync_dirs.remove(&from);
                    self.sync_dirs.insert(to.clone());
                    Some(WatcherEvent::RenamedSyncDir(ModifiedNamePaths { from, to }))
                } else {
                    None
                }
            }

            EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                if from.exists() {
                    None
                } else {
                    Some(WatcherEvent::Removed(from))
                }
            }

            _ => None,
        }
    }

    fn handle_event(&self, event: Event, from: PathBuf) -> Option<WatcherEvent> {
        match event.kind {
            EventKind::Create(CreateKind::File) => {
                if from.exists() {
                    Some(WatcherEvent::CreatedFile(from))
                } else {
                    None
                }
            }

            EventKind::Modify(ModifyKind::Data(_)) => {
                if from.exists() && from.is_file() {
                    Some(WatcherEvent::ModifiedFileContent(from))
                } else {
                    None
                }
            }

            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                let to = event.paths.get(1).cloned()?;

                if !to.exists() {
                    return None;
                }

                let modified = if to.is_file() {
                    WatcherEvent::RenamedFile(ModifiedNamePaths { from, to })
                } else if to.is_dir() {
                    WatcherEvent::RenamedDir(ModifiedNamePaths { from, to })
                } else {
                    return None;
                };

                Some(modified)
            }

            EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                if from.exists() {
                    None
                } else {
                    Some(WatcherEvent::Removed(from))
                }
            }

            _ => None,
        }
    }
}
