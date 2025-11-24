#[derive(Debug, Clone)]
pub enum ConfigWatcherEvent {
    Modify,
    Remove,
}
