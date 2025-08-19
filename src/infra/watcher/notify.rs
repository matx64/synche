use crate::{
    application::watcher::FileWatcherInterface,
    domain::{
        CanonicalPath, RelativePath,
        watcher::{WatcherEvent, WatcherEventKind, WatcherEventPath},
    },
    utils::fs::is_ds_store,
};
use notify::{
    Config, Error, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{ModifyKind, RenameMode},
};
use std::collections::HashSet;
use tokio::{
    io,
    sync::mpsc::{self, Receiver},
};
use tracing::error;

pub struct NotifyFileWatcher {
    watcher: RecommendedWatcher,
    notify_rx: Receiver<Result<Event, Error>>,
    sync_dirs: HashSet<CanonicalPath>,
    base_dir_path: Option<CanonicalPath>,
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
            base_dir_path: None,
        }
    }
}

impl FileWatcherInterface for NotifyFileWatcher {
    async fn watch(&mut self, base_dir: CanonicalPath, dirs: Vec<CanonicalPath>) -> io::Result<()> {
        self.watcher
            .watch(&base_dir, RecursiveMode::Recursive)
            .unwrap();

        self.base_dir_path = Some(base_dir);
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
                    if let Some(path) = event.paths.first().cloned()
                        && let Ok(path) = CanonicalPath::new(path)
                        && !self.sync_dirs.contains(&path)
                        && self.sync_dirs.iter().any(|dir| path.starts_with(dir))
                        && !is_ds_store(&path)
                    {
                        if let Some(w) = self.handle_event(event, path) {
                            return Some(w);
                        } else {
                            continue;
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
    fn handle_event(&self, event: Event, path: CanonicalPath) -> Option<WatcherEvent> {
        match event.kind {
            EventKind::Create(_)
            | EventKind::Modify(ModifyKind::Data(_))
            | EventKind::Modify(ModifyKind::Any)
            | EventKind::Modify(ModifyKind::Name(RenameMode::To))
            | EventKind::Modify(ModifyKind::Name(RenameMode::Any))
                if path.exists() && (path.is_file() || path.is_dir()) =>
            {
                Some(WatcherEvent::new(
                    WatcherEventKind::CreateOrModify,
                    self.build_path(path)?,
                ))
            }

            EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Name(RenameMode::From))
            | EventKind::Modify(ModifyKind::Name(RenameMode::Any))
                if !path.exists() =>
            {
                Some(WatcherEvent::new(
                    WatcherEventKind::Remove,
                    self.build_path(path)?,
                ))
            }

            _ => None,
        }
    }

    fn build_path(&self, path: CanonicalPath) -> Option<WatcherEventPath> {
        let relative = match RelativePath::new(&path, self.base_dir_path.as_ref().unwrap()) {
            Ok(rel) => Some(rel),
            Err(err) => {
                error!("{err}");
                None
            }
        }?;

        Some(WatcherEventPath {
            relative,
            canonical: path,
        })
    }
}
