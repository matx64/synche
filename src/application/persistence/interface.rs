use crate::domain::EntryInfo;

pub trait PersistenceInterface {
    fn insert_or_replace_entry(&self, entry: &EntryInfo) -> PersistenceResult<()>;
    fn get_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>>;
    fn list_all_entries(&self, include_deleted: bool) -> PersistenceResult<Vec<EntryInfo>>;
    fn delete_entry(&self, name: &str) -> PersistenceResult<()>;
    fn clean_removed_entries(&self) -> PersistenceResult<()>;
}

pub type PersistenceResult<T> = Result<T, PersistenceError>;

#[derive(Debug)]
pub enum PersistenceError {
    Failure(String),
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistenceError::Failure(s) => f.write_str(s),
        }
    }
}
