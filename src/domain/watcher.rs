use std::path::PathBuf;

#[derive(Debug)]
pub struct WatcherEvent {
    pub kind: WatcherEventKind,
    pub path: WatcherEventPath,
}

impl WatcherEvent {
    pub fn new(kind: WatcherEventKind, path: WatcherEventPath) -> Self {
        Self { kind, path }
    }
}

#[derive(Debug)]
pub enum WatcherEventKind {
    CreatedFile,
    CreatedDir,
    ModifiedAny,
    ModifiedFileContent,
    Removed,
}

#[derive(Debug)]
pub struct WatcherEventPath {
    pub absolute: PathBuf,
    pub relative: String,
}

impl WatcherEventPath {
    pub fn _exists(&self) -> bool {
        self.absolute.exists()
    }

    pub fn is_file(&self) -> bool {
        self.absolute.is_file()
    }

    pub fn is_dir(&self) -> bool {
        self.absolute.is_dir()
    }
}
