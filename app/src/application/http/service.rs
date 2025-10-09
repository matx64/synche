use crate::application::{EntryManager, persistence::interface::PersistenceInterface};
use std::sync::Arc;

pub struct HttpService<D: PersistenceInterface> {
    entry_manager: Arc<EntryManager<D>>,
}

impl<D: PersistenceInterface> HttpService<D> {
    pub fn new(entry_manager: Arc<EntryManager<D>>) -> Self {
        Self { entry_manager }
    }

    pub fn add_folder() {}

    pub fn remove_folder() {}

    pub fn send_event() {}
}
