use crate::domain::filesystem::WatcherEvent;
use std::path::PathBuf;
use tokio::io;

pub trait FileWatcherInterface {
    async fn watch(&mut self, base_dir: PathBuf, dirs: Vec<PathBuf>) -> io::Result<()>;
    async fn next(&mut self) -> Option<WatcherEvent>;
}
