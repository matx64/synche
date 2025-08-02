use crate::{
    application::persistence::interface::{
        PersistenceError, PersistenceInterface, PersistenceResult,
    },
    domain::{EntryInfo, EntryKind, entry::VersionVector},
};
use rusqlite::{Connection, ToSql, params, types::FromSql};

pub struct SqliteDb {
    conn: Connection,
}

impl SqliteDb {
    pub fn new(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS entries (
                name TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                hash TEXT,
                is_deleted INTEGER NOT NULL DEFAULT 0,
                vv TEXT NOT NULL
            )",
            [],
        )?;

        Ok(Self { conn })
    }
}

impl PersistenceInterface for SqliteDb {
    fn insert_or_replace_entry(&self, entry: &EntryInfo) -> PersistenceResult<()> {
        let vv_json = serde_json::to_string(&entry.vv)?;
        let deleted_flag = if entry.is_deleted { 1 } else { 0 };

        self.conn.execute(
            "INSERT OR REPLACE INTO entries (name, kind, hash, is_deleted, vv)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![entry.name, entry.kind, entry.hash, deleted_flag, vv_json],
        )?;
        Ok(())
    }

    fn get_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, kind, hash, is_deleted, vv
             FROM entries
             WHERE name = ?1
             AND is_deleted = 0",
        )?;
        let mut rows = stmt.query(params![name])?;

        if let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            let kind: EntryKind = row.get(1)?;
            let hash: Option<String> = row.get(2)?;
            let is_deleted: i32 = row.get(3)?;
            let vv_json: String = row.get(4)?;
            let vv: VersionVector = serde_json::from_str(&vv_json)?;

            Ok(Some(EntryInfo {
                name,
                kind,
                hash,
                is_deleted: is_deleted != 0,
                vv,
            }))
        } else {
            Ok(None)
        }
    }

    fn list_all_entries(&self, include_deleted: bool) -> PersistenceResult<Vec<EntryInfo>> {
        let stmt = if include_deleted {
            "SELECT name, kind, hash, is_deleted, vv FROM entries"
        } else {
            "SELECT name, kind, hash, is_deleted, vv FROM entries WHERE is_deleted = 0"
        };

        let mut stmt = self.conn.prepare(stmt)?;

        let iter = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let kind: EntryKind = row.get(1)?;
            let hash: Option<String> = row.get(2)?;
            let is_deleted: i32 = row.get(3)?;
            let vv_json: String = row.get(4)?;
            let vv: VersionVector = serde_json::from_str(&vv_json).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    vv_json.len(),
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;

            Ok(EntryInfo {
                name,
                kind,
                hash,
                is_deleted: is_deleted != 0,
                vv,
            })
        })?;

        iter.collect::<Result<_, _>>().map_err(Into::into)
    }

    fn remove_entry_soft(&self, name: &str) -> PersistenceResult<Option<EntryInfo>> {
        if let Some(mut entry) = self.get_entry(name)? {
            self.conn.execute(
                "UPDATE entries SET is_deleted = 1 WHERE name = ?1",
                params![name],
            )?;
            entry.is_deleted = true;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    fn remove_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>> {
        if let Some(entry) = self.get_entry(name)? {
            self.conn
                .execute("DELETE FROM entries WHERE name = ?1", params![name])?;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }
}

impl ToSql for EntryKind {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        match self {
            EntryKind::File => Ok("File".into()),
            EntryKind::Directory => Ok("Dir".into()),
        }
    }
}

impl FromSql for EntryKind {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value {
            rusqlite::types::ValueRef::Text(text) => match text {
                b"File" => Ok(EntryKind::File),
                b"Dir" => Ok(EntryKind::Directory),
                _ => Err(rusqlite::types::FromSqlError::InvalidType),
            },
            _ => Err(rusqlite::types::FromSqlError::InvalidType),
        }
    }
}

impl From<rusqlite::Error> for PersistenceError {
    fn from(e: rusqlite::Error) -> Self {
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
    use crate::application::persistence::interface::PersistenceInterface;

    fn make_db() -> SqliteDb {
        SqliteDb::new(":memory:").expect("Failed to open in-memory database")
    }

    fn sample_entry(name: &str) -> EntryInfo {
        EntryInfo {
            name: name.to_string(),
            kind: EntryKind::File,
            hash: Some("abc123".to_string()),
            is_deleted: false,
            vv: VersionVector::default(),
        }
    }

    #[test]
    fn test_insert_and_get_entry() {
        let db = make_db();
        let entry = sample_entry("file1.txt");

        db.insert_or_replace_entry(&entry).unwrap();
        let fetched = db
            .get_entry(&entry.name)
            .unwrap()
            .expect("Entry should exist");
        assert_eq!(fetched.name, entry.name);
        assert!(matches!(fetched.kind, EntryKind::File));
        assert_eq!(fetched.hash, entry.hash);
        assert_eq!(fetched.is_deleted, entry.is_deleted);
        assert_eq!(fetched.vv, entry.vv);
    }

    #[test]
    fn test_get_nonexistent_entry() {
        let db = make_db();
        let result = db.get_entry("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_all_entries() {
        let db = make_db();
        let e1 = sample_entry("a.txt");
        let mut e2 = sample_entry("b.txt");
        e2.kind = EntryKind::Directory;
        e2.is_deleted = true;

        db.insert_or_replace_entry(&e1).unwrap();
        db.insert_or_replace_entry(&e2).unwrap();

        let mut all = db.list_all_entries(true).unwrap();
        all.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "a.txt");
        assert!(matches!(all[0].kind, EntryKind::File));
        assert_eq!(all[1].name, "b.txt");
        assert!(matches!(all[1].kind, EntryKind::Directory));
        assert!(all[1].is_deleted);
    }

    #[test]
    fn test_overwrite_entry() {
        let db = make_db();
        let mut entry = sample_entry("dup.txt");
        entry.hash = Some("first".to_string());
        db.insert_or_replace_entry(&entry).unwrap();

        entry.hash = Some("second".to_string());
        entry.is_deleted = true;
        db.insert_or_replace_entry(&entry).unwrap();

        let fetched = db.get_entry(&entry.name).unwrap();
        assert!(fetched.is_none());
    }

    #[test]
    fn test_remove_entry() {
        let db = make_db();
        let entry = sample_entry("toremove.txt");
        db.insert_or_replace_entry(&entry).unwrap();

        let removed = db
            .remove_entry(&entry.name)
            .unwrap()
            .expect("Entry should be removed");
        assert_eq!(removed.name, entry.name);

        let after = db.get_entry(&entry.name).unwrap();
        assert!(after.is_none());
    }
}
