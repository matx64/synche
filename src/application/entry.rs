use crate::{
    application::persistence::interface::PersistenceInterface,
    domain::{Directory, EntryInfo, EntryKind, Peer, entry::VersionCmp},
    proto::transport::PeerHandshakeData,
};
use std::{
    collections::HashMap,
    io,
    path::PathBuf,
    sync::RwLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    fs::{self},
    time::interval,
};
use tracing::{error, info, warn};
use uuid::Uuid;

pub struct EntryManager<D: PersistenceInterface> {
    db: D,
    local_id: Uuid,
    directories: RwLock<HashMap<String, Directory>>,
    base_dir: PathBuf,
}

impl<D: PersistenceInterface> EntryManager<D> {
    pub fn new(
        db: D,
        local_id: Uuid,
        directories: HashMap<String, Directory>,
        filesystem_entries: HashMap<String, EntryInfo>,
        base_dir: PathBuf,
    ) -> Self {
        Self::build_db(&db, local_id, filesystem_entries);
        Self {
            db,
            local_id,
            directories: RwLock::new(directories),
            base_dir,
        }
    }

    fn build_db(db: &D, local_id: Uuid, filesystem_entries: HashMap<String, EntryInfo>) {
        let mut entries: HashMap<String, EntryInfo> = db
            .list_all_entries(true)
            .unwrap()
            .into_iter()
            .map(|f| (f.name.clone(), f))
            .collect();

        for (name, entry) in &mut entries {
            match filesystem_entries.get(name) {
                Some(fs_entry) if fs_entry.hash != entry.hash => {
                    *entry.vv.entry(local_id).or_insert(0) += 1;

                    db.insert_or_replace_entry(&EntryInfo {
                        name: entry.name.clone(),
                        vv: entry.vv.clone(),
                        kind: fs_entry.kind.clone(),
                        hash: fs_entry.hash.clone(),
                        is_removed: fs_entry.is_removed,
                    })
                    .unwrap();
                }

                None => {
                    db.delete_entry(&entry.name).unwrap();
                }

                _ => {}
            }
        }

        for (name, fs_entry) in filesystem_entries {
            if !entries.contains_key(&name) {
                db.insert_or_replace_entry(&fs_entry).unwrap();
            }
        }
    }

    pub fn list_dirs(&self) -> HashMap<String, Directory> {
        self.directories
            .read()
            .map(|dirs| dirs.clone())
            .unwrap_or_default()
    }

    pub fn insert_entry(&self, mut entry: EntryInfo) -> EntryInfo {
        entry.vv.entry(self.local_id).or_insert(0);
        self.db.insert_or_replace_entry(&entry).unwrap();
        entry
    }

    pub fn entry_created(&self, name: &str, kind: EntryKind, hash: Option<String>) -> EntryInfo {
        self.insert_entry(EntryInfo {
            name: name.to_owned(),
            kind,
            hash,
            is_removed: false,
            vv: HashMap::from([(self.local_id, 0)]),
        })
    }

    pub fn entry_modified(&self, mut entry: EntryInfo, hash: Option<String>) -> EntryInfo {
        entry.hash = hash;
        *entry.vv.entry(self.local_id).or_insert(0) += 1;

        self.db.insert_or_replace_entry(&entry).unwrap();
        entry
    }

    pub fn get_entry(&self, name: &str) -> Option<EntryInfo> {
        self.db.get_entry(name).unwrap()
    }

    pub fn entry_exists(&self, name: &str) -> bool {
        self.db
            .get_entry(name)
            .unwrap()
            .map(|e| !e.is_removed)
            .unwrap_or(false)
    }

    pub async fn get_entries_to_request(
        &self,
        peer: &Peer,
        peer_entries: HashMap<String, EntryInfo>,
    ) -> Vec<EntryInfo> {
        let mut to_request = Vec::new();

        let dirs = {
            let dirs = self.directories.read().unwrap();
            dirs.clone()
        };

        for (name, peer_entry) in peer_entries {
            if dirs.contains_key(&peer_entry.get_root_parent()) {
                if let Some(mut local_entry) = self.get_entry(&name) {
                    let cmp = self
                        .compare_and_resolve_conflict(&mut local_entry, &peer_entry, peer.id)
                        .await
                        .unwrap();

                    if matches!(cmp, VersionCmp::KeepOther) {
                        to_request.push(peer_entry);
                    }
                } else {
                    to_request.push(peer_entry);
                }
            }
        }

        to_request
    }

    pub async fn compare_and_resolve_conflict(
        &self,
        local_entry: &mut EntryInfo,
        peer_entry: &EntryInfo,
        peer_id: Uuid,
    ) -> io::Result<VersionCmp> {
        let cmp = match local_entry.compare(peer_entry) {
            VersionCmp::Conflict => {
                self.handle_conflict(local_entry, peer_entry, peer_id)
                    .await?
            }
            other => other,
        };

        if matches!(cmp, VersionCmp::Equal | VersionCmp::KeepSelf) {
            self.merge_versions_and_insert(local_entry, peer_entry, peer_id);
        }

        Ok(cmp)
    }

    pub async fn handle_conflict(
        &self,
        local_entry: &mut EntryInfo,
        peer_entry: &EntryInfo,
        peer_id: Uuid,
    ) -> io::Result<VersionCmp> {
        warn!(entry = local_entry.name, peer = ?peer_id, "[⚠️  CONFLICT]");

        match (local_entry.is_removed, peer_entry.is_removed) {
            (true, false) => {
                return Ok(VersionCmp::KeepOther);
            }

            (false, true) => {
                return Ok(VersionCmp::KeepSelf);
            }

            (true, true) => {
                return Ok(VersionCmp::Equal);
            }

            (false, false) => {}
        }

        if self.local_id < peer_id {
            return Ok(VersionCmp::KeepSelf);
        }

        let path = PathBuf::from(&self.base_dir).join(&local_entry.name);

        if !path.exists() {
            return Ok(VersionCmp::KeepOther);
        }

        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let ext = path.extension().unwrap_or_default().to_string_lossy();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let new_path = path.with_file_name(format!(
            "{}_CONFLICT_{}_{}.{}",
            stem, now, self.local_id, ext
        ));

        fs::copy(path, new_path).await?;

        Ok(VersionCmp::KeepOther)
    }

    pub async fn handle_metadata(
        &self,
        peer_id: Uuid,
        peer_entry: &EntryInfo,
    ) -> io::Result<VersionCmp> {
        let mut local_entry = match self.get_entry(&peer_entry.name) {
            Some(entry) if !entry.is_removed => entry,
            _ => return Ok(VersionCmp::KeepOther),
        };

        self.compare_and_resolve_conflict(&mut local_entry, &peer_entry, peer_id)
            .await
    }

    pub fn merge_versions_and_insert(
        &self,
        local_entry: &mut EntryInfo,
        peer_entry: &EntryInfo,
        peer_id: Uuid,
    ) {
        local_entry.vv.entry(peer_id).or_insert(0);
        for (pid, pv) in &peer_entry.vv {
            let local_version = local_entry.vv.entry(*pid).or_insert(0);
            *local_version = (*local_version).max(*pv);
        }
        self.db.insert_or_replace_entry(local_entry).unwrap();
    }

    pub fn remove_entry(&self, name: &str) -> Option<EntryInfo> {
        self.get_entry(name)
            .map(|mut entry| {
                *entry.vv.entry(self.local_id).or_insert(0) += 1;
                entry.hash = None;
                entry.is_removed = true;

                self.db.insert_or_replace_entry(&entry).unwrap();
                Some(entry)
            })
            .unwrap_or_default()
    }

    pub fn remove_dir(&self, deleted: &str) -> Vec<EntryInfo> {
        let mut removed_entries = Vec::new();

        let entries = self.db.list_all_entries(false).unwrap();
        for mut entry in entries {
            if entry.name.starts_with(&format!("{}/", deleted)) {
                *entry.vv.entry(self.local_id).or_insert(0) += 1;
                entry.hash = None;
                entry.is_removed = true;

                self.db.insert_or_replace_entry(&entry).unwrap();
                removed_entries.push(entry);
            }
        }

        removed_entries
    }

    pub fn get_handshake_data(&self) -> PeerHandshakeData {
        let directories = self
            .directories
            .read()
            .map(|dirs| dirs.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        let entries = self
            .db
            .list_all_entries(false)
            .unwrap()
            .into_iter()
            .map(|f| (f.name.clone(), f))
            .collect::<HashMap<String, EntryInfo>>();

        PeerHandshakeData {
            directories,
            entries,
        }
    }

    pub fn _update_dirs(&self, updated: HashMap<String, Directory>) {
        if let Ok(mut dirs) = self.directories.write() {
            dirs.clear();
            *dirs = updated;
        }
    }

    pub async fn clean_removed_entries(&self) -> io::Result<()> {
        let mut retries: u8 = 0;
        let mut interval = interval(Duration::from_secs(120));

        loop {
            interval.tick().await;

            info!("🧹 Cleaning removed entries...");

            if let Err(err) = self.db.clean_removed_entries() {
                error!("Failed to clean removed entries: {err}");
                retries += 1;

                if retries >= 3 {
                    return Err(io::Error::other(
                        "Failed to clean removed entries 3 times in a row.",
                    ));
                }
            } else {
                retries = 0;
            }
        }
    }
}
