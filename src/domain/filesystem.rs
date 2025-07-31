use std::path::PathBuf;

#[derive(Debug)]
pub enum WatcherEvent {
    CreatedFile(PathBuf),
    CreatedDir(PathBuf),
    ModifiedFileContent(PathBuf),
    RenamedFile(ModifiedNamePaths),
    RenamedDir(ModifiedNamePaths),
    RenamedSyncDir(ModifiedNamePaths),
    Removed(PathBuf),
}

#[derive(Debug)]
pub struct ModifiedNamePaths {
    pub from: PathBuf,
    pub to: PathBuf,
}
