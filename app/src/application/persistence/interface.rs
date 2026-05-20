use tokio::io;

use crate::domain::EntryInfo;

/// Port for entry-metadata persistence.
///
/// Implementations store and retrieve `EntryInfo` keyed by their
/// `RelativePath` string. The interface is intentionally small —
/// callers never query or mutate version vectors directly; they
/// `insert_or_replace_entry` after merging in memory.
///
/// All methods are `async` because the default adapter (`SqliteDb`)
/// goes over a real I/O boundary; implementations must be safe to call
/// concurrently from multiple tasks.
#[async_trait::async_trait]
pub trait PersistenceInterface: Send + Sync + 'static {
    /// Inserts a new entry or replaces an existing one with the same name.
    async fn insert_or_replace_entry(&self, entry: &EntryInfo) -> PersistenceResult<()>;
    /// Looks up an entry by its relative-path name, returning `None`
    /// if absent.
    async fn get_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>>;
    /// Returns every persisted entry. Used at startup to rehydrate the
    /// in-memory view.
    async fn list_all_entries(&self) -> PersistenceResult<Vec<EntryInfo>>;
    /// Deletes an entry by name. Deleting a missing entry must not error.
    async fn delete_entry(&self, name: &str) -> PersistenceResult<()>;
}

/// Result alias for fallible persistence calls.
pub type PersistenceResult<T> = Result<T, PersistenceError>;

/// Error returned by `PersistenceInterface` implementors.
///
/// Currently a single opaque variant — error categorization is the
/// adapter's responsibility, and the surrounding `io::Error` produced
/// by the `From` impl is what callers actually propagate.
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
