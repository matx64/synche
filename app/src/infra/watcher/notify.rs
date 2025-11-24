use crate::{
    application::watcher::interface::FileWatcherInterface,
    domain::{
        AppState, CanonicalPath, ConfigWatcherEvent, HomeWatcherEvent, RelativePath,
        WatcherEventPath,
    },
    utils::fs::{config_file, is_ds_store},
};
use notify::{
    Config, Error, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{ModifyKind, RenameMode},
};
use std::{path::PathBuf, sync::Arc};
use tokio::{
    io,
    sync::{
        Mutex,
        mpsc::{self, Receiver},
    },
};

pub struct NotifyFileWatcher {
    state: Arc<AppState>,
    home_watcher: RecommendedWatcher,
    config_watcher: RecommendedWatcher,
    home_rx: Mutex<Receiver<Result<Event, Error>>>,
    config_rx: Mutex<Receiver<Result<Event, Error>>>,
    config_path: PathBuf,
}

impl FileWatcherInterface for NotifyFileWatcher {
    fn new(state: Arc<AppState>) -> Self {
        let (home_tx, home_rx) = mpsc::channel(100);
        let (config_tx, config_rx) = mpsc::channel(100);

        let home_watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                home_tx.blocking_send(res).unwrap();
            },
            Config::default(),
        )
        .unwrap();

        let config_watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                config_tx.blocking_send(res).unwrap();
            },
            Config::default(),
        )
        .unwrap();

        Self {
            state,
            home_watcher,
            config_watcher,
            home_rx: Mutex::new(home_rx),
            config_rx: Mutex::new(config_rx),
            config_path: config_file().as_ref().to_owned(),
        }
    }

    async fn watch_home(&mut self) -> io::Result<()> {
        self.home_watcher
            .watch(&self.state.home_path(), RecursiveMode::Recursive)
            .map_err(|e| io::Error::other(e.to_string()))
    }

    async fn watch_config(&mut self) -> io::Result<()> {
        self.config_watcher
            .watch(&config_file(), RecursiveMode::NonRecursive)
            .map_err(|e| io::Error::other(e.to_string()))
    }

    async fn next_home_event(&self) -> io::Result<Option<HomeWatcherEvent>> {
        while let Some(res) = self.home_rx.lock().await.recv().await {
            match res {
                Ok(event) if event.kind.is_access() || event.kind.is_other() => {
                    continue;
                }

                Ok(event) => {
                    if let Some(path) = event.paths.first().cloned()
                        && let canonical = CanonicalPath::from_canonical(path)
                        && let relative = RelativePath::new(&canonical, self.state.home_path())
                        && self.is_valid_entry(&relative).await
                        && !is_ds_store(&canonical)
                    {
                        if let Some(event) = self.handle_home_event(event, canonical, relative) {
                            return Ok(Some(event));
                        } else {
                            continue;
                        }
                    }
                    continue;
                }

                Err(e) => {
                    return Err(io::Error::other(e));
                }
            }
        }
        Ok(None)
    }

    async fn next_config_event(&self) -> io::Result<Option<ConfigWatcherEvent>> {
        match self.config_rx.lock().await.recv().await {
            Some(Ok(event)) => Ok(self.handle_config_event(event)),
            Some(Err(e)) => Err(io::Error::other(e)),
            None => Ok(None),
        }
    }
}

impl NotifyFileWatcher {
    fn handle_home_event(
        &self,
        event: Event,
        canonical: CanonicalPath,
        relative: RelativePath,
    ) -> Option<HomeWatcherEvent> {
        match event.kind {
            EventKind::Create(_)
            | EventKind::Modify(ModifyKind::Data(_))
            | EventKind::Modify(ModifyKind::Any)
            | EventKind::Modify(ModifyKind::Name(RenameMode::To))
            | EventKind::Modify(ModifyKind::Name(RenameMode::Any))
                if canonical.exists() && (canonical.is_file() || canonical.is_dir()) =>
            {
                Some(HomeWatcherEvent::CreateOrModify(WatcherEventPath {
                    relative,
                    canonical,
                }))
            }

            EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Name(RenameMode::From))
            | EventKind::Modify(ModifyKind::Name(RenameMode::Any))
                if !canonical.exists() =>
            {
                Some(HomeWatcherEvent::Remove(WatcherEventPath {
                    canonical,
                    relative,
                }))
            }

            _ => None,
        }
    }

    fn handle_config_event(&self, event: Event) -> Option<ConfigWatcherEvent> {
        match event.kind {
            EventKind::Create(_)
            | EventKind::Modify(ModifyKind::Data(_))
            | EventKind::Modify(ModifyKind::Any)
            | EventKind::Modify(ModifyKind::Name(RenameMode::To))
            | EventKind::Modify(ModifyKind::Name(RenameMode::Any))
                if self.config_path.exists() && self.config_path.is_file() =>
            {
                Some(ConfigWatcherEvent::Modify)
            }

            EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Name(RenameMode::From))
            | EventKind::Modify(ModifyKind::Name(RenameMode::Any))
                if !self.config_path.exists() =>
            {
                Some(ConfigWatcherEvent::Remove)
            }

            _ => None,
        }
    }

    async fn is_valid_entry(&self, path: &RelativePath) -> bool {
        let dirs = self.state.sync_dirs.read().await;

        !dirs.contains_key(path) && dirs.keys().any(|d| path.starts_with(&**d))
    }
}
