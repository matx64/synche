use std::path::PathBuf;

#[derive(Debug)]
pub enum FileChangeEvent {
    Created(PathBuf),
    ModifiedData(PathBuf),
    ModifiedName(PathBuf),
    Deleted(PathBuf),
}
