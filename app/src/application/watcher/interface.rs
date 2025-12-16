use crate::{
    application::AppState,
    domain::{ConfigWatcherEvent, HomeWatcherEvent},
};
use std::sync::Arc;
use tokio::io;

pub trait FileWatcherInterface {
    fn new(state: Arc<AppState>) -> Self;

    async fn watch_home(&mut self) -> io::Result<()>;
    async fn watch_config(&mut self) -> io::Result<()>;

    async fn next_home_event(&self) -> io::Result<Option<HomeWatcherEvent>>;
    async fn next_config_event(&self) -> io::Result<Option<ConfigWatcherEvent>>;
}
