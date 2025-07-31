use crate::domain::EntryInfo;

pub trait PersistenceInterface {
    fn insert_or_replace_entry(&self, entry: &EntryInfo) -> PersistenceResult<()>;
    fn get_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>>;
    fn list_all_entries(&self) -> PersistenceResult<Vec<EntryInfo>>;
    fn remove_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>>;
}

pub type PersistenceResult<T, E = PersistenceError> = Result<T, E>;

#[derive(Debug)]
pub enum PersistenceError {
    Failure(String),
}
