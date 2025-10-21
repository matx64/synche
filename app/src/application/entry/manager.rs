use crate::{
    application::{entry::ignore::IgnoreHandler, persistence::interface::PersistenceInterface},
    domain::{
        CanonicalPath, EntryInfo, EntryKind, HandshakeData, Peer, RelativePath, SyncDirectory,
        VersionCmp,
    },
    utils::fs::{compute_hash, is_ds_store},
};
use std::{
    collections::{HashMap, VecDeque},
    io,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    fs::{self},
    sync::RwLock,
};
use tracing::{error, info, warn};
use uuid::Uuid;
use walkdir::WalkDir;

pub struct EntryManager<P: PersistenceInterface> {
    db: P,
    local_id: Uuid,
    sync_directories: RwLock<HashMap<String, SyncDirectory>>,
    ignore_handler: RwLock<IgnoreHandler>,
    base_dir_path: CanonicalPath,
}

impl<P: PersistenceInterface> EntryManager<P> {
    pub fn new(
        db: P,
        local_id: Uuid,
        sync_directories: HashMap<String, SyncDirectory>,
        base_dir_path: CanonicalPath,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            local_id,
            sync_directories: RwLock::new(sync_directories),
            ignore_handler: RwLock::new(IgnoreHandler::new(base_dir_path.clone())),
            base_dir_path,
        })
    }

    pub async fn init(&self) -> io::Result<()> {
        let mut filesystem_entries = HashMap::new();

        for dir in self.sync_directories.read().await.values() {
            let path = self.base_dir_path.join(&dir.name);

            fs::create_dir_all(&path).await?;

            filesystem_entries.extend(self.build_dir(path).await?);
        }

        self.build_db(filesystem_entries).await;
        Ok(())
    }

    pub async fn build_dir(
        &self,
        dir_path: CanonicalPath,
    ) -> io::Result<HashMap<RelativePath, EntryInfo>> {
        let mut dir_entries = HashMap::new();

        let mut queue = VecDeque::from([dir_path]);
        while let Some(dir_path) = queue.pop_front() {
            let (entries, child_dirs) = self.walk_dir(dir_path).await?;
            dir_entries.extend(entries);
            queue.extend(child_dirs);
        }

        Ok(dir_entries)
    }

    pub async fn walk_dir(
        &self,
        dir_path: CanonicalPath,
    ) -> io::Result<(HashMap<RelativePath, EntryInfo>, Vec<CanonicalPath>)> {
        let mut dir_entries = HashMap::new();
        let mut dir_child_dirs: Vec<CanonicalPath> = Vec::new();

        let gitignore_path = dir_path.join(".gitignore");
        if gitignore_path.exists() {
            self.insert_gitignore(&gitignore_path).await;
        }

        for entry in WalkDir::new(&dir_path)
            .max_depth(1)
            .into_iter()
            .filter_map(Result::ok)
        {
            let canonical = CanonicalPath::new(entry.path())?;
            let relative = RelativePath::new(&canonical, &self.base_dir_path);

            if self.is_ignored(&canonical, &relative).await {
                continue;
            }

            if canonical.is_file() && !is_ds_store(&canonical) {
                dir_entries.insert(
                    relative.clone(),
                    EntryInfo {
                        name: relative,
                        kind: EntryKind::File,
                        hash: Some(compute_hash(&canonical)?),
                        version: HashMap::from([(self.local_id, 0)]),
                    },
                );
            } else if canonical.is_dir() {
                dir_entries.insert(
                    relative.clone(),
                    EntryInfo {
                        name: relative,
                        kind: EntryKind::Directory,
                        hash: None,
                        version: HashMap::from([(self.local_id, 0)]),
                    },
                );

                if canonical != dir_path {
                    dir_child_dirs.push(canonical);
                }
            }
        }

        Ok((dir_entries, dir_child_dirs))
    }

    async fn build_db(&self, filesystem_entries: HashMap<RelativePath, EntryInfo>) {
        let mut db_entries: HashMap<RelativePath, EntryInfo> = self
            .db
            .list_all_entries()
            .await
            .unwrap()
            .into_iter()
            .map(|f| (f.name.clone(), f))
            .collect();

        for (name, entry) in &mut db_entries {
            match filesystem_entries.get(name) {
                Some(fs_entry) if fs_entry.hash != entry.hash => {
                    *entry.version.entry(self.local_id).or_insert(0) += 1;

                    self.db
                        .insert_or_replace_entry(&EntryInfo {
                            name: entry.name.clone(),
                            version: entry.version.clone(),
                            kind: fs_entry.kind.clone(),
                            hash: fs_entry.hash.clone(),
                        })
                        .await
                        .unwrap();
                }

                None => {
                    self.db.delete_entry(&entry.name).await.unwrap();
                }

                _ => {}
            }
        }

        for (name, fs_entry) in filesystem_entries {
            if !db_entries.contains_key(&name) {
                self.db.insert_or_replace_entry(&fs_entry).await.unwrap();
            }
        }
    }

    pub async fn is_sync_dir(&self, name: &str) -> bool {
        self.sync_directories.read().await.contains_key(name)
    }

    pub async fn add_sync_dir(&self, name: &str) -> io::Result<CanonicalPath> {
        let path = self.base_dir_path.join(name);
        fs::create_dir_all(&path).await?;

        let dir_entries = self.build_dir(path.clone()).await?;

        for (_, info) in dir_entries {
            self.insert_entry(info).await;
        }

        self.sync_directories.write().await.insert(
            name.to_string(),
            SyncDirectory {
                name: name.to_string(),
            },
        );
        Ok(path)
    }

    pub async fn list_dirs(&self) -> HashMap<String, SyncDirectory> {
        self.sync_directories.read().await.clone()
    }

    pub async fn is_ignored(&self, path: &CanonicalPath, relative: &RelativePath) -> bool {
        self.ignore_handler.read().await.is_ignored(path, relative)
    }

    pub async fn insert_entry(&self, mut entry: EntryInfo) -> EntryInfo {
        entry.version.entry(self.local_id).or_insert(0);
        self.db.insert_or_replace_entry(&entry).await.unwrap();
        entry
    }

    pub async fn entry_created(
        &self,
        name: &RelativePath,
        kind: EntryKind,
        hash: Option<String>,
    ) -> EntryInfo {
        self.insert_entry(EntryInfo {
            name: name.to_owned(),
            kind,
            hash,
            version: HashMap::from([(self.local_id, 0)]),
        })
        .await
    }

    pub async fn entry_modified(&self, mut entry: EntryInfo, hash: Option<String>) -> EntryInfo {
        entry.hash = hash;
        *entry.version.entry(self.local_id).or_insert(0) += 1;

        self.db.insert_or_replace_entry(&entry).await.unwrap();
        entry
    }

    pub async fn get_entry(&self, name: &str) -> Option<EntryInfo> {
        self.db.get_entry(name).await.unwrap()
    }

    pub async fn get_entries_to_request(
        &self,
        peer: &Peer,
        peer_entries: HashMap<RelativePath, EntryInfo>,
    ) -> io::Result<Vec<EntryInfo>> {
        let mut to_request = Vec::new();

        let dirs = { self.sync_directories.read().await.clone() };

        for (name, peer_entry) in peer_entries {
            if dirs.contains_key(&peer_entry.get_root_parent()) {
                if let Some(mut local_entry) = self.get_entry(&name).await {
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
            self.merge_versions_and_insert(local_entry, peer_entry, peer_id)
                .await;
        }

        Ok(cmp)
    }

    pub async fn handle_conflict(
        &self,
        local_entry: &mut EntryInfo,
        peer_entry: &EntryInfo,
        peer_id: Uuid,
    ) -> io::Result<VersionCmp> {
        warn!(entry = ?local_entry.name, peer = ?peer_id, "[⚠️  CONFLICT]");

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

        let path = self.base_dir_path.join(&*local_entry.name);

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
        match self.get_entry(&peer_entry.name).await {
            Some(mut local_entry) => {
                self.compare_and_resolve_conflict(&mut local_entry, peer_entry, peer_id)
                    .await
            }
            None => Ok(VersionCmp::KeepOther),
        }
    }

    pub async fn merge_versions_and_insert(
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
        self.db.insert_or_replace_entry(local_entry).await.unwrap();
    }

    pub async fn remove_entry(&self, name: &str) -> Option<EntryInfo> {
        if let Some(entry) = self.get_entry(name).await {
            let updated = self.delete_and_update_entry(entry).await;
            Some(updated)
        } else {
            None
        }
    }

    pub async fn remove_dir(&self, removed: &str) -> Vec<EntryInfo> {
        let mut removed_entries = Vec::new();

        let entries = self.db.list_all_entries().await.unwrap();
        for mut entry in entries {
            if entry.name.starts_with(&format!("{}/", removed)) {
                entry = self.delete_and_update_entry(entry).await;
                removed_entries.push(entry);
            }
        }

        removed_entries
    }

    pub async fn delete_and_update_entry(&self, mut entry: EntryInfo) -> EntryInfo {
        self.db.delete_entry(&entry.name).await.unwrap();

        *entry.version.entry(self.local_id).or_insert(0) += 1;
        entry.set_removed_hash();

        entry
    }

    pub async fn get_handshake_data(&self) -> HandshakeData {
        let sync_dirs = self
            .sync_directories
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();

        let entries = self
            .db
            .list_all_entries()
            .await
            .unwrap()
            .into_iter()
            .map(|f| (f.name.clone(), f))
            .collect::<HashMap<RelativePath, EntryInfo>>();

        HandshakeData { sync_dirs, entries }
    }

    pub async fn insert_gitignore(&self, gitignore_path: &CanonicalPath) {
        match self
            .ignore_handler
            .write()
            .await
            .insert_gitignore(gitignore_path)
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

    pub async fn remove_gitignore(&self, relative: &RelativePath) {
        self.ignore_handler.write().await.remove_gitignore(relative);
    }
}
