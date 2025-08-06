use crate::{
    application::watcher::FileWatcherInterface,
    domain::watcher::{WatcherEvent, WatcherEventKind, WatcherEventPath},
    utils::fs::get_relative_path,
};
use notify::{
    Config, Error, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{ModifyKind, RenameMode},
};
use std::{collections::HashSet, path::PathBuf};
use tokio::{
    io,
    sync::mpsc::{self, Receiver},
};
use tracing::{error, warn};

pub struct NotifyFileWatcher {
    watcher: RecommendedWatcher,
    notify_rx: Receiver<Result<Event, Error>>,
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
        let event = match self.notify_rx.recv().await {
            Some(Ok(event)) if !event.kind.is_access() && !event.kind.is_other() => event,
            Some(Err(e)) => {
                error!("Notify Watcher error: {}", e);
                return None;
            }
            _ => return None,
        };

        let from = event.paths.first().cloned()?;

        warn!("{:?}", event);

        if self.sync_dirs.contains(&from) {
            panic!("Modified Synced Directory: {:?}", event);
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
        let (notify_tx, notify_rx) = mpsc::channel(100);

        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                let _ = notify_tx.blocking_send(res);
            },
            Config::default(),
        )
        .unwrap();

        Self {
            watcher,
            notify_rx,
            sync_dirs: HashSet::new(),
            base_dir_absolute: PathBuf::new(),
        }
    }

    fn handle_event(&self, event: Event, from: PathBuf) -> Option<WatcherEvent> {
        match event.kind {
            EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(RenameMode::To))
                if from.exists() =>
            {
                if from.is_file() {
                    Some(WatcherEvent::new(
                        WatcherEventKind::CreatedFile,
                        self.build_path(from)?,
                    ))
                } else if from.is_dir() {
                    Some(WatcherEvent::new(
                        WatcherEventKind::CreatedDir,
                        self.build_path(from)?,
                    ))
                } else {
                    None
                }
            }

            EventKind::Modify(ModifyKind::Name(RenameMode::Any)) if from.exists() => Some(
                WatcherEvent::new(WatcherEventKind::ModifiedAny, self.build_path(from)?),
            ),

            EventKind::Modify(ModifyKind::Data(_)) | EventKind::Modify(ModifyKind::Any)
                if from.exists() && from.is_file() =>
            {
                Some(WatcherEvent::new(
                    WatcherEventKind::ModifiedFileContent,
                    self.build_path(from)?,
                ))
            }

            EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Name(RenameMode::From))
            | EventKind::Modify(ModifyKind::Name(RenameMode::Any))
                if !from.exists() =>
            {
                Some(WatcherEvent::new(
                    WatcherEventKind::Removed,
                    self.build_path(from)?,
                ))
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
