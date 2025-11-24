use crate::domain::{Channel, ConfigWatcherEvent, HomeWatcherEvent, RelativePath};
use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};
use tokio::{io, sync::RwLock};

const DEBOUNCE_DURATION: Duration = Duration::from_secs(1);

pub struct WatcherBuffer {
    home_chan: Channel<HomeWatcherEvent>,
    config_chan: Channel<ConfigWatcherEvent>,
    home_events: RwLock<HashMap<RelativePath, DebounceState<HomeWatcherEvent>>>,
    config_events: RwLock<HashMap<RelativePath, DebounceState<ConfigWatcherEvent>>>,
}

struct DebounceState<E> {
    last_event: E,
    last_event_at: SystemTime,
}

impl Default for WatcherBuffer {
    fn default() -> Self {
        Self {
            home_chan: Channel::new(100),
            config_chan: Channel::new(100),
            home_events: Default::default(),
            config_events: Default::default(),
        }
    }
}

impl WatcherBuffer {
    pub async fn run(&self) -> io::Result<()> {
        loop {
            let mut home_ready = Vec::new();
            let mut config_ready = Vec::new();

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
                let config_events = self.config_events.read().await;

                for (path, event) in config_events.iter() {
                    if let Ok(elapsed) = now.duration_since(event.last_event_at)
                        && elapsed >= DEBOUNCE_DURATION
                    {
                        config_ready.push(path.clone());
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
                let mut config_events = self.config_events.write().await;

                for path in config_ready {
                    if let Some(removed) = config_events.remove(&path) {
                        self.config_chan
                            .tx
                            .send(removed.last_event)
                            .await
                            .map_err(io::Error::other)?;
                    }
                }
            }

            tokio::time::sleep(DEBOUNCE_DURATION).await;
        }
    }

    pub async fn next_home_event(&self) -> Option<HomeWatcherEvent> {
        self.home_chan.rx.lock().await.recv().await
    }

    pub async fn next_config_event(&self) -> Option<ConfigWatcherEvent> {
        self.config_chan.rx.lock().await.recv().await
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
        self.config_events.write().await.insert(
            event.path().relative.clone(),
            DebounceState {
                last_event: event,
                last_event_at: SystemTime::now(),
            },
        );
    }
}
