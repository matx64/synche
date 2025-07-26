use crate::{
    application::persistence::interface::{
        PersistenceError, PersistenceInterface, PersistenceResult,
    },
    domain::{FileInfo, entry::VersionVector},
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
    fn insert_or_replace_file(&self, file: &FileInfo) -> PersistenceResult<()> {
        let vv_json = serde_json::to_string(&file.vv)?;

        self.conn.execute(
            "INSERT OR REPLACE INTO files (name, hash, vv) VALUES (?1, ?2, ?3)",
            params![file.name, file.hash, vv_json],
        )?;
        Ok(())
    }

    fn get_file(&self, name: &str) -> PersistenceResult<Option<FileInfo>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, hash, vv FROM files WHERE name = ?1")?;
        let mut rows = stmt.query(params![name])?;

        if let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            let hash: String = row.get(1)?;
            let vv_json: String = row.get(2)?;
            let vv: VersionVector = serde_json::from_str(&vv_json)?;

            Ok(Some(FileInfo { name, hash, vv }))
        } else {
            Ok(None)
        }
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
