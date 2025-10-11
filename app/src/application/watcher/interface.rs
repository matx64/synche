use crate::domain::{CanonicalPath, WatcherEvent};
use tokio::io;

pub trait FileWatcherInterface {
    async fn watch(
        &mut self,
        base_dir_path: CanonicalPath,
        sync_directories: Vec<CanonicalPath>,
    ) -> io::Result<()>;

    async fn next(&mut self) -> Option<WatcherEvent>;

    fn add_sync_dir(&mut self, dir_path: CanonicalPath);
    fn remove_sync_dir(&mut self, dir_path: CanonicalPath);
}

pub struct FileWatcherSyncDirectoryUpdate {
    pub path: CanonicalPath,
    pub kind: FileWatcherSyncDirectoryUpdateKind,
}

pub enum FileWatcherSyncDirectoryUpdateKind {
    Added,
    Removed,
}
