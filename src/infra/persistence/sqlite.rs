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
                version TEXT NOT NULL
            )",
            [],
        )?;

        Ok(Self { conn })
    }
}

impl PersistenceInterface for SqliteDb {
    fn insert_or_replace_entry(&self, entry: &EntryInfo) -> PersistenceResult<()> {
        let version_json = serde_json::to_string(&entry.version)?;

        self.conn.execute(
            "INSERT OR REPLACE INTO entries (name, kind, hash, version)
             VALUES (?1, ?2, ?3, ?4)",
            params![entry.name, entry.kind, entry.hash, version_json],
        )?;
        Ok(())
    }

    fn get_entry(&self, name: &str) -> PersistenceResult<Option<EntryInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, kind, hash, version
             FROM entries
             WHERE name = ?1",
        )?;
        let mut rows = stmt.query(params![name])?;

        if let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            let kind: EntryKind = row.get(1)?;
            let hash: Option<String> = row.get(2)?;
            let version_json: String = row.get(3)?;
            let version: VersionVector = serde_json::from_str(&version_json)?;

            Ok(Some(EntryInfo {
                name,
                kind,
                hash,
                version,
            }))
        } else {
            Ok(None)
        }
    }

    fn list_all_entries(&self) -> PersistenceResult<Vec<EntryInfo>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, kind, hash, version FROM entries")?;

        let iter = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let kind: EntryKind = row.get(1)?;
            let hash: Option<String> = row.get(2)?;
            let version_json: String = row.get(3)?;
            let version: VersionVector = serde_json::from_str(&version_json).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    version_json.len(),
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;

            Ok(EntryInfo {
                name,
                kind,
                hash,
                version,
            })
        })?;

        iter.collect::<Result<_, _>>().map_err(Into::into)
    }

    fn delete_entry(&self, name: &str) -> PersistenceResult<()> {
        self.conn
            .execute("DELETE FROM entries WHERE name = ?1", params![name])?;
        Ok(())
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
