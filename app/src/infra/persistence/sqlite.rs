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

/// `sqlx`-backed SQLite adapter for `PersistenceInterface`.
///
/// Stores one row per `EntryInfo` and serializes the `VersionVector`
/// inline. Accepts `:memory:` as a path so tests can run against an
/// in-process database without touching disk.
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
                version TEXT NOT NULL,
                deleted INTEGER NOT NULL DEFAULT 0,
                tombstoned_at INTEGER DEFAULT NULL
            )",
        )
        .await?;

        Self::migrate_deleted_column(&pool).await?;
        Self::migrate_tombstoned_at_column(&pool).await?;

        Ok(Self { pool })
    }

    /// One-time migration off the legacy `REMOVED_HASH` tombstone sentinel
    /// (issue #42). Older databases encoded a tombstone by stamping the
    /// `hash` column with a 32-zero string and have no `deleted` column. If
    /// the column is missing, add it, then promote every legacy sentinel
    /// row to an explicit tombstone (`deleted = 1`) and clear the now-defunct
    /// sentinel from `hash`, so existing deletions survive the upgrade and
    /// keep propagating to peers. The promotion runs even when the column is
    /// already present, allowing startup to finish a migration that crashed
    /// after `ALTER TABLE`.
    async fn migrate_deleted_column(pool: &Pool<Sqlite>) -> Result<(), Error> {
        let columns = sqlx::query("PRAGMA table_info(entries)")
            .fetch_all(pool)
            .await?;
        let has_deleted = columns
            .iter()
            .filter_map(|row| row.try_get::<String, _>("name").ok())
            .any(|name| name == "deleted");

        if !has_deleted {
            pool.execute("ALTER TABLE entries ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0")
                .await?;
        }

        sqlx::query("UPDATE entries SET deleted = 1 WHERE hash = ?")
            .bind(LEGACY_REMOVED_HASH)
            .execute(pool)
            .await?;

        pool.execute("UPDATE entries SET hash = NULL WHERE deleted != 0")
            .await?;

        Ok(())
    }

    /// One-time migration adding the `tombstoned_at` column (issue #43).
    /// Tombstone rows carry the Unix-millis timestamp of when this device
    /// last persisted them as deleted; the periodic GC drops tombstones
    /// older than the retention window. If the column is missing, add it,
    /// then backfill every existing tombstone with the current time so
    /// pre-upgrade tombstones become GC-eligible starting at upgrade rather
    /// than being dropped immediately. The backfill runs even when the
    /// column already exists so startup can finish a migration that crashed
    /// after `ALTER TABLE`.
    async fn migrate_tombstoned_at_column(pool: &Pool<Sqlite>) -> Result<(), Error> {
        let columns = sqlx::query("PRAGMA table_info(entries)")
            .fetch_all(pool)
            .await?;
        let has_tombstoned_at = columns
            .iter()
            .filter_map(|row| row.try_get::<String, _>("name").ok())
            .any(|name| name == "tombstoned_at");

        if !has_tombstoned_at {
            pool.execute("ALTER TABLE entries ADD COLUMN tombstoned_at INTEGER DEFAULT NULL")
                .await?;
        }

        sqlx::query(
            "UPDATE entries SET tombstoned_at = ? WHERE deleted = 1 AND tombstoned_at IS NULL",
        )
        .bind(now_unix_millis())
        .execute(pool)
        .await?;

        Ok(())
    }
}

/// Current Unix time in milliseconds, clamped into `i64` for SQLite.
fn now_unix_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Legacy tombstone sentinel from before issue #42. Used **only** by the
/// one-time `deleted`-column migration to recognize pre-upgrade tombstones;
/// it must never re-enter the live hash path.
const LEGACY_REMOVED_HASH: &str = "00000000000000000000000000000000";

#[async_trait::async_trait]
impl PersistenceInterface for SqliteDb {
    async fn insert_or_replace_entry(&self, entry: &EntryInfo) -> PersistenceResult<()> {
        let version_json = serde_json::to_string(&entry.version)?;

        // UPSERT (not INSERT OR REPLACE) so a re-persist of an existing
        // tombstone preserves its original `tombstoned_at` via COALESCE.
        // INSERT OR REPLACE deletes the prior row and would reset the
        // timestamp on every version bump / peer re-advertisement. A fresh
        // tombstone is stamped now; a row going live clears the timestamp.
        sqlx::query(
            "INSERT INTO entries (name, kind, hash, version, deleted, tombstoned_at)
                VALUES (?, ?, ?, ?, ?, CASE WHEN ? = 1 THEN ? ELSE NULL END)
                ON CONFLICT(name) DO UPDATE SET
                    kind = excluded.kind,
                    hash = excluded.hash,
                    version = excluded.version,
                    deleted = excluded.deleted,
                    tombstoned_at = CASE
                        WHEN excluded.deleted = 1
                            THEN COALESCE(entries.tombstoned_at, excluded.tombstoned_at)
                        ELSE NULL
                    END",
        )
        .bind(&*entry.name)
        .bind(entry.kind.to_string())
        .bind(entry.hash.clone())
        .bind(version_json)
        .bind(entry.deleted as i64)
        .bind(entry.deleted as i64)
        .bind(now_unix_millis())
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

    async fn gc_tombstones(&self, cutoff_ms: i64) -> PersistenceResult<u64> {
        let result = sqlx::query(
            "DELETE FROM entries
                WHERE deleted = 1 AND tombstoned_at IS NOT NULL AND tombstoned_at < ?",
        )
        .bind(cutoff_ms)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
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
        let deleted: i64 = row.try_get("deleted")?;

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
            deleted: deleted != 0,
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
            deleted: false,
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
            deleted: false,
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
            deleted: false,
        };

        db.insert_or_replace_entry(&entry).await.unwrap();

        let retrieved = db.get_entry("test.txt").await.unwrap().unwrap();
        assert_eq!(retrieved.version.len(), 0);
    }

    #[tokio::test]
    async fn test_tombstone_round_trips() {
        let db = create_test_db().await;

        let mut entry = create_test_entry("dir/gone.txt", EntryKind::File, Some("abc".to_string()));
        entry.mark_removed();
        db.insert_or_replace_entry(&entry).await.unwrap();

        let retrieved = db.get_entry("dir/gone.txt").await.unwrap().unwrap();
        assert!(retrieved.is_removed());
        assert!(retrieved.deleted);
        assert_eq!(retrieved.hash, None);
    }

    #[tokio::test]
    async fn test_migrates_legacy_removed_hash_to_deleted_flag() {
        // A database written before issue #42 has no `deleted` column and
        // encodes tombstones with the all-zeros sentinel in `hash`. Opening
        // it through `SqliteDb::new` must add the column, promote the
        // sentinel row to an explicit tombstone, and clear the hash.
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("legacy.db");

        let legacy_pool = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true),
        )
        .await
        .unwrap();

        legacy_pool
            .execute(
                "CREATE TABLE entries (
                    name TEXT PRIMARY KEY,
                    kind TEXT NOT NULL,
                    hash TEXT,
                    version TEXT NOT NULL
                )",
            )
            .await
            .unwrap();

        // One legacy tombstone (sentinel hash) and one live file.
        sqlx::query("INSERT INTO entries (name, kind, hash, version) VALUES (?, ?, ?, ?)")
            .bind("dir/gone.txt")
            .bind("F")
            .bind(LEGACY_REMOVED_HASH)
            .bind("{}")
            .execute(&legacy_pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO entries (name, kind, hash, version) VALUES (?, ?, ?, ?)")
            .bind("dir/live.txt")
            .bind("F")
            .bind("abc123")
            .bind("{}")
            .execute(&legacy_pool)
            .await
            .unwrap();
        legacy_pool.close().await;

        let db = SqliteDb::new(&db_path).await.unwrap();

        let tombstone = db.get_entry("dir/gone.txt").await.unwrap().unwrap();
        assert!(
            tombstone.is_removed(),
            "legacy sentinel must become a tombstone"
        );
        assert_eq!(tombstone.hash, None, "legacy sentinel hash must be cleared");

        let live = db.get_entry("dir/live.txt").await.unwrap().unwrap();
        assert!(!live.is_removed());
        assert_eq!(live.hash, Some("abc123".to_string()));
    }

    #[tokio::test]
    async fn test_migration_finishes_when_deleted_column_already_exists() {
        // If a previous startup crashed after adding the `deleted` column but
        // before promoting/clearing legacy sentinel rows, opening the DB again
        // must complete those data fixes instead of returning early.
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("partial_legacy.db");

        let legacy_pool = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true),
        )
        .await
        .unwrap();

        legacy_pool
            .execute(
                "CREATE TABLE entries (
                    name TEXT PRIMARY KEY,
                    kind TEXT NOT NULL,
                    hash TEXT,
                    version TEXT NOT NULL,
                    deleted INTEGER NOT NULL DEFAULT 0
                )",
            )
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO entries (name, kind, hash, version, deleted) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("dir/gone-after-alter.txt")
        .bind("F")
        .bind(LEGACY_REMOVED_HASH)
        .bind("{}")
        .bind(0_i64)
        .execute(&legacy_pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO entries (name, kind, hash, version, deleted) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("dir/dirty-deleted.txt")
        .bind("F")
        .bind("stale-hash")
        .bind("{}")
        .bind(1_i64)
        .execute(&legacy_pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO entries (name, kind, hash, version, deleted) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("dir/live.txt")
        .bind("F")
        .bind("abc123")
        .bind("{}")
        .bind(0_i64)
        .execute(&legacy_pool)
        .await
        .unwrap();
        legacy_pool.close().await;

        let db = SqliteDb::new(&db_path).await.unwrap();

        let promoted = db
            .get_entry("dir/gone-after-alter.txt")
            .await
            .unwrap()
            .unwrap();
        assert!(promoted.is_removed());
        assert_eq!(promoted.hash, None);

        let cleared = db
            .get_entry("dir/dirty-deleted.txt")
            .await
            .unwrap()
            .unwrap();
        assert!(cleared.is_removed());
        assert_eq!(cleared.hash, None);

        let live = db.get_entry("dir/live.txt").await.unwrap().unwrap();
        assert!(!live.is_removed());
        assert_eq!(live.hash, Some("abc123".to_string()));
    }

    /// Reads the raw `tombstoned_at` column for a row (tests live in-module
    /// so they can reach `db.pool` directly to inspect/age the timestamp).
    async fn tombstoned_at(db: &SqliteDb, name: &str) -> Option<i64> {
        let row = sqlx::query("SELECT tombstoned_at FROM entries WHERE name = ?")
            .bind(name)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        row.try_get::<Option<i64>, _>("tombstoned_at").unwrap()
    }

    #[tokio::test]
    async fn test_tombstoned_at_set_preserved_and_cleared() {
        let db = create_test_db().await;

        // Live entry: no timestamp.
        let mut entry = create_test_entry("dir/file.txt", EntryKind::File, Some("abc".into()));
        db.insert_or_replace_entry(&entry).await.unwrap();
        assert_eq!(tombstoned_at(&db, "dir/file.txt").await, None);

        // Becomes a tombstone: stamped now.
        entry.mark_removed();
        db.insert_or_replace_entry(&entry).await.unwrap();
        let first = tombstoned_at(&db, "dir/file.txt").await;
        assert!(first.is_some(), "tombstone must carry a timestamp");

        // Re-persisting the tombstone (e.g. a peer re-advertisement) must
        // preserve the original stamp, not reset it.
        db.insert_or_replace_entry(&entry).await.unwrap();
        assert_eq!(tombstoned_at(&db, "dir/file.txt").await, first);

        // Resurrecting the entry to live clears the timestamp.
        entry.deleted = false;
        entry.hash = Some("def".into());
        db.insert_or_replace_entry(&entry).await.unwrap();
        assert_eq!(tombstoned_at(&db, "dir/file.txt").await, None);
    }

    #[tokio::test]
    async fn test_gc_tombstones_removes_only_aged_tombstones() {
        let db = create_test_db().await;

        let mut old = create_test_entry("dir/old.txt", EntryKind::File, Some("a".into()));
        old.mark_removed();
        db.insert_or_replace_entry(&old).await.unwrap();

        let mut recent = create_test_entry("dir/recent.txt", EntryKind::File, Some("b".into()));
        recent.mark_removed();
        db.insert_or_replace_entry(&recent).await.unwrap();

        let live = create_test_entry("dir/live.txt", EntryKind::File, Some("c".into()));
        db.insert_or_replace_entry(&live).await.unwrap();

        // Age the old tombstone well into the past.
        sqlx::query("UPDATE entries SET tombstoned_at = 1000 WHERE name = ?")
            .bind("dir/old.txt")
            .execute(&db.pool)
            .await
            .unwrap();

        let removed = db.gc_tombstones(10_000).await.unwrap();
        assert_eq!(removed, 1);

        assert!(db.get_entry("dir/old.txt").await.unwrap().is_none());
        assert!(db.get_entry("dir/recent.txt").await.unwrap().is_some());
        assert!(db.get_entry("dir/live.txt").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_migration_backfills_tombstoned_at_for_existing_tombstones() {
        // A database written before issue #43 has the `deleted` column but
        // no `tombstoned_at`. Opening it must add the column and stamp every
        // existing tombstone so it becomes GC-eligible from upgrade time.
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("pre_43.db");

        let legacy_pool = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true),
        )
        .await
        .unwrap();

        legacy_pool
            .execute(
                "CREATE TABLE entries (
                    name TEXT PRIMARY KEY,
                    kind TEXT NOT NULL,
                    hash TEXT,
                    version TEXT NOT NULL,
                    deleted INTEGER NOT NULL DEFAULT 0
                )",
            )
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO entries (name, kind, hash, version, deleted) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("dir/gone.txt")
        .bind("F")
        .bind(Option::<String>::None)
        .bind("{}")
        .bind(1_i64)
        .execute(&legacy_pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO entries (name, kind, hash, version, deleted) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("dir/live.txt")
        .bind("F")
        .bind("abc123")
        .bind("{}")
        .bind(0_i64)
        .execute(&legacy_pool)
        .await
        .unwrap();
        legacy_pool.close().await;

        let db = SqliteDb::new(&db_path).await.unwrap();

        assert!(
            tombstoned_at(&db, "dir/gone.txt").await.is_some(),
            "existing tombstone must be backfilled with a timestamp"
        );
        assert_eq!(
            tombstoned_at(&db, "dir/live.txt").await,
            None,
            "live entries must not be stamped"
        );
    }
}
