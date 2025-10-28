use tokio::io;

use crate::domain::EntryInfo;

#[async_trait::async_trait]
pub trait PersistenceInterface: Send + Sync + 'static {
    async fn insert_or_replace_entry(&self, entry: &EntryInfo) -> PersistenceResult<()>;
    async fn get_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>>;
    async fn list_all_entries(&self) -> PersistenceResult<Vec<EntryInfo>>;
    async fn delete_entry(&self, name: &str) -> PersistenceResult<()>;
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

impl From<PersistenceError> for io::Error {
    fn from(value: PersistenceError) -> Self {
        Self::other(value.to_string())
    }
}
