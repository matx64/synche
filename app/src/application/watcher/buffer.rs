use crate::domain::{ConfigWatcherEvent, HomeWatcherEvent, MutexChannel, RelativePath};
use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};
use tokio::{io, sync::RwLock};

const DEBOUNCE_DURATION: Duration = Duration::from_secs(1);

/// In-memory debounce buffer that coalesces bursty filesystem events
/// into a single settled event per path.
///
/// Home events are debounced per `RelativePath` so rapid writes to the
/// same file collapse to one downstream message; config events are
/// debounced as a single slot because `config.toml` is monolithic. An
/// event is flushed once `DEBOUNCE_DURATION` has elapsed since the
/// last write for that key.
pub struct WatcherBuffer {
    home_chan: MutexChannel<HomeWatcherEvent>,
    config_chan: MutexChannel<ConfigWatcherEvent>,
    home_events: RwLock<HashMap<RelativePath, DebounceState<HomeWatcherEvent>>>,
    config_events: RwLock<Option<DebounceState<ConfigWatcherEvent>>>,
}

struct DebounceState<E> {
    last_event: E,
    last_event_at: SystemTime,
}

impl Default for WatcherBuffer {
    fn default() -> Self {
        Self {
            home_events: Default::default(),
            config_events: Default::default(),
            home_chan: MutexChannel::new(100),
            config_chan: MutexChannel::new(100),
        }
    }
}

impl WatcherBuffer {
    pub async fn run(&self) -> io::Result<()> {
        loop {
            let mut home_ready = Vec::new();
            let now = SystemTime::now();

            {
                let home_events = self.home_events.read().await;

                for (path, event) in home_events.iter() {
                    if let Ok(elapsed) = now.duration_since(event.last_event_at)
                        && elapsed >= DEBOUNCE_DURATION
                    {
                        home_ready.push(path.clone());
                    }
                }
            }

            {
                let mut home_events = self.home_events.write().await;

                for path in home_ready {
                    if let Some(removed) = home_events.remove(&path) {
                        self.home_chan
                            .tx
                            .send(removed.last_event)
                            .await
                            .map_err(io::Error::other)?;
                    }
                }
            }

            {
                let mut config_guard = self.config_events.write().await;

                if let Some(state) = config_guard.as_mut()
                    && let Ok(elapsed) = now.duration_since(state.last_event_at)
                    && elapsed >= DEBOUNCE_DURATION
                {
                    self.config_chan
                        .tx
                        .send(state.last_event.clone())
                        .await
                        .map_err(io::Error::other)?;
                    *config_guard = None;
                }
            }

            tokio::time::sleep(DEBOUNCE_DURATION).await;
        }
    }

    pub async fn next_home_event(&self) -> Option<HomeWatcherEvent> {
        self.home_chan.recv().await
    }

    pub async fn next_config_event(&self) -> Option<ConfigWatcherEvent> {
        self.config_chan.recv().await
    }

    pub async fn insert_home_event(&self, event: HomeWatcherEvent) {
        self.home_events.write().await.insert(
            event.path().relative.clone(),
            DebounceState {
                last_event: event,
                last_event_at: SystemTime::now(),
            },
        );
    }

    pub async fn insert_config_event(&self, event: ConfigWatcherEvent) {
        let mut guard = self.config_events.write().await;
        *guard = Some(DebounceState {
            last_event: event,
            last_event_at: SystemTime::now(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        CanonicalPath, ConfigWatcherEvent, HomeWatcherEvent, RelativePath, WatcherEventPath,
    };
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::time::sleep;

    fn create_test_event(name: &str, temp_dir: &TempDir) -> HomeWatcherEvent {
        let path = temp_dir.path().join(name);
        std::fs::write(&path, "test").unwrap();

        HomeWatcherEvent::EntryCreateOrModify(WatcherEventPath {
            relative: RelativePath::from(name),
            canonical: CanonicalPath::new(&path).unwrap(),
        })
    }

    #[tokio::test]
    async fn test_debounce_single_event() {
        let buffer = Arc::new(WatcherBuffer::default());
        let temp = TempDir::new().unwrap();

        let buffer_clone = buffer.clone();
        tokio::spawn(async move {
            let _ = buffer_clone.run().await;
        });

        let event = create_test_event("file.txt", &temp);
        buffer.insert_home_event(event.clone()).await;

        let result = tokio::time::timeout(DEBOUNCE_DURATION / 2, buffer.next_home_event()).await;
        assert!(
            result.is_err(),
            "Event should not be delivered before debounce window"
        );

        sleep(DEBOUNCE_DURATION + Duration::from_millis(100)).await;

        let received = buffer.next_home_event().await;
        assert!(
            received.is_some(),
            "Event should be delivered after debounce window"
        );

        if let Some(HomeWatcherEvent::EntryCreateOrModify(path)) = received {
            assert_eq!(
                <RelativePath as AsRef<str>>::as_ref(&path.relative),
                "file.txt"
            );
        } else {
            panic!("Expected EntryCreateOrModify event");
        }
    }

    #[tokio::test]
    async fn test_debounce_rapid_fire_same_path() {
        let buffer = Arc::new(WatcherBuffer::default());
        let temp = TempDir::new().unwrap();

        let buffer_clone = buffer.clone();
        tokio::spawn(async move {
            let _ = buffer_clone.run().await;
        });

        let path = temp.path().join("file.txt");
        std::fs::write(&path, "initial").unwrap();

        for i in 0..5 {
            std::fs::write(&path, format!("version{}", i)).unwrap();
            let event = HomeWatcherEvent::EntryCreateOrModify(WatcherEventPath {
                relative: RelativePath::from("file.txt"),
                canonical: CanonicalPath::new(&path).unwrap(),
            });
            buffer.insert_home_event(event).await;
            sleep(DEBOUNCE_DURATION / 10).await;
        }

        sleep(DEBOUNCE_DURATION * 2).await;

        let received = buffer.next_home_event().await;
        assert!(received.is_some(), "Should receive one debounced event");

        let result = tokio::time::timeout(DEBOUNCE_DURATION / 10, buffer.next_home_event()).await;
        assert!(result.is_err(), "Should not receive duplicate events");
    }

    #[tokio::test]
    async fn test_debounce_different_paths() {
        let buffer = Arc::new(WatcherBuffer::default());
        let temp = TempDir::new().unwrap();

        let buffer_clone = buffer.clone();
        tokio::spawn(async move {
            let _ = buffer_clone.run().await;
        });

        let event1 = create_test_event("file1.txt", &temp);
        let event2 = create_test_event("file2.txt", &temp);
        let event3 = create_test_event("file3.txt", &temp);

        buffer.insert_home_event(event1).await;
        buffer.insert_home_event(event2).await;
        buffer.insert_home_event(event3).await;

        sleep(DEBOUNCE_DURATION + Duration::from_millis(200)).await;

        let mut received_paths = vec![];
        for _ in 0..3 {
            if let Some(HomeWatcherEvent::EntryCreateOrModify(path)) =
                buffer.next_home_event().await
            {
                received_paths.push(path.relative.to_string());
            }
        }

        received_paths.sort();
        assert_eq!(received_paths, vec!["file1.txt", "file2.txt", "file3.txt"]);

        let result = tokio::time::timeout(DEBOUNCE_DURATION / 10, buffer.next_home_event()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_config_event_debounce() {
        let buffer = Arc::new(WatcherBuffer::default());
        let temp = TempDir::new().unwrap();

        let buffer_clone = buffer.clone();
        tokio::spawn(async move {
            let _ = buffer_clone.run().await;
        });

        let config_path = temp.path().join("config.toml");
        std::fs::write(&config_path, "test1").unwrap();

        for i in 0..3 {
            std::fs::write(&config_path, format!("test{}", i)).unwrap();
            let event = ConfigWatcherEvent::Modify;
            buffer.insert_config_event(event).await;
            sleep(DEBOUNCE_DURATION / 5).await;
        }

        sleep(DEBOUNCE_DURATION + Duration::from_millis(200)).await;

        let received = buffer.next_config_event().await;
        assert!(received.is_some(), "Should receive one config event");

        let result = tokio::time::timeout(DEBOUNCE_DURATION / 10, buffer.next_config_event()).await;
        assert!(
            result.is_err(),
            "Should not receive duplicate config events"
        );
    }

    #[tokio::test]
    async fn test_debounce_mixed_event_types_same_path() {
        let buffer = Arc::new(WatcherBuffer::default());
        let temp = TempDir::new().unwrap();

        let buffer_clone = buffer.clone();
        tokio::spawn(async move {
            let _ = buffer_clone.run().await;
        });

        let path = temp.path().join("file.txt");
        std::fs::write(&path, "test").unwrap();

        let watcher_path = WatcherEventPath {
            relative: RelativePath::from("file.txt"),
            canonical: CanonicalPath::new(&path).unwrap(),
        };

        buffer
            .insert_home_event(HomeWatcherEvent::EntryCreateOrModify(watcher_path.clone()))
            .await;
        sleep(DEBOUNCE_DURATION / 5).await;
        buffer
            .insert_home_event(HomeWatcherEvent::EntryRemove(watcher_path.clone()))
            .await;

        sleep(DEBOUNCE_DURATION + Duration::from_millis(200)).await;

        let received = buffer.next_home_event().await;
        assert!(received.is_some());
        assert!(matches!(received, Some(HomeWatcherEvent::EntryRemove(_))));

        let result = tokio::time::timeout(DEBOUNCE_DURATION / 10, buffer.next_home_event()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_debounce_update_resets_timer() {
        let buffer = Arc::new(WatcherBuffer::default());
        let temp = TempDir::new().unwrap();

        let buffer_clone = buffer.clone();
        tokio::spawn(async move {
            let _ = buffer_clone.run().await;
        });

        let event = create_test_event("file.txt", &temp);

        buffer.insert_home_event(event.clone()).await;

        // Wait 80% of debounce duration (not complete)
        sleep(DEBOUNCE_DURATION * 4 / 5).await;

        // Insert another event for same path (resets timer)
        buffer.insert_home_event(event.clone()).await;

        // Wait another 80% (total 160% from first, but only 80% from second)
        sleep(DEBOUNCE_DURATION * 4 / 5).await;

        let result = tokio::time::timeout(DEBOUNCE_DURATION / 10, buffer.next_home_event()).await;
        assert!(result.is_err(), "Timer should have been reset");

        // Wait remaining time (30% more to complete the debounce)
        sleep(DEBOUNCE_DURATION * 3 / 10).await;

        let received = buffer.next_home_event().await;
        assert!(received.is_some());
    }
}
