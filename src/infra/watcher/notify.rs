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

impl NotifyFileWatcher {
    pub fn new() -> Self {
        let (notify_tx, notify_rx) = mpsc::channel(100);

        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                notify_tx.blocking_send(res).unwrap();
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
        while let Some(res) = self.notify_rx.recv().await {
            match res {
                Ok(event) if event.kind.is_access() || event.kind.is_other() => {
                    continue;
                }

                Ok(event) => {
                    if let Some(path) = event.paths.first().cloned() {
                        if !self.sync_dirs.contains(&path)
                            && self.sync_dirs.iter().any(|dir| path.starts_with(dir))
                        {
                            warn!("{:?}", event);
                            if let Some(w) = self.handle_event(event, path) {
                                return Some(w);
                            } else {
                                continue;
                            }
                        }
                    }
                    continue;
                }

                Err(e) => {
                    error!("Notify Watcher error: {}", e);
                    return None;
                }
            }
        }
        None
    }
}

impl NotifyFileWatcher {
    fn handle_event(&self, event: Event, path: PathBuf) -> Option<WatcherEvent> {
        match event.kind {
            EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(RenameMode::To))
                if path.exists() =>
            {
                if path.is_file() {
                    Some(WatcherEvent::new(
                        WatcherEventKind::CreatedFile,
                        self.build_path(path)?,
                    ))
                } else if path.is_dir() {
                    Some(WatcherEvent::new(
                        WatcherEventKind::CreatedDir,
                        self.build_path(path)?,
                    ))
                } else {
                    None
                }
            }

            EventKind::Modify(ModifyKind::Name(RenameMode::Any)) if path.exists() => Some(
                WatcherEvent::new(WatcherEventKind::ModifiedAny, self.build_path(path)?),
            ),

            EventKind::Modify(ModifyKind::Data(_)) | EventKind::Modify(ModifyKind::Any)
                if path.exists() && path.is_file() =>
            {
                Some(WatcherEvent::new(
                    WatcherEventKind::ModifiedFileContent,
                    self.build_path(path)?,
                ))
            }

            EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Name(RenameMode::From))
            | EventKind::Modify(ModifyKind::Name(RenameMode::Any))
                if !path.exists() =>
            {
                Some(WatcherEvent::new(
                    WatcherEventKind::Removed,
                    self.build_path(path)?,
                ))
            }

            _ => None,
        }
    }

    fn build_path(&self, path: PathBuf) -> Option<WatcherEventPath> {
        let relative = match get_relative_path(&path, &self.base_dir_absolute) {
            Ok(rel) => Some(rel),
            Err(err) => {
                error!("{err}");
                None
            }
        }?;

        Some(WatcherEventPath {
            relative,
            absolute: path,
        })
    }
}
