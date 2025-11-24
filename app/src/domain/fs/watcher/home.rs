use crate::domain::WatcherEventPath;

#[derive(Debug, Clone)]
pub enum HomeWatcherEvent {
    CreateOrModify(WatcherEventPath),
    Remove(WatcherEventPath),
}

impl HomeWatcherEvent {
    pub fn path(&self) -> &WatcherEventPath {
        match self {
            Self::CreateOrModify(p) | Self::Remove(p) => p,
        }
    }
}
