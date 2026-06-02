use crate::{
    application::{
        AppState, EntryManager, PeerManager,
        persistence::interface::PersistenceInterface,
        watcher::{buffer::WatcherBuffer, interface::FileWatcherInterface},
    },
    domain::{
        Config, ConfigWatcherEvent, EntryInfo, EntryKind, HomeWatcherEvent, RelativePath,
        ServerEvent, TransportChannelData, WatcherEventPath,
    },
    utils::fs::compute_hash,
};
use std::{collections::HashSet, sync::Arc};
use tokio::{io, sync::mpsc::Sender};
use tracing::{error, info, trace, warn};

/// Application service that consumes events from a
/// `FileWatcherInterface` adapter, debounces them through a
/// `WatcherBuffer`, and reacts: home-tree changes become outbound
/// `Metadata` transfers and persistence writes; `config.toml` changes
/// are applied live (including the `home_path` restart sentinel).
pub struct FileWatcher<T: FileWatcherInterface, P: PersistenceInterface> {
    adapter: T,
    buffer: WatcherBuffer,
    state: Arc<AppState>,
    peer_manager: Arc<PeerManager>,
    entry_manager: Arc<EntryManager<P>>,
    sender_tx: Sender<TransportChannelData>,
}

impl<T: FileWatcherInterface, P: PersistenceInterface> FileWatcher<T, P> {
    pub fn new(
        adapter: T,
        state: Arc<AppState>,
        peer_manager: Arc<PeerManager>,
        entry_manager: Arc<EntryManager<P>>,
        sender_tx: Sender<TransportChannelData>,
    ) -> Self {
        Self {
            state,
            adapter,
            sender_tx,
            peer_manager,
            entry_manager,
            buffer: WatcherBuffer::default(),
        }
    }

    /// Starts watching both streams and drives the buffer plus the
    /// four event-handling tasks concurrently. Returns when any task
    /// errors or terminates.
    pub async fn run(&mut self) -> io::Result<()> {
        self.adapter.watch_home().await?;
        self.adapter.watch_config().await?;

        tokio::select! {
            res = self.buffer.run() => res,
            res = self.recv_home_buffer_events() => res,
            res = self.recv_adapter_home_events() => res,
            res = self.recv_config_buffer_events() => res,
            res = self.recv_adapter_config_events() => res,
        }
    }

    async fn recv_adapter_home_events(&self) -> io::Result<()> {
        while let Some(event) = self.adapter.next_home_event().await? {
            let path = event.path();
            if self.state.is_remote_write_in_progress(&path.relative).await {
                continue;
            }

            if !self
                .entry_manager
                .is_ignored(&path.canonical, &path.relative)
                .await
            {
                self.buffer.insert_home_event(event).await;
            }
        }
        warn!("Watcher Adapter home channel closed");
        Ok(())
    }

    async fn recv_adapter_config_events(&self) -> io::Result<()> {
        while let Some(event) = self.adapter.next_config_event().await? {
            self.buffer.insert_config_event(event).await;
        }
        warn!("Watcher Adapter config channel closed");
        Ok(())
    }

    async fn recv_home_buffer_events(&self) -> io::Result<()> {
        while let Some(event) = self.buffer.next_home_event().await {
            info!("{event:?}");
            match event {
                HomeWatcherEvent::EntryCreateOrModify(path) => {
                    self.handle_entry_create_or_modify(path).await?;
                }
                HomeWatcherEvent::EntryRemove(path) => {
                    self.handle_entry_remove(path).await?;
                }
                HomeWatcherEvent::SyncDirectoryRemove(path) => {
                    if self.remove_sync_dir(&path.relative).await? {
                        self.resync_all_peers().await?;
                    }
                }
            }
        }
        warn!("Watcher Buffer home channel closed");
        Ok(())
    }

    async fn recv_config_buffer_events(&self) -> io::Result<()> {
        while let Some(event) = self.buffer.next_config_event().await {
            info!("{event:?}");
            match event {
                ConfigWatcherEvent::Modify => {
                    self.handle_config_modify().await?;
                }
                ConfigWatcherEvent::Remove => {
                    return Err(io::Error::other("config.toml removed or moved"));
                }
            }
        }
        warn!("Watcher Buffer config channel closed");
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(path = %path.relative))]
    async fn handle_entry_create_or_modify(&self, path: WatcherEventPath) -> io::Result<()> {
        // Issue #40 (B5): skip events the synchronizer itself triggered
        // while committing a peer Transfer. Without this guard a watcher
        // event in the move→persist window sees the stale DB hash and
        // broadcasts a spurious local-edit Metadata for a remote write.
        if self.state.is_remote_write_in_progress(&path.relative).await {
            return Ok(());
        }

        match self.entry_manager.get_entry(&path.relative).await? {
            None => self.handle_entry_create(path).await,

            Some(entry) if path.is_file() && entry.is_file() => {
                self.handle_modify_file(path, entry).await
            }

            _ => Ok(()),
        }
    }

    async fn handle_entry_create(&self, path: WatcherEventPath) -> io::Result<()> {
        if path.is_file() {
            self.handle_create_file(path).await
        } else {
            self.handle_create_dir(path).await
        }
    }

    async fn handle_create_file(&self, path: WatcherEventPath) -> io::Result<()> {
        let disk_hash = Some(compute_hash(&path.canonical).await?);

        let file = self
            .entry_manager
            .entry_created(&path.relative, EntryKind::File, disk_hash)
            .await?;

        self.send_metadata(file).await;

        if path.relative.ends_with(".gitignore") {
            self.entry_manager.insert_gitignore(&path.canonical).await;
        }
        Ok(())
    }

    async fn handle_create_dir(&self, path: WatcherEventPath) -> io::Result<()> {
        let dir_entries = self.entry_manager.build_dir(path.canonical).await?;

        for (relative, info) in dir_entries {
            self.entry_manager
                .entry_created(&relative, info.kind.clone(), info.hash.clone())
                .await?;
            self.send_metadata(info).await;
        }
        Ok(())
    }

    async fn handle_modify_file(&self, path: WatcherEventPath, file: EntryInfo) -> io::Result<()> {
        let disk_hash = Some(compute_hash(&path.canonical).await?);

        if file.hash != disk_hash {
            let file = self.entry_manager.entry_modified(file, disk_hash).await?;
            self.send_metadata(file).await;

            if path.relative.ends_with(".gitignore") {
                self.entry_manager.insert_gitignore(&path.canonical).await;
            }
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(path = %path.relative))]
    async fn handle_entry_remove(&self, path: WatcherEventPath) -> io::Result<()> {
        // Issue #40 (B5): skip the removal the synchronizer itself
        // triggered while applying a peer tombstone. Without this guard
        // the watcher re-bumps the local counter and re-broadcasts the
        // tombstone as a local delete.
        if self.state.is_remote_write_in_progress(&path.relative).await {
            return Ok(());
        }

        if let Some(removed) = self.entry_manager.remove_entry(&path.relative).await? {
            if !removed.is_file() {
                let removed_entries = self.entry_manager.remove_dir(&path.relative).await?;

                for entry in removed_entries {
                    if entry.name.ends_with(".gitignore") {
                        self.entry_manager.remove_gitignore(&entry.name).await;
                    }

                    self.send_metadata(entry).await;
                }
            }

            if removed.name.ends_with(".gitignore") {
                self.entry_manager.remove_gitignore(&removed.name).await;
            }

            self.send_metadata(removed).await;
        }
        Ok(())
    }

    async fn send_metadata(&self, file: EntryInfo) {
        if let Err(err) = self
            .sender_tx
            .send(TransportChannelData::Metadata(file))
            .await
        {
            error!("Failed to buffer metadata {}", err);
        }
    }

    #[tracing::instrument(skip_all)]
    async fn handle_config_modify(&self) -> io::Result<()> {
        let new_config = Config::init(self.state.dirs()).await?;

        if new_config.home_path != *self.state.home_path() {
            let path_str = new_config.home_path.display().to_string();
            match self.state.validate_home_path(&path_str).await {
                Ok(_) => {
                    return Err(io::Error::other(format!(
                        "HOME_PATH_CHANGED:{}:{}",
                        self.state.home_path().display(),
                        new_config.home_path.display()
                    )));
                }
                Err(e) => {
                    warn!(
                        "Invalid home_path '{}' in config.toml, ignoring change: {}",
                        path_str, e
                    );
                    return Ok(());
                }
            }
        }

        let current_dirs: HashSet<RelativePath> = self
            .entry_manager
            .list_dirs()
            .await
            .keys()
            .cloned()
            .collect();

        let new_dirs: HashSet<RelativePath> = new_config
            .directory
            .iter()
            .map(|d| d.name.clone())
            .collect();

        if new_dirs == current_dirs {
            info!("Config modified but sync directories unchanged");
            return Ok(());
        }

        let added: Vec<RelativePath> = new_dirs.difference(&current_dirs).cloned().collect();
        let removed: Vec<RelativePath> = current_dirs.difference(&new_dirs).cloned().collect();

        for dir in removed {
            trace!("Config change: removing sync dir {dir:?}");
            if let Err(e) = self.remove_sync_dir(&dir).await {
                error!("Failed to remove sync dir {dir:?}: {e}");
            }
        }

        for dir in added {
            trace!("Config change: adding sync dir {dir:?}");
            if let Err(e) = self.add_sync_dir(dir.clone()).await {
                error!("Failed to add sync dir {dir:?}: {e}");
            }
        }

        self.resync_all_peers().await
    }

    async fn add_sync_dir(&self, name: RelativePath) -> io::Result<()> {
        self.entry_manager.add_sync_dir(name.clone()).await?;
        info!("Sync dir added: {name:?}");
        let _ = self
            .state
            .sse_sender()
            .send(ServerEvent::SyncDirectoryAdded(name));
        Ok(())
    }

    async fn remove_sync_dir(&self, name: &RelativePath) -> io::Result<bool> {
        if self.entry_manager.remove_sync_dir(name).await? {
            info!("Sync dir removed: {name:?}");
            let _ = self
                .state
                .sse_sender()
                .send(ServerEvent::SyncDirectoryRemoved(name.to_owned()));
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn resync_all_peers(&self) -> io::Result<()> {
        let peers = self.peer_manager.list().await;
        let peer_count = peers.len();

        for peer in peers {
            self.sender_tx
                .send(TransportChannelData::HandshakeSyn(peer.addr))
                .await
                .map_err(|e| io::Error::other(e.to_string()))?;
        }

        info!("Resync triggered with {} peer(s)", peer_count);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::CanonicalPath, infra::persistence::sqlite::SqliteDb,
        utils::test_support::test_env_with_dirs,
    };
    use std::{
        collections::{HashMap, VecDeque},
        sync::Mutex,
        time::Duration,
    };
    use tokio::sync::mpsc;

    /// Minimal adapter: the tests drive the `handle_*` methods directly,
    /// so the event streams are never polled.
    struct NoopWatcher;

    impl FileWatcherInterface for NoopWatcher {
        fn new(_state: Arc<AppState>) -> Self {
            NoopWatcher
        }
        async fn watch_home(&mut self) -> io::Result<()> {
            Ok(())
        }
        async fn watch_config(&mut self) -> io::Result<()> {
            Ok(())
        }
        async fn next_home_event(&self) -> io::Result<Option<HomeWatcherEvent>> {
            Ok(None)
        }
        async fn next_config_event(&self) -> io::Result<Option<crate::domain::ConfigWatcherEvent>> {
            Ok(None)
        }
    }

    struct QueueWatcher {
        home_events: Mutex<VecDeque<HomeWatcherEvent>>,
    }

    impl QueueWatcher {
        fn with_home_events(events: Vec<HomeWatcherEvent>) -> Self {
            Self {
                home_events: Mutex::new(events.into()),
            }
        }
    }

    impl FileWatcherInterface for QueueWatcher {
        fn new(_state: Arc<AppState>) -> Self {
            Self::with_home_events(Vec::new())
        }
        async fn watch_home(&mut self) -> io::Result<()> {
            Ok(())
        }
        async fn watch_config(&mut self) -> io::Result<()> {
            Ok(())
        }
        async fn next_home_event(&self) -> io::Result<Option<HomeWatcherEvent>> {
            Ok(self.home_events.lock().unwrap().pop_front())
        }
        async fn next_config_event(&self) -> io::Result<Option<crate::domain::ConfigWatcherEvent>> {
            Ok(None)
        }
    }

    async fn setup() -> (
        crate::utils::test_support::TestEnv,
        FileWatcher<NoopWatcher, SqliteDb>,
        Arc<EntryManager<SqliteDb>>,
        mpsc::Receiver<TransportChannelData>,
    ) {
        let env = test_env_with_dirs(&["sync"]).await;
        let state = env.state.clone();
        let db = SqliteDb::new(":memory:").await.unwrap();
        let entry_manager = EntryManager::new(db, state.clone());
        let peer_manager = PeerManager::new(state.clone());
        let (sender_tx, sender_rx) = mpsc::channel(8);
        let watcher = FileWatcher::new(
            NoopWatcher::new(state.clone()),
            state,
            peer_manager,
            entry_manager.clone(),
            sender_tx,
        );
        (env, watcher, entry_manager, sender_rx)
    }

    /// Issue #40 (B5): while a path is marked remote-write-in-progress,
    /// a watcher modify event for it must be a no-op — no local counter
    /// bump, no outbound Metadata. Clearing the mark restores normal
    /// behavior.
    #[tokio::test]
    async fn modify_event_is_skipped_while_remote_write_in_progress() {
        let (env, watcher, entry_manager, mut sender_rx) = setup().await;
        let local_id = env.state.local_id();
        let name: RelativePath = "sync/payload.bin".into();

        let sync_dir = env.home_path().join("sync");
        tokio::fs::create_dir_all(&sync_dir).await.unwrap();
        let file = sync_dir.join("payload.bin");
        tokio::fs::write(&file, b"new on-disk bytes").await.unwrap();

        // DB row with a hash that differs from disk, so an unguarded
        // modify event would bump the counter and broadcast.
        entry_manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("stale-db-hash".into()),
                version: HashMap::from([(local_id, 3)]),
            })
            .await
            .unwrap();

        let path = WatcherEventPath {
            relative: name.clone(),
            canonical: CanonicalPath::from_absolute(&file),
        };

        env.state.mark_remote_write(&name).await;
        watcher
            .handle_entry_create_or_modify(path.clone())
            .await
            .unwrap();

        assert!(
            sender_rx.try_recv().is_err(),
            "marked path must not broadcast Metadata"
        );
        let stored = entry_manager.get_entry(&name).await.unwrap().unwrap();
        assert_eq!(
            stored.version.get(&local_id),
            Some(&3),
            "local counter must not be bumped for a remote write"
        );
        assert_eq!(stored.hash.as_deref(), Some("stale-db-hash"));

        // Once the mark is cleared, the same event is handled normally.
        env.state.clear_remote_write(&name).await;
        watcher.handle_entry_create_or_modify(path).await.unwrap();

        match sender_rx.try_recv().expect("expected Metadata after clear") {
            TransportChannelData::Metadata(entry) => {
                assert_eq!(entry.name, name);
                assert_eq!(entry.version.get(&local_id), Some(&4));
            }
            _ => panic!("unexpected outbound message"),
        }
    }

    /// Issue #40 (B5): a watcher remove event for a path the
    /// synchronizer is removing (peer tombstone) must be skipped so it
    /// does not re-bump the local counter and re-broadcast a tombstone.
    #[tokio::test]
    async fn remove_event_is_skipped_while_remote_write_in_progress() {
        let (env, watcher, entry_manager, mut sender_rx) = setup().await;
        let local_id = env.state.local_id();
        let name: RelativePath = "sync/gone.txt".into();

        entry_manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("hash".into()),
                version: HashMap::from([(local_id, 1)]),
            })
            .await
            .unwrap();

        let path = WatcherEventPath {
            relative: name.clone(),
            canonical: CanonicalPath::from_absolute(env.home_path().join("sync/gone.txt")),
        };

        env.state.mark_remote_write(&name).await;
        watcher.handle_entry_remove(path).await.unwrap();

        assert!(
            sender_rx.try_recv().is_err(),
            "marked path must not broadcast a tombstone"
        );
        let stored = entry_manager.get_entry(&name).await.unwrap().unwrap();
        assert!(
            !stored.is_removed(),
            "marked remove must not turn the entry into a local tombstone"
        );
        assert_eq!(stored.version.get(&local_id), Some(&1));
    }

    /// Issue #40 (B5): suppression must happen before the event enters
    /// the debounce buffer. Otherwise the remote-write mark can be
    /// cleared before the debounced event is handled, reintroducing the
    /// stale-DB race.
    #[tokio::test]
    async fn adapter_event_is_dropped_before_debounce_while_remote_write_in_progress() {
        let env = test_env_with_dirs(&["sync"]).await;
        let state = env.state.clone();
        let db = SqliteDb::new(":memory:").await.unwrap();
        let entry_manager = EntryManager::new(db, state.clone());
        let peer_manager = PeerManager::new(state.clone());
        let (sender_tx, mut sender_rx) = mpsc::channel(8);

        let name: RelativePath = "sync/payload.bin".into();
        let local_id = state.local_id();
        let sync_dir = env.home_path().join("sync");
        tokio::fs::create_dir_all(&sync_dir).await.unwrap();
        let file = sync_dir.join("payload.bin");
        tokio::fs::write(&file, b"remote bytes").await.unwrap();

        entry_manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("stale-db-hash".into()),
                version: HashMap::from([(local_id, 7)]),
            })
            .await
            .unwrap();

        let path = WatcherEventPath {
            relative: name.clone(),
            canonical: CanonicalPath::from_absolute(&file),
        };
        let watcher = Arc::new(FileWatcher::new(
            QueueWatcher::with_home_events(vec![HomeWatcherEvent::EntryCreateOrModify(path)]),
            state.clone(),
            peer_manager,
            entry_manager.clone(),
            sender_tx,
        ));

        state.mark_remote_write(&name).await;
        watcher.recv_adapter_home_events().await.unwrap();
        state.clear_remote_write(&name).await;

        let buffer_watcher = watcher.clone();
        let buffer_task = tokio::spawn(async move { buffer_watcher.buffer.run().await });

        assert!(
            tokio::time::timeout(
                Duration::from_millis(1200),
                watcher.buffer.next_home_event()
            )
            .await
            .is_err(),
            "marked adapter event must not survive in debounce buffer"
        );
        buffer_task.abort();

        assert!(
            sender_rx.try_recv().is_err(),
            "dropped adapter event must not broadcast Metadata"
        );
        let stored = entry_manager.get_entry(&name).await.unwrap().unwrap();
        assert_eq!(stored.version.get(&local_id), Some(&7));
        assert_eq!(stored.hash.as_deref(), Some("stale-db-hash"));
    }
}
