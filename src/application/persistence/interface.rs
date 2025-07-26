use crate::domain::FileInfo;

pub trait PersistenceInterface {
    fn insert_or_replace_file(&self, file: &FileInfo) -> PersistenceResult<()>;
    fn get_file(&self, name: &str) -> PersistenceResult<Option<FileInfo>>;
}

pub type PersistenceResult<T, E = PersistenceError> = Result<T, E>;

pub enum PersistenceError {
    Failure(String),
}
