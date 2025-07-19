use crate::domain::filesystem::FileChangeEvent;
use std::path::PathBuf;
use tokio::io;

pub trait FileWatcherInterface {
    async fn watch(&mut self, dirs: Vec<PathBuf>) -> io::Result<()>;
    async fn next(&mut self) -> Option<FileChangeEvent>;
}
