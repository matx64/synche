use crate::domain::FileInfo;

pub trait PersistenceInterface {
    fn insert_or_replace_file(&self, file: &FileInfo) -> PersistenceResult<()>;
    fn get_file(&self, name: &str) -> PersistenceResult<Option<FileInfo>>;
    fn list_all_files(&self) -> PersistenceResult<Vec<FileInfo>>;
    fn remove_file(&self, name: &str) -> PersistenceResult<Option<FileInfo>>;
}

pub type PersistenceResult<T, E = PersistenceError> = Result<T, E>;

#[derive(Debug)]
pub enum PersistenceError {
    Failure(String),
}
