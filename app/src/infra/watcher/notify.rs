use crate::{
    application::watcher::interface::FileWatcherInterface,
    domain::{
        AppState, CanonicalPath, RelativePath, WatcherEvent, WatcherEventKind, WatcherEventPath,
    },
    utils::fs::is_ds_store,
};
use notify::{
    Config, Error, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{ModifyKind, RenameMode},
};
use std::sync::Arc;
use tokio::{
    io,
    sync::mpsc::{self, Receiver},
};
use tracing::error;

pub struct NotifyFileWatcher {
    state: Arc<AppState>,
    watcher: RecommendedWatcher,
    notify_rx: Receiver<Result<Event, Error>>,
}

impl FileWatcherInterface for NotifyFileWatcher {
    fn new(state: Arc<AppState>) -> Self {
        let (notify_tx, notify_rx) = mpsc::channel(100);

        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                notify_tx.blocking_send(res).unwrap();
            },
            Config::default(),
        )
        .unwrap();

        Self {
            state,
            watcher,
            notify_rx,
        }
    }

    async fn watch(&mut self) -> io::Result<()> {
        self.watcher
            .watch(&self.state.home_path, RecursiveMode::Recursive)
            .map_err(|e| io::Error::other(e.to_string()))
    }

    async fn next(&mut self) -> Option<WatcherEvent> {
        while let Some(res) = self.notify_rx.recv().await {
            match res {
                Ok(event) if event.kind.is_access() || event.kind.is_other() => {
                    continue;
                }

                Ok(event) => {
                    if let Some(path) = event.paths.first().cloned()
                        && let path = CanonicalPath::from_canonical(path)
                        && !self.sync_dirs.contains(&path)
                        && self.sync_dirs.iter().any(|dir| path.starts_with(dir))
                        && !is_ds_store(&path)
                    {
                        if let Some(event) = self.handle_event(event, path) {
                            return Some(event);
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
                    self.build_path(path),
                ))
            }

            EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Name(RenameMode::From))
            | EventKind::Modify(ModifyKind::Name(RenameMode::Any))
                if !path.exists() =>
            {
                Some(WatcherEvent::new(
                    WatcherEventKind::Remove,
                    self.build_path(path),
                ))
            }

            _ => None,
        }
    }

    fn build_path(&self, canonical: CanonicalPath) -> WatcherEventPath {
        WatcherEventPath {
            relative: RelativePath::new(&canonical, &self.state.home_path),
            canonical,
        }
    }
}
