use crate::{
    application::persistence::interface::{PersistenceInterface, PersistenceResult},
    domain::EntryInfo,
};
use sqlx::{Error, Executor, Pool, Sqlite, SqlitePool};
use std::path::Path;

pub struct SqliteDb {
    pool: Pool<Sqlite>,
}

impl SqliteDb {
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let path = path.as_ref().to_str().unwrap();

        let uri = if path.starts_with("sqlite:") {
            path.to_string()
        } else {
            format!("sqlite://{}", path)
        };

        let pool = SqlitePool::connect(&uri).await?;

        pool.execute(
            "CREATE TABLE IF NOT EXISTS entries (
                name TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                hash TEXT,
                version TEXT NOT NULL
            )",
        )
        .await?;

        Ok(Self { pool })
    }
}

impl PersistenceInterface for SqliteDb {
    async fn insert_or_replace_entry(&self, entry: &EntryInfo) -> PersistenceResult<()> {
        todo!()
    }

    async fn get_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>> {}

    async fn list_all_entries(&self) -> PersistenceResult<Vec<EntryInfo>> {
        todo!()
    }

    async fn delete_entry(&self, name: &str) -> PersistenceResult<()> {
        todo!()
    }
}
