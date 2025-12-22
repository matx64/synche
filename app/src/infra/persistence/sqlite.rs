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
        let path_str = path.as_ref().to_string_lossy();

        let pool = if path_str == ":memory:" || path_str == "sqlite::memory:" {
            SqlitePool::connect("sqlite::memory:").await?
        } else {
            SqlitePool::connect_with(
                SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true),
            )
            .await?
        };

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{EntryKind, VersionVector};
    use std::collections::HashMap;
    use tempfile::tempdir;
    use uuid::Uuid;

    async fn create_test_db() -> SqliteDb {
        SqliteDb::new(":memory:").await.unwrap()
    }

    fn create_test_entry(name: &str, kind: EntryKind, hash: Option<String>) -> EntryInfo {
        let mut version = VersionVector::new();
        let device_id = Uuid::new_v4();
        version.insert(device_id, 1);

        EntryInfo {
            name: name.into(),
            kind,
            hash,
            version,
        }
    }

    #[tokio::test]
    async fn test_database_creation() {
        let _dir = tempdir().unwrap();
        let db_path = _dir.path().join("test.db");

        let db = SqliteDb::new(&db_path).await.unwrap();

        assert!(db_path.exists());

        let entries = db.list_all_entries().await.unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[tokio::test]
    async fn test_get_entry_not_found() {
        let db = create_test_db().await;

        let result = db.get_entry("nonexistent.txt").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_insert_entry() {
        let db = create_test_db().await;
        let entry = create_test_entry(
            "test/file.txt",
            EntryKind::File,
            Some("hash123".to_string()),
        );

        db.insert_or_replace_entry(&entry).await.unwrap();

        let retrieved = db.get_entry("test/file.txt").await.unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(&*retrieved.name, "test/file.txt");
        assert_eq!(retrieved.kind, EntryKind::File);
        assert_eq!(retrieved.hash, Some("hash123".to_string()));
        assert_eq!(retrieved.version, entry.version);
    }

    #[tokio::test]
    async fn test_replace_entry() {
        let db = create_test_db().await;

        let entry1 = create_test_entry("test/file.txt", EntryKind::File, Some("hash1".to_string()));
        db.insert_or_replace_entry(&entry1).await.unwrap();

        let mut entry2 =
            create_test_entry("test/file.txt", EntryKind::File, Some("hash2".to_string()));
        let device_id = Uuid::new_v4();
        entry2.version.insert(device_id, 5);
        db.insert_or_replace_entry(&entry2).await.unwrap();

        let retrieved = db.get_entry("test/file.txt").await.unwrap().unwrap();
        assert_eq!(retrieved.hash, Some("hash2".to_string()));
        assert_eq!(retrieved.version.get(&device_id), Some(&5));
    }

    #[tokio::test]
    async fn test_list_all_entries_empty() {
        let db = create_test_db().await;

        let entries = db.list_all_entries().await.unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[tokio::test]
    async fn test_list_all_entries_multiple() {
        let db = create_test_db().await;

        let entry1 =
            create_test_entry("dir1/file1.txt", EntryKind::File, Some("hash1".to_string()));
        let entry2 =
            create_test_entry("dir1/file2.txt", EntryKind::File, Some("hash2".to_string()));
        let entry3 = create_test_entry("dir2", EntryKind::Directory, None);

        db.insert_or_replace_entry(&entry1).await.unwrap();
        db.insert_or_replace_entry(&entry2).await.unwrap();
        db.insert_or_replace_entry(&entry3).await.unwrap();

        let entries = db.list_all_entries().await.unwrap();
        assert_eq!(entries.len(), 3);

        let names: Vec<&str> = entries.iter().map(|e| &*e.name).collect();
        assert!(names.contains(&"dir1/file1.txt"));
        assert!(names.contains(&"dir1/file2.txt"));
        assert!(names.contains(&"dir2"));
    }

    #[tokio::test]
    async fn test_delete_entry() {
        let db = create_test_db().await;

        let entry = create_test_entry(
            "test/file.txt",
            EntryKind::File,
            Some("hash123".to_string()),
        );

        db.insert_or_replace_entry(&entry).await.unwrap();
        assert!(db.get_entry("test/file.txt").await.unwrap().is_some());

        db.delete_entry("test/file.txt").await.unwrap();
        assert!(db.get_entry("test/file.txt").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_entry() {
        let db = create_test_db().await;

        // Deleting a nonexistent entry should not error
        db.delete_entry("nonexistent.txt").await.unwrap();
    }

    #[tokio::test]
    async fn test_entry_kind_file() {
        let db = create_test_db().await;
        let entry = create_test_entry("file.txt", EntryKind::File, Some("hash".to_string()));

        db.insert_or_replace_entry(&entry).await.unwrap();

        let retrieved = db.get_entry("file.txt").await.unwrap().unwrap();
        assert_eq!(retrieved.kind, EntryKind::File);
    }

    #[tokio::test]
    async fn test_entry_kind_directory() {
        let db = create_test_db().await;
        let entry = create_test_entry("mydir", EntryKind::Directory, None);

        db.insert_or_replace_entry(&entry).await.unwrap();

        let retrieved = db.get_entry("mydir").await.unwrap().unwrap();
        assert_eq!(retrieved.kind, EntryKind::Directory);
    }

    #[tokio::test]
    async fn test_version_vector_serialization() {
        let db = create_test_db().await;

        let mut version = VersionVector::new();
        let device1 = Uuid::new_v4();
        let device2 = Uuid::new_v4();
        let device3 = Uuid::new_v4();
        version.insert(device1, 10);
        version.insert(device2, 25);
        version.insert(device3, 3);

        let entry = EntryInfo {
            name: "test.txt".into(),
            kind: EntryKind::File,
            hash: Some("hash".to_string()),
            version: version.clone(),
        };

        db.insert_or_replace_entry(&entry).await.unwrap();

        let retrieved = db.get_entry("test.txt").await.unwrap().unwrap();
        assert_eq!(retrieved.version.len(), 3);
        assert_eq!(retrieved.version.get(&device1), Some(&10));
        assert_eq!(retrieved.version.get(&device2), Some(&25));
        assert_eq!(retrieved.version.get(&device3), Some(&3));
    }

    #[tokio::test]
    async fn test_hash_none() {
        let db = create_test_db().await;
        let entry = create_test_entry("dir", EntryKind::Directory, None);

        db.insert_or_replace_entry(&entry).await.unwrap();

        let retrieved = db.get_entry("dir").await.unwrap().unwrap();
        assert_eq!(retrieved.hash, None);
    }

    #[tokio::test]
    async fn test_hash_some() {
        let db = create_test_db().await;
        let entry = create_test_entry(
            "file.txt",
            EntryKind::File,
            Some("abc123def456".to_string()),
        );

        db.insert_or_replace_entry(&entry).await.unwrap();

        let retrieved = db.get_entry("file.txt").await.unwrap().unwrap();
        assert_eq!(retrieved.hash, Some("abc123def456".to_string()));
    }

    #[tokio::test]
    async fn test_entry_kind_display() {
        assert_eq!(EntryKind::File.to_string(), "F");
        assert_eq!(EntryKind::Directory.to_string(), "D");
    }

    #[tokio::test]
    async fn test_long_hash() {
        let db = create_test_db().await;

        // SHA256 hash (64 characters)
        let long_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let entry = create_test_entry("file.txt", EntryKind::File, Some(long_hash.to_string()));

        db.insert_or_replace_entry(&entry).await.unwrap();

        let retrieved = db.get_entry("file.txt").await.unwrap().unwrap();
        assert_eq!(retrieved.hash, Some(long_hash.to_string()));
    }

    #[tokio::test]
    async fn test_empty_version_vector() {
        let db = create_test_db().await;

        let entry = EntryInfo {
            name: "test.txt".into(),
            kind: EntryKind::File,
            hash: Some("hash".to_string()),
            version: HashMap::new(),
        };

        db.insert_or_replace_entry(&entry).await.unwrap();

        let retrieved = db.get_entry("test.txt").await.unwrap().unwrap();
        assert_eq!(retrieved.version.len(), 0);
    }
}
