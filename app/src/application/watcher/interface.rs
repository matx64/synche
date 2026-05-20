use crate::{
    application::AppState,
    domain::{ConfigWatcherEvent, HomeWatcherEvent},
};
use std::sync::Arc;
use tokio::io;

/// Port for filesystem-event observation.
///
/// An implementor must surface two independent event streams:
///
/// - **home** events (entry created/modified/removed) from anywhere
///   under the active `home_path`, used to drive sync;
/// - **config** events from `config.toml` only, used to apply live
///   edits to the user's settings.
///
/// Implementations are expected to debounce rapid bursts so callers
/// receive a settled view rather than per-syscall noise. Both
/// `watch_*` methods are called once at startup; the corresponding
/// `next_*_event` methods are then polled in a loop.
pub trait FileWatcherInterface {
    fn new(state: Arc<AppState>) -> Self;

    /// Starts watching the user's home directory. Idempotent across
    /// repeated calls is not required.
    async fn watch_home(&mut self) -> io::Result<()>;
    /// Starts watching `config.toml`.
    async fn watch_config(&mut self) -> io::Result<()>;

    /// Awaits the next debounced home-tree event, or `None` if the
    /// underlying stream has terminated.
    async fn next_home_event(&self) -> io::Result<Option<HomeWatcherEvent>>;
    /// Awaits the next debounced `config.toml` event, or `None` if the
    /// underlying stream has terminated.
    async fn next_config_event(&self) -> io::Result<Option<ConfigWatcherEvent>>;
}
