use crate::domain::WatcherEventPath;

#[derive(Debug, Clone)]
pub enum ConfigWatcherEvent {
    CreateOrModify(WatcherEventPath),
    Remove(WatcherEventPath),
}

impl ConfigWatcherEvent {
    pub fn path(&self) -> &WatcherEventPath {
        match self {
            Self::CreateOrModify(p) | Self::Remove(p) => p,
        }
    }
}
