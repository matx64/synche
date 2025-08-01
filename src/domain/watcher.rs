use std::path::PathBuf;

#[derive(Debug)]
pub enum WatcherEvent {
    CreatedFile(WatcherEventPath),
    CreatedDir(WatcherEventPath),
    ModifiedFileContent(WatcherEventPath),
    RenamedFile((WatcherEventPath, WatcherEventPath)),
    RenamedDir((WatcherEventPath, WatcherEventPath)),
    RenamedSyncDir((WatcherEventPath, WatcherEventPath)),
    Removed(WatcherEventPath),
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
