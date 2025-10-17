use crate::{
    application::persistence::interface::{
        PersistenceError, PersistenceInterface, PersistenceResult,
    },
    domain::{EntryInfo, EntryKind},
};
use sqlx::{
    Error, Executor, FromRow, Pool, Row, Sqlite, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteRow},
};
use std::path::Path;

pub struct SqliteDb {
    pool: Pool<Sqlite>,
}

impl SqliteDb {
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true),
        )
        .await?;

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

#[async_trait::async_trait]
impl PersistenceInterface for SqliteDb {
    async fn insert_or_replace_entry(&self, entry: &EntryInfo) -> PersistenceResult<()> {
        let version_json = serde_json::to_string(&entry.version)?;

        sqlx::query(
            "INSERT OR REPLACE INTO entries (name, kind, hash, version) 
                VALUES (?, ?, ?, ?)",
        )
        .bind(&*entry.name)
        .bind(entry.kind.to_string())
        .bind(entry.hash.clone())
        .bind(version_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>> {
        let entry = sqlx::query_as("SELECT * FROM entries WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        Ok(entry)
    }

    async fn list_all_entries(&self) -> PersistenceResult<Vec<EntryInfo>> {
        let entries = sqlx::query_as("SELECT * FROM entries")
            .fetch_all(&self.pool)
            .await?;

        Ok(entries)
    }

    async fn delete_entry(&self, name: &str) -> PersistenceResult<()> {
        sqlx::query("DELETE FROM entries WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

impl std::fmt::Display for EntryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryKind::File => f.write_str("F"),
            EntryKind::Directory => f.write_str("D"),
        }
    }
}

impl FromRow<'_, SqliteRow> for EntryInfo {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        let name: String = row.try_get("name")?;
        let hash: Option<String> = row.try_get("hash")?;

        let version_json: String = row.try_get("version")?;
        let version =
            serde_json::from_str(&version_json).map_err(|err| Error::Decode(Box::new(err)))?;

        let kind_str: String = row.try_get("kind")?;
        let kind = match kind_str.as_str() {
            "F" => EntryKind::File,
            "D" => EntryKind::Directory,
            other => {
                return Err(Error::Decode(
                    format!("Unknown entry kind: {}", other).into(),
                ));
            }
        };

        Ok(EntryInfo {
            name: name.into(),
            kind,
            version,
            hash,
        })
    }
}

impl From<Error> for PersistenceError {
    fn from(e: Error) -> Self {
        PersistenceError::Failure(e.to_string())
    }
}

impl From<serde_json::Error> for PersistenceError {
    fn from(e: serde_json::Error) -> Self {
        PersistenceError::Failure(e.to_string())
    }
}
