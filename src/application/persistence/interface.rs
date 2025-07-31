use crate::domain::EntryInfo;

pub trait PersistenceInterface {
    fn insert_or_replace_file(&self, file: &EntryInfo) -> PersistenceResult<()>;
    fn get_file(&self, name: &str) -> PersistenceResult<Option<EntryInfo>>;
    fn list_all_files(&self) -> PersistenceResult<Vec<EntryInfo>>;
    fn remove_file(&self, name: &str) -> PersistenceResult<Option<EntryInfo>>;
}

pub type PersistenceResult<T, E = PersistenceError> = Result<T, E>;

#[derive(Debug)]
pub enum PersistenceError {
    Failure(String),
}
