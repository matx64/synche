use crate::domain::EntryInfo;

pub trait PersistenceInterface {
    fn insert_or_replace_entry(&self, entry: &EntryInfo) -> PersistenceResult<()>;
    fn get_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>>;
    fn list_all_entries(&self, include_deleted: bool) -> PersistenceResult<Vec<EntryInfo>>;
    fn remove_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>>;
    fn clean_deleted_entries(&self) -> PersistenceResult<()>;
}

pub type PersistenceResult<T, E = PersistenceError> = Result<T, E>;

#[derive(Debug)]
pub enum PersistenceError {
    Failure(String),
}
