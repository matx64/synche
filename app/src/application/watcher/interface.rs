use crate::domain::{AppState, WatcherEvent};
use std::sync::Arc;
use tokio::io;

pub trait FileWatcherInterface {
    fn new(state: Arc<AppState>) -> Self;

    async fn watch(&mut self) -> io::Result<()>;

    async fn next(&mut self) -> Option<WatcherEvent>;
}
