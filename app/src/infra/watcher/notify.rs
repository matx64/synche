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
                        && let canonical = CanonicalPath::from_canonical(path)
                        && let relative = RelativePath::new(&canonical, &self.state.home_path)
                        && self.is_valid_entry(&relative).await
                        && !is_ds_store(&canonical)
                    {
                        if let Some(event) = self.handle_event(event, canonical, relative) {
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
    fn handle_event(
        &self,
        event: Event,
        canonical: CanonicalPath,
        relative: RelativePath,
    ) -> Option<WatcherEvent> {
        match event.kind {
            EventKind::Create(_)
            | EventKind::Modify(ModifyKind::Data(_))
            | EventKind::Modify(ModifyKind::Any)
            | EventKind::Modify(ModifyKind::Name(RenameMode::To))
            | EventKind::Modify(ModifyKind::Name(RenameMode::Any))
                if canonical.exists() && (canonical.is_file() || canonical.is_dir()) =>
            {
                Some(WatcherEvent::new(
                    WatcherEventKind::CreateOrModify,
                    WatcherEventPath {
                        canonical,
                        relative,
                    },
                ))
            }

            EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Name(RenameMode::From))
            | EventKind::Modify(ModifyKind::Name(RenameMode::Any))
                if !canonical.exists() =>
            {
                Some(WatcherEvent::new(
                    WatcherEventKind::Remove,
                    WatcherEventPath {
                        canonical,
                        relative,
                    },
                ))
            }

            _ => None,
        }
    }

    async fn is_valid_entry(&self, path: &RelativePath) -> bool {
        let dirs = self.state.sync_dirs.read().await;

        !dirs.contains_key(path) && dirs.keys().any(|d| path.starts_with(&**d))
    }
}
