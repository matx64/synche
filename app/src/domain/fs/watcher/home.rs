use crate::domain::WatcherEventPath;

#[derive(Debug, Clone)]
pub enum HomeWatcherEvent {
    EntryCreateOrModify(WatcherEventPath),
    EntryRemove(WatcherEventPath),
    SyncDirectoryRemove(WatcherEventPath),
}

impl HomeWatcherEvent {
    pub fn path(&self) -> &WatcherEventPath {
        match self {
            Self::EntryCreateOrModify(p) | Self::EntryRemove(p) | Self::SyncDirectoryRemove(p) => p,
        }
    }
}
