use super::{app_state::AppState, ignore::IgnoreHandler};
use crate::{
    application::persistence::interface::PersistenceInterface,
    domain::{
        CanonicalPath, EntryInfo, EntryKind, HandshakeData, Peer, RelativePath, SyncDirectory,
        VersionCmp,
    },
    utils::fs::{compute_hash, is_ds_store, is_git_path},
};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    fs::{self},
    io,
};
use tracing::warn;
use uuid::Uuid;
use walkdir::WalkDir;

pub struct EntryManager<P: PersistenceInterface> {
    db: P,
    state: Arc<AppState>,
    ignore_handler: IgnoreHandler,
}

impl<P: PersistenceInterface> EntryManager<P> {
    pub fn new(db: P, state: Arc<AppState>) -> Arc<Self> {
        Arc::new(Self {
            db,
            ignore_handler: IgnoreHandler::new(state.clone()),
            state,
        })
    }

    pub async fn init(&self) -> io::Result<()> {
        let mut filesystem_entries = HashMap::new();

        for dir in self.state.sync_dirs.read().await.values() {
            let path = dir.name.to_canonical(self.state.home_path());

            fs::create_dir_all(&path).await?;

            filesystem_entries.extend(self.build_dir(path).await?);
        }

        self.build_db(filesystem_entries).await
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
            let relative = RelativePath::new(&canonical, self.state.home_path())?;

            if is_git_path(&relative) || self.is_ignored(&canonical, &relative).await {
                continue;
            }

            if canonical.is_file() && !is_ds_store(&canonical) {
                dir_entries.insert(
                    relative.clone(),
                    EntryInfo {
                        name: relative,
                        kind: EntryKind::File,
                        hash: Some(compute_hash(&canonical).await?),
                        version: HashMap::from([(self.state.local_id(), 0)]),
                    },
                );
            } else if canonical.is_dir() {
                dir_entries.insert(
                    relative.clone(),
                    EntryInfo {
                        name: relative,
                        kind: EntryKind::Directory,
                        hash: None,
                        version: HashMap::from([(self.state.local_id(), 0)]),
                    },
                );

                if canonical != dir_path {
                    dir_child_dirs.push(canonical);
                }
            }
        }

        Ok((dir_entries, dir_child_dirs))
    }

    async fn build_db(
        &self,
        filesystem_entries: HashMap<RelativePath, EntryInfo>,
    ) -> io::Result<()> {
        let mut db_entries: HashMap<RelativePath, EntryInfo> = self
            .db
            .list_all_entries()
            .await?
            .into_iter()
            .map(|f| (f.name.clone(), f))
            .collect();

        let sync_dirs = { self.state.sync_dirs.read().await.clone() };

        for (name, entry) in &mut db_entries {
            if !sync_dirs.contains_key(&entry.get_sync_dir()) {
                self.db.delete_entry(name).await?;
                continue;
            }

            match filesystem_entries.get(name) {
                Some(fs_entry) if fs_entry.hash != entry.hash => {
                    *entry.version.entry(self.state.local_id()).or_insert(0) += 1;

                    self.db
                        .insert_or_replace_entry(&EntryInfo {
                            name: name.clone(),
                            version: entry.version.clone(),
                            kind: fs_entry.kind.clone(),
                            hash: fs_entry.hash.clone(),
                        })
                        .await?;
                }

                None => {
                    self.db.delete_entry(name).await?;
                }

                _ => {}
            }
        }

        for (name, fs_entry) in filesystem_entries {
            if !db_entries.contains_key(&name) {
                self.db.insert_or_replace_entry(&fs_entry).await?;
            }
        }
        Ok(())
    }

    pub async fn add_sync_dir(&self, name: RelativePath) -> io::Result<()> {
        let path = name.to_canonical(self.state.home_path());
        fs::create_dir_all(&path).await?;

        let dir_entries = self.build_dir(path.clone()).await?;

        for (_, info) in dir_entries {
            self.insert_entry(info).await?;
        }

        self.state
            .sync_dirs
            .write()
            .await
            .insert(name.clone(), SyncDirectory { name });
        Ok(())
    }

    pub async fn remove_sync_dir(&self, name: &RelativePath) -> io::Result<bool> {
        if self.state.sync_dirs.write().await.remove(name).is_some() {
            self.remove_dir(name).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn list_dirs(&self) -> HashMap<RelativePath, SyncDirectory> {
        self.state.sync_dirs.read().await.clone()
    }

    pub async fn is_ignored(&self, path: &CanonicalPath, relative: &RelativePath) -> bool {
        self.ignore_handler.is_ignored(path, relative).await
    }

    pub async fn insert_entry(&self, mut entry: EntryInfo) -> io::Result<EntryInfo> {
        entry.version.entry(self.state.local_id()).or_insert(0);
        self.db.insert_or_replace_entry(&entry).await?;
        Ok(entry)
    }

    pub async fn entry_created(
        &self,
        name: &RelativePath,
        kind: EntryKind,
        hash: Option<String>,
    ) -> io::Result<EntryInfo> {
        self.insert_entry(EntryInfo {
            name: name.to_owned(),
            kind,
            hash,
            version: HashMap::from([(self.state.local_id(), 0)]),
        })
        .await
    }

    pub async fn entry_modified(
        &self,
        mut entry: EntryInfo,
        hash: Option<String>,
    ) -> io::Result<EntryInfo> {
        entry.hash = hash;
        *entry.version.entry(self.state.local_id()).or_insert(0) += 1;

        self.db.insert_or_replace_entry(&entry).await?;
        Ok(entry)
    }

    pub async fn get_entry(&self, name: &str) -> io::Result<Option<EntryInfo>> {
        let entry = self.db.get_entry(name).await?;
        Ok(entry)
    }

    pub async fn get_entries_to_request(
        &self,
        peer: &Peer,
        peer_entries: HashMap<RelativePath, EntryInfo>,
    ) -> io::Result<Vec<EntryInfo>> {
        let mut to_request = Vec::new();

        let dirs = { self.state.sync_dirs.read().await.clone() };

        for (name, peer_entry) in peer_entries {
            if is_git_path(&name) {
                continue;
            }

            if dirs.contains_key(&peer_entry.get_sync_dir()) {
                if let Some(mut local_entry) = self.get_entry(&name).await? {
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
                .await?;
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

        if self.state.local_id() < peer_id {
            return Ok(VersionCmp::KeepSelf);
        }

        let path = local_entry.name.to_canonical(self.state.home_path());

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
            stem,
            now,
            self.state.local_id(),
            ext
        ));

        fs::copy(path, new_path).await?;

        Ok(VersionCmp::KeepOther)
    }

    pub async fn handle_metadata(
        &self,
        peer_id: Uuid,
        peer_entry: &EntryInfo,
    ) -> io::Result<VersionCmp> {
        if is_git_path(&peer_entry.name) {
            return Ok(VersionCmp::KeepSelf);
        }

        match self.get_entry(&peer_entry.name).await? {
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
    ) -> io::Result<()> {
        local_entry.version.entry(peer_id).or_insert(0);

        for (pid, pv) in &peer_entry.version {
            let local_version = local_entry.version.entry(*pid).or_insert(0);
            *local_version = (*local_version).max(*pv);
        }

        self.db.insert_or_replace_entry(local_entry).await?;
        Ok(())
    }

    pub async fn remove_entry(&self, name: &str) -> io::Result<Option<EntryInfo>> {
        if let Some(entry) = self.get_entry(name).await? {
            let updated = self.delete_and_update_entry(entry).await?;
            Ok(Some(updated))
        } else {
            Ok(None)
        }
    }

    pub async fn remove_dir(&self, removed: &str) -> io::Result<Vec<EntryInfo>> {
        let mut removed_entries = Vec::new();

        let entries = self.db.list_all_entries().await?;
        let removed_path: RelativePath = removed.into();
        for mut entry in entries {
            if entry.name.starts_with_dir(&removed_path) {
                entry = self.delete_and_update_entry(entry).await?;
                removed_entries.push(entry);
            }
        }

        Ok(removed_entries)
    }

    pub async fn delete_and_update_entry(&self, mut entry: EntryInfo) -> io::Result<EntryInfo> {
        self.db.delete_entry(&entry.name).await?;

        *entry.version.entry(self.state.local_id()).or_insert(0) += 1;
        entry.set_removed_hash();

        Ok(entry)
    }

    pub async fn get_handshake_data(&self) -> io::Result<HandshakeData> {
        let sync_dirs = self
            .state
            .sync_dirs
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();

        let entries = self
            .db
            .list_all_entries()
            .await?
            .into_iter()
            .filter(|f| !is_git_path(&f.name))
            .map(|f| (f.name.clone(), f))
            .collect::<HashMap<RelativePath, EntryInfo>>();

        Ok(HandshakeData {
            sync_dirs,
            entries,
            instance_id: self.state.instance_id(),
            hostname: self.state.hostname().clone(),
        })
    }

    pub async fn insert_gitignore(&self, gitignore_path: &CanonicalPath) {
        self.ignore_handler.insert_gitignore(gitignore_path).await;
    }

    pub async fn remove_gitignore(&self, relative: &RelativePath) {
        self.ignore_handler.remove_gitignore(relative).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::persistence::sqlite::SqliteDb;
    use std::fs;
    use std::net::{IpAddr, Ipv4Addr};
    use tempfile::TempDir;
    use uuid::Uuid;

    async fn setup() -> (
        crate::utils::test_support::TestEnv,
        TempDir,
        CanonicalPath,
        Arc<EntryManager<SqliteDb>>,
    ) {
        let env = crate::utils::test_support::test_env().await;
        // Create a sub-dir inside the test's isolated home so the test gets
        // a sync-dir scaffold that does not collide with other tests.
        let temp_dir = TempDir::new_in(env.home_path()).unwrap();
        let sync_dir = CanonicalPath::from_absolute(temp_dir.path());

        let db = SqliteDb::new(":memory:").await.unwrap();
        let manager = EntryManager::new(db, env.state.clone());

        (env, temp_dir, sync_dir, manager)
    }

    async fn add_sync_dir(
        manager: &Arc<EntryManager<SqliteDb>>,
        sync_dir: &CanonicalPath,
    ) -> RelativePath {
        let relative = RelativePath::new(sync_dir, manager.state.home_path()).unwrap();
        manager.state.sync_dirs.write().await.insert(
            relative.clone(),
            SyncDirectory {
                name: relative.clone(),
            },
        );
        relative
    }

    fn entry(name: RelativePath, hash: Option<&str>, peer_id: Uuid) -> EntryInfo {
        EntryInfo {
            name,
            kind: EntryKind::File,
            hash: hash.map(str::to_string),
            version: HashMap::from([(peer_id, 1)]),
        }
    }

    #[tokio::test]
    async fn build_dir_excludes_git_directory() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;

        fs::write(sync_dir.join("notes.txt"), "hello").unwrap();
        fs::write(sync_dir.join(".gitignore"), "*.log").unwrap();
        fs::write(sync_dir.join(".gitattributes"), "* text=auto").unwrap();

        fs::create_dir_all(sync_dir.join(".git/objects/ab")).unwrap();
        fs::write(sync_dir.join(".git/HEAD"), "ref: refs/heads/main").unwrap();
        fs::write(sync_dir.join(".git/config"), "[core]").unwrap();
        fs::write(sync_dir.join(".git/objects/ab/cdef"), "obj").unwrap();

        let entries = manager.build_dir(sync_dir).await.unwrap();

        let names: Vec<&str> = entries.keys().map(|p| p.as_ref()).collect();

        assert!(names.iter().any(|n| n.ends_with("/notes.txt")));
        assert!(names.iter().any(|n| n.ends_with("/.gitignore")));
        assert!(names.iter().any(|n| n.ends_with("/.gitattributes")));

        for name in &names {
            assert!(
                !name.contains("/.git/") && !name.ends_with("/.git"),
                "unexpected .git entry: {name}"
            );
        }
    }

    #[tokio::test]
    async fn get_entries_to_request_ignores_git_peer_entries() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let peer_id = Uuid::new_v4();
        let peer = Peer::new(
            peer_id,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            "peer".to_string(),
            Uuid::new_v4(),
            vec![SyncDirectory {
                name: sync_root.clone(),
            }],
        );

        let git_name: RelativePath = format!("{}/.git/config", &*sync_root).into();
        let normal_name: RelativePath = format!("{}/notes.txt", &*sync_root).into();
        let git_entry = entry(git_name.clone(), Some("git-hash"), peer_id);
        let normal_entry = entry(normal_name.clone(), Some("notes-hash"), peer_id);
        let peer_entries =
            HashMap::from([(git_name, git_entry), (normal_name.clone(), normal_entry)]);

        let entries = manager
            .get_entries_to_request(&peer, peer_entries)
            .await
            .unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, normal_name);
    }

    #[tokio::test]
    async fn handle_metadata_ignores_git_peer_entries() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let peer_id = Uuid::new_v4();
        let git_name: RelativePath = format!("{}/.git/config", &*sync_root).into();
        let peer_entry = entry(git_name.clone(), Some("git-hash"), peer_id);

        let cmp = manager.handle_metadata(peer_id, &peer_entry).await.unwrap();

        assert!(matches!(cmp, VersionCmp::KeepSelf));
        assert!(manager.get_entry(&git_name).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_handshake_data_excludes_stale_git_entries() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let peer_id = Uuid::new_v4();
        let git_name: RelativePath = format!("{}/.git/config", &*sync_root).into();
        let normal_name: RelativePath = format!("{}/notes.txt", &*sync_root).into();

        manager
            .insert_entry(entry(git_name.clone(), Some("git-hash"), peer_id))
            .await
            .unwrap();
        manager
            .insert_entry(entry(normal_name.clone(), Some("notes-hash"), peer_id))
            .await
            .unwrap();

        let data = manager.get_handshake_data().await.unwrap();

        assert!(!data.entries.contains_key(&git_name));
        assert!(data.entries.contains_key(&normal_name));
    }
}
