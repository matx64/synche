use crate::domain::{ConfigWatcherEvent, HomeWatcherEvent, MutexChannel, RelativePath};
use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};
use tokio::{io, sync::RwLock};

const DEBOUNCE_DURATION: Duration = Duration::from_secs(1);

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
