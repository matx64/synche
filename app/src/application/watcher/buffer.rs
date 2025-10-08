use crate::domain::{RelativePath, watcher::WatcherEvent};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::{Mutex, mpsc::Sender};

pub struct WatcherBuffer {
    items: Arc<Mutex<HashMap<RelativePath, BufferItem>>>,
}

struct BufferItem {
    events: Vec<WatcherEvent>,
    last_event_at: SystemTime,
}

impl WatcherBuffer {
    pub fn new(watch_tx: Sender<WatcherEvent>) -> Self {
        let debounce = Duration::from_secs(1);

        let items: Arc<Mutex<HashMap<RelativePath, BufferItem>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let items_clone = items.clone();

        tokio::spawn(async move {
            loop {
                {
                    let mut items = items_clone.lock().await;

                    let now = SystemTime::now();
                    let mut ready = Vec::new();

                    for (entry_name, item) in items.iter() {
                        if now.duration_since(item.last_event_at).unwrap_or_default() >= debounce {
                            ready.push(entry_name.clone());
                        }
                    }

                    for entry_name in ready {
                        if let Some(removed) = items.remove(&entry_name)
                            && let Some(last_event) = removed.events.last().cloned()
                        {
                            watch_tx.send(last_event).await.unwrap();
                        }
                    }
                }
                tokio::time::sleep(debounce).await;
            }
        });

        Self { items }
    }

    pub async fn insert(&self, event: WatcherEvent) {
        let mut items = self.items.lock().await;
        if let Some(item) = items.get_mut(&event.path.relative) {
            item.last_event_at = SystemTime::now();
            item.events.push(event);
        } else {
            items.insert(
                event.path.relative.clone(),
                BufferItem {
                    events: vec![event],
                    last_event_at: SystemTime::now(),
                },
            );
        }
    }
}
