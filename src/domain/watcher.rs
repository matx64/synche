use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct WatcherEvent {
    pub kind: WatcherEventKind,
    pub path: WatcherEventPath,
}

impl WatcherEvent {
    pub fn new(kind: WatcherEventKind, path: WatcherEventPath) -> Self {
        Self { kind, path }
    }
}

#[derive(Debug, Clone)]
pub enum WatcherEventKind {
    CreateOrModify,
    Remove,
}

#[derive(Debug, Clone)]
pub struct WatcherEventPath {
    pub absolute: PathBuf,
    pub relative: String,
}

impl WatcherEventPath {
    pub fn is_file(&self) -> bool {
        self.absolute.is_file()
    }
}
