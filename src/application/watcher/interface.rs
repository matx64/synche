use crate::domain::{CanonicalPath, watcher::WatcherEvent};
use std::path::PathBuf;
use tokio::io;

pub trait FileWatcherInterface {
    async fn watch(&mut self, base_dir_path: CanonicalPath, dirs: Vec<PathBuf>) -> io::Result<()>;
    async fn next(&mut self) -> Option<WatcherEvent>;
}
