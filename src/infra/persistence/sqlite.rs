use crate::{
    application::persistence::interface::{
        PersistenceError, PersistenceInterface, PersistenceResult,
    },
    domain::{EntryInfo, entry::VersionVector},
};
use rusqlite::{Connection, params};

pub struct SqliteDb {
    conn: Connection,
}

impl SqliteDb {
    pub fn new(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                name TEXT PRIMARY KEY,
                hash TEXT NOT NULL,
                vv TEXT NOT NULL
            )",
            [],
        )?;

        Ok(Self { conn })
    }
}

impl PersistenceInterface for SqliteDb {
    fn insert_or_replace_entry(&self, file: &EntryInfo) -> PersistenceResult<()> {
        let vv_json = serde_json::to_string(&file.vv)?;

        self.conn.execute(
            "INSERT OR REPLACE INTO files (name, hash, vv) VALUES (?1, ?2, ?3)",
            params![file.name, file.hash, vv_json],
        )?;
        Ok(())
    }

    fn get_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, hash, vv FROM files WHERE name = ?1")?;
        let mut rows = stmt.query(params![name])?;

        if let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            let hash: String = row.get(1)?;
            let vv_json: String = row.get(2)?;
            let vv: VersionVector = serde_json::from_str(&vv_json)?;

            Ok(Some(EntryInfo { name, hash, vv }))
        } else {
            Ok(None)
        }
    }

    fn list_all_entries(&self) -> PersistenceResult<Vec<EntryInfo>> {
        let mut stmt = self.conn.prepare("SELECT name, hash, vv FROM files")?;

        let file_iter = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let hash: String = row.get(1)?;
            let vv_json: String = row.get(2)?;
            let vv: VersionVector = serde_json::from_str(&vv_json).map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    vv_json.len(),
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })?;
            Ok(EntryInfo { name, hash, vv })
        })?;

        let mut files = Vec::new();
        for file in file_iter {
            files.push(file?);
        }

        Ok(files)
    }

    fn remove_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>> {
        let Some(file) = self.get_entry(name)? else {
            return Ok(None);
        };

        self.conn
            .execute("DELETE FROM files WHERE name = ?1", params![name])?;

        Ok(Some(file))
    }
}

impl From<rusqlite::Error> for PersistenceError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Failure(value.to_string())
    }
}

impl From<serde_json::Error> for PersistenceError {
    fn from(value: serde_json::Error) -> Self {
        Self::Failure(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn sample_file(name: &str) -> EntryInfo {
        EntryInfo {
            name: name.to_string(),
            hash: "abc123".to_string(),
            vv: HashMap::from([(Uuid::new_v4(), 0)]),
        }
    }

    fn init_db() -> SqliteDb {
        SqliteDb::new(":memory:").expect("Failed to create in-memory db")
    }

    #[test]
    fn test_insert_and_get_file() {
        let db = init_db();
        let file = sample_file("file1.txt");

        db.insert_or_replace_entry(&file).unwrap();
        let loaded = db.get_entry("file1.txt").unwrap().unwrap();

        assert_eq!(loaded.name, file.name);
        assert_eq!(loaded.hash, file.hash);
        assert_eq!(loaded.vv, file.vv);
    }

    #[test]
    fn test_remove_file() {
        let db = init_db();
        let file = sample_file("file2.txt");

        db.insert_or_replace_entry(&file).unwrap();
        let removed = db.remove_entry("file2.txt").unwrap();

        assert_eq!(removed.unwrap().name, file.name);
        assert!(db.get_entry("file2.txt").unwrap().is_none());
    }

    #[test]
    fn test_list_all_files() {
        let db = init_db();
        let file1 = sample_file("fileA.txt");
        let file2 = sample_file("fileB.txt");

        db.insert_or_replace_entry(&file1).unwrap();
        db.insert_or_replace_entry(&file2).unwrap();

        let mut files = db.list_all_entries().unwrap();
        files.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name, "fileA.txt");
        assert_eq!(files[1].name, "fileB.txt");
    }
}
