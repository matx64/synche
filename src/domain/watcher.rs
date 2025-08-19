use crate::domain::{CanonicalPath, RelativePath};

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
    pub canonical: CanonicalPath,
    pub relative: RelativePath,
}

impl WatcherEventPath {
    pub fn is_file(&self) -> bool {
        self.canonical.is_file()
    }
}
