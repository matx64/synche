use crate::{
    application::watcher::FileWatcherInterface,
    domain::watcher::{WatcherEvent, WatcherEventPath},
    utils::fs::get_relative_path,
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

        self.watcher
            .watch(&self.base_dir_absolute, RecursiveMode::Recursive)
            .unwrap();
        self.sync_dirs = dirs.into_iter().collect();

        Ok(())
    }

    async fn next(&mut self) -> Option<WatcherEvent> {
        let event = match self.watch_rx.recv().await {
            Some(Ok(event)) if !event.kind.is_access() && !event.kind.is_other() => event,
            Some(Err(e)) => {
                error!("Notify Watcher error: {}", e);
                return None;
            }
            _ => return None,
        };

        let from = event.paths.first().cloned()?;

        if self.sync_dirs.contains(&from) {
            self.handle_sync_dir_event(event, from)
        } else {
            for dir in self.sync_dirs.iter() {
                if from.starts_with(dir) {
                    return self.handle_event(event, from);
                }
            }
            None
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
        info!("Notify Event in Sync Directory: {event:?}");

        match event.kind {
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                let to = event.paths.get(1).cloned()?;

                if !to.exists() {
                    return None;
                }

                let from = self.build_path(from)?;
                let to = self.build_path(to)?;

                if to.is_dir() {
                    self.sync_dirs.remove(&from.absolute);
                    self.sync_dirs.insert(to.absolute.clone());
                    Some(WatcherEvent::RenamedSyncDir((from, to)))
                } else {
                    None
                }
            }

            EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                if from.exists() {
                    None
                } else {
                    Some(WatcherEvent::Removed(self.build_path(from)?))
                }
            }

            _ => None,
        }
    }

    fn handle_event(&self, event: Event, from: PathBuf) -> Option<WatcherEvent> {
        match event.kind {
            EventKind::Create(kind) if from.exists() => match kind {
                CreateKind::File => Some(WatcherEvent::CreatedFile(self.build_path(from)?)),
                CreateKind::Folder => Some(WatcherEvent::CreatedDir(self.build_path(from)?)),
                _ => None,
            },

            EventKind::Modify(ModifyKind::Data(_)) if from.exists() && from.is_file() => {
                Some(WatcherEvent::ModifiedFileContent(self.build_path(from)?))
            }

            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                let to = event.paths.get(1).cloned()?;

                if !to.exists() {
                    return None;
                }

                let from = self.build_path(from)?;
                let to = self.build_path(to)?;

                let modified = if to.is_file() {
                    WatcherEvent::RenamedFile((from, to))
                } else if to.is_dir() {
                    WatcherEvent::RenamedDir((from, to))
                } else {
                    return None;
                };

                Some(modified)
            }

            EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Name(RenameMode::From))
            | EventKind::Modify(ModifyKind::Name(RenameMode::Any))
                if !from.exists() =>
            {
                Some(WatcherEvent::Removed(self.build_path(from)?))
            }

            _ => None,
        }
    }

    fn build_path(&self, path: PathBuf) -> Option<WatcherEventPath> {
        Some(WatcherEventPath {
            relative: get_relative_path(&path, &self.base_dir_absolute).ok()?,
            absolute: path,
        })
    }
}
