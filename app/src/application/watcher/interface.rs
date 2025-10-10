use crate::domain::{CanonicalPath, WatcherEvent};
use tokio::io;

pub trait FileWatcherInterface {
    async fn watch(
        &mut self,
        base_dir_path: CanonicalPath,
        sync_directories: Vec<CanonicalPath>,
    ) -> io::Result<()>;
    async fn next(&mut self) -> Option<WatcherEvent>;
}
