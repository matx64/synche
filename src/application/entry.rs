use crate::{
    application::{IgnoreHandler, persistence::interface::PersistenceInterface},
    domain::{Directory, EntryInfo, EntryKind, Peer, entry::VersionCmp},
    proto::transport::PeerHandshakeData,
};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    fs::{self},
    sync::RwLock,
};
use tracing::{error, info, warn};
use uuid::Uuid;

pub struct EntryManager<D: PersistenceInterface> {
    db: D,
    local_id: Uuid,
    directories: RwLock<HashMap<String, Directory>>,
    ignore_handler: RwLock<IgnoreHandler>,
    base_dir: PathBuf,
}

impl<D: PersistenceInterface> EntryManager<D> {
    pub fn new(
        db: D,
        local_id: Uuid,
        directories: HashMap<String, Directory>,
        ignore_handler: IgnoreHandler,
        filesystem_entries: HashMap<String, EntryInfo>,
        base_dir: PathBuf,
    ) -> Self {
        Self::build_db(&db, local_id, filesystem_entries);
        Self {
            db,
            local_id,
            directories: RwLock::new(directories),
            ignore_handler: RwLock::new(ignore_handler),
            base_dir,
        }
    }

    fn build_db(db: &D, local_id: Uuid, filesystem_entries: HashMap<String, EntryInfo>) {
        let mut entries: HashMap<String, EntryInfo> = db
            .list_all_entries()
            .unwrap()
            .into_iter()
            .map(|f| (f.name.clone(), f))
            .collect();

        for (name, entry) in &mut entries {
            match filesystem_entries.get(name) {
                Some(fs_entry) if fs_entry.hash != entry.hash => {
                    *entry.version.entry(local_id).or_insert(0) += 1;

                    db.insert_or_replace_entry(&EntryInfo {
                        name: entry.name.clone(),
                        version: entry.version.clone(),
                        kind: fs_entry.kind.clone(),
                        hash: fs_entry.hash.clone(),
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

    pub async fn list_dirs(&self) -> HashMap<String, Directory> {
        self.directories.read().await.clone()
    }

    pub async fn is_ignored<P: AsRef<Path>>(&self, path: P, relative: &str) -> bool {
        self.ignore_handler.read().await.is_ignored(path, relative)
    }

    pub fn insert_entry(&self, mut entry: EntryInfo) -> EntryInfo {
        entry.version.entry(self.local_id).or_insert(0);
        self.db.insert_or_replace_entry(&entry).unwrap();
        entry
    }

    pub fn entry_created(&self, name: &str, kind: EntryKind, hash: Option<String>) -> EntryInfo {
        self.insert_entry(EntryInfo {
            name: name.to_owned(),
            kind,
            hash,
            version: HashMap::from([(self.local_id, 0)]),
        })
    }

    pub fn entry_modified(&self, mut entry: EntryInfo, hash: Option<String>) -> EntryInfo {
        entry.hash = hash;
        *entry.version.entry(self.local_id).or_insert(0) += 1;

        self.db.insert_or_replace_entry(&entry).unwrap();
        entry
    }

    pub fn get_entry(&self, name: &str) -> Option<EntryInfo> {
        self.db.get_entry(name).unwrap()
    }

    pub async fn get_entries_to_request(
        &self,
        peer: &Peer,
        peer_entries: HashMap<String, EntryInfo>,
    ) -> io::Result<Vec<EntryInfo>> {
        let mut to_request = Vec::new();

        let dirs = { self.directories.read().await.clone() };

        for (name, peer_entry) in peer_entries {
            if dirs.contains_key(&peer_entry.get_root_parent()) {
                if let Some(mut local_entry) = self.get_entry(&name) {
                    let cmp = self
                        .compare_and_resolve_conflict(&mut local_entry, &peer_entry, peer.id)
                        .await?;

                    if matches!(cmp, VersionCmp::KeepOther) {
                        to_request.push(peer_entry);
                    }
                } else {
                    to_request.push(peer_entry);
                }
            }
        }

        Ok(to_request)
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

        match (local_entry.is_removed(), peer_entry.is_removed()) {
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

        if !path.exists() || path.is_dir() {
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
        match self.get_entry(&peer_entry.name) {
            Some(mut local_entry) => {
                self.compare_and_resolve_conflict(&mut local_entry, peer_entry, peer_id)
                    .await
            }
            None => Ok(VersionCmp::KeepOther),
        }
    }

    pub fn merge_versions_and_insert(
        &self,
        local_entry: &mut EntryInfo,
        peer_entry: &EntryInfo,
        peer_id: Uuid,
    ) {
        local_entry.version.entry(peer_id).or_insert(0);
        for (pid, pv) in &peer_entry.version {
            let local_version = local_entry.version.entry(*pid).or_insert(0);
            *local_version = (*local_version).max(*pv);
        }
        self.db.insert_or_replace_entry(local_entry).unwrap();
    }

    pub fn remove_entry(&self, name: &str) -> Option<EntryInfo> {
        self.get_entry(name)
            .map(|mut entry| {
                entry = self.delete_and_update_entry(entry);
                Some(entry)
            })
            .unwrap_or_default()
    }

    pub fn remove_dir(&self, removed: &str) -> Vec<EntryInfo> {
        let mut removed_entries = Vec::new();

        let entries = self.db.list_all_entries().unwrap();
        for mut entry in entries {
            if entry.name.starts_with(&format!("{}/", removed)) {
                entry = self.delete_and_update_entry(entry);
                removed_entries.push(entry);
            }
        }

        removed_entries
    }

    pub fn delete_and_update_entry(&self, mut entry: EntryInfo) -> EntryInfo {
        self.db.delete_entry(&entry.name).unwrap();

        *entry.version.entry(self.local_id).or_insert(0) += 1;
        entry.set_removed_hash();

        entry
    }

    pub async fn get_handshake_data(&self) -> PeerHandshakeData {
        let directories = self
            .directories
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();

        let entries = self
            .db
            .list_all_entries()
            .unwrap()
            .into_iter()
            .map(|f| (f.name.clone(), f))
            .collect::<HashMap<String, EntryInfo>>();

        PeerHandshakeData {
            directories,
            entries,
        }
    }

    pub async fn insert_gitignore<P: AsRef<Path>>(&self, gitignore_path: P) {
        match self
            .ignore_handler
            .write()
            .await
            .insert_gitignore(&gitignore_path)
        {
            Ok(_) => {
                info!(
                    "⭕  Inserted or Updated .gitignore: {}",
                    gitignore_path.as_ref().to_string_lossy()
                );
            }
            Err(err) => {
                error!("Error inserting gitignore: {err}");
            }
        }
    }

    pub async fn remove_gitignore(&self, relative: &str) {
        self.ignore_handler.write().await.remove_gitignore(relative);
    }
}
