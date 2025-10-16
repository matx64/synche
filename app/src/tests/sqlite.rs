use crate::{
    application::persistence::interface::PersistenceInterface,
    domain::{EntryInfo, EntryKind, VersionVector},
    infra::persistence::sqlite::SqliteDb,
};
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_sqlite() {
    let dbfile = NamedTempFile::new().unwrap();
    let db = SqliteDb::new(dbfile).await.unwrap();

    let entry = EntryInfo {
        name: "file.txt".to_string().into(),
        kind: EntryKind::File,
        hash: Some("abc123".into()),
        version: VersionVector::new(),
    };

    db.insert_or_replace_entry(&entry).await.unwrap();

    let fetched = db.get_entry("file.txt").await.unwrap().unwrap();
    assert_eq!(fetched.name, entry.name);
    assert_eq!(fetched.kind, entry.kind);
    assert_eq!(fetched.hash, entry.hash);

    let all = db.list_all_entries().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, entry.name);

    db.delete_entry("file.txt").await.unwrap();
    let after_delete = db.get_entry("file.txt").await.unwrap();
    assert!(after_delete.is_none());
}
