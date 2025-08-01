use std::path::PathBuf;

#[derive(Debug)]
pub enum WatcherEvent {
    CreatedFile(WatcherEventPath),
    CreatedDir(WatcherEventPath),
    ModifiedFileContent(WatcherEventPath),
    RenamedFile(ModifiedNamePaths),
    RenamedDir(ModifiedNamePaths),
    RenamedSyncDir(ModifiedNamePaths),
    Removed(WatcherEventPath),
}

#[derive(Debug)]
pub struct WatcherEventPath {
    pub absolute: PathBuf,
    pub relative: String,
}

#[derive(Debug)]
pub struct ModifiedNamePaths {
    pub from: WatcherEventPath,
    pub to: WatcherEventPath,
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
