use super::{app_state::AppState, ignore::IgnoreHandler};
use crate::{
    application::persistence::interface::PersistenceInterface,
    domain::{
        CanonicalPath, EntryInfo, EntryKind, HandshakeData, MAX_TRUSTED_COUNTER, Peer,
        RelativePath, StagedTransfer, SyncDirectory, VersionCmp,
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
use tracing::{trace, warn};
use uuid::Uuid;
use walkdir::WalkDir;

/// Outcome of `EntryManager::commit_staged_transfer`.
///
/// Issue #33 B1 — the application-layer commit decides whether the
/// staged bytes earn the rename into `home_path` based on the local
/// view of the entry. `Committed` returns the new local entry so the
/// caller can broadcast metadata; `Dropped` carries a short, GUI-
/// suitable reason for the `EntrySyncFailed` SSE.
#[derive(Debug)]
pub enum CommitOutcome {
    Committed(EntryInfo),
    Dropped(&'static str),
}

enum CommitAction {
    Apply,
    Drop(&'static str),
}

/// Owns the lifecycle of synchronized filesystem entries.
///
/// Combines a `PersistenceInterface` (durable metadata store), the
/// shared `AppState` (sync directories, home path, device id), and an
/// `IgnoreHandler` (`.gitignore` rules) to scan the home directory at
/// startup, react to local file events, and reconcile metadata that
/// arrives from peers — including materializing conflict files when
/// `VersionCmp::Conflict` is detected.
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

    /// Scans every configured sync directory, then reconciles the
    /// on-disk view with the persisted entries — creating missing
    /// directories, hashing files, and seeding version vectors for
    /// fresh entries. Called once at startup.
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
                    bump_local_counter(&mut entry.version, self.state.local_id())?;

                    self.db
                        .insert_or_replace_entry(&EntryInfo {
                            name: name.clone(),
                            version: entry.version.clone(),
                            kind: fs_entry.kind.clone(),
                            hash: fs_entry.hash.clone(),
                        })
                        .await?;
                }

                None if entry.is_removed() => {
                    // Tombstones intentionally have no on-disk file; keep
                    // the row so the deletion stays durable across restart
                    // and continues to propagate to peers (issue #33 B3).
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
        trace!(entry = %entry.name, "inserting entry");
        self.db.insert_or_replace_entry(&entry).await?;
        Ok(entry)
    }

    /// Persist a peer-supplied entry after sanitizing its version vector.
    ///
    /// Drops all foreign axes (only `entry.version[peer_id]` is trusted)
    /// and rejects counters above `MAX_TRUSTED_COUNTER` as poisoned. The
    /// same rule that `merge_versions_and_insert` applies on the
    /// `Equal | KeepSelf` branch, applied at the Transfer /
    /// directory-create boundary so a peer cannot poison foreign axes
    /// or write `u64::MAX` counters into the DB.
    ///
    /// If a row already exists, its trusted local vector is preserved
    /// and only the sender's own axis is merged from the inbound entry.
    /// This keeps the accepted peer copy from erasing local history,
    /// which would make later local edits look stale against an older
    /// peer advertisement.
    ///
    /// Returns `Ok(None)` if the entry was dropped (warn-and-drop),
    /// `Ok(Some(entry))` after a successful persist.
    pub async fn insert_peer_entry(
        &self,
        peer_id: Uuid,
        mut entry: EntryInfo,
    ) -> io::Result<Option<EntryInfo>> {
        let Some(sanitized) = Self::sanitize_peer_entry(peer_id, &entry) else {
            return Ok(None);
        };

        let mut version = self
            .get_entry(&entry.name)
            .await?
            .map(|stored| stored.version)
            .unwrap_or_default();
        let pv = sanitized.version.get(&peer_id).copied().unwrap_or(0);
        let peer_version = version.entry(peer_id).or_insert(0);
        *peer_version = (*peer_version).max(pv);
        version.entry(self.state.local_id()).or_insert(0);
        entry.version = version;
        trace!(entry = %entry.name, peer = %peer_id, "inserting peer entry");
        self.db.insert_or_replace_entry(&entry).await?;
        Ok(Some(entry))
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
        bump_local_counter(&mut entry.version, self.state.local_id())?;

        self.db.insert_or_replace_entry(&entry).await?;
        Ok(entry)
    }

    pub async fn get_entry(&self, name: &str) -> io::Result<Option<EntryInfo>> {
        let entry = self.db.get_entry(name).await?;
        Ok(entry)
    }

    /// Given a peer's full entry map (typically delivered in a
    /// handshake), returns the subset that we should request from
    /// them — entries we don't have, or entries where the peer's
    /// version dominates ours after conflict resolution.
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
                let Some(peer_entry) = Self::sanitize_peer_entry(peer.id, &peer_entry) else {
                    continue;
                };

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

    /// Compares the local and peer copies of an entry and, if the
    /// result is `Conflict`, defers to `handle_conflict` to decide a
    /// winner (and possibly write a conflict file). When the local
    /// side wins or both sides agree without a conflict, also merges
    /// the peer's version counters into the local entry so future
    /// comparisons converge.
    ///
    /// Issue #33 B2: never merge the peer's axis when the raw compare
    /// was `Conflict` and the tiebreak resolved to `KeepSelf`. The peer
    /// announced a counter under an axis whose content we never
    /// integrated — absorbing it would make our vector dominate the
    /// peer's on the next exchange, and the peer would then silently
    /// overwrite its own edit (no conflict file on either side).
    /// Leaving our vector untouched lets the peer re-detect the
    /// conflict from its side and preserve its edit via the existing
    /// `KeepOther` conflict-file path.
    #[tracing::instrument(skip_all, fields(entry = %local_entry.name, peer = %peer_id))]
    pub async fn compare_and_resolve_conflict(
        &self,
        local_entry: &mut EntryInfo,
        peer_entry: &EntryInfo,
        peer_id: Uuid,
    ) -> io::Result<VersionCmp> {
        let raw = local_entry.compare(peer_entry);
        let cmp = match raw {
            VersionCmp::Conflict => {
                self.handle_conflict(local_entry, peer_entry, peer_id)
                    .await?
            }
            other => other,
        };

        let should_merge = matches!(raw, VersionCmp::Equal)
            || (matches!(cmp, VersionCmp::KeepSelf) && !matches!(raw, VersionCmp::Conflict));
        if should_merge {
            self.merge_versions_and_insert(local_entry, peer_entry, peer_id)
                .await?;
        }

        Ok(cmp)
    }

    /// Resolves a true concurrent-edit conflict.
    ///
    /// Removal-vs-live takes a fixed tiebreak (the live side wins);
    /// otherwise the lower `local_id` wins to give a deterministic,
    /// peer-agnostic choice. If the local side must give way, the
    /// existing file is copied to `<stem>_CONFLICT_<unix>_<id>.<ext>`
    /// so no user data is lost before the peer's version is adopted.
    #[tracing::instrument(skip_all, fields(entry = %local_entry.name, peer = %peer_id))]
    pub async fn handle_conflict(
        &self,
        local_entry: &mut EntryInfo,
        peer_entry: &EntryInfo,
        peer_id: Uuid,
    ) -> io::Result<VersionCmp> {
        warn!("version conflict");

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

    /// Reconciles a single inbound metadata message: drops it if the
    /// path is excluded, requests/keeps based on
    /// `compare_and_resolve_conflict` if the entry exists locally, or
    /// declares the remote version the winner if we've never seen it.
    pub async fn handle_metadata(
        &self,
        peer_id: Uuid,
        peer_entry: &EntryInfo,
    ) -> io::Result<VersionCmp> {
        if is_git_path(&peer_entry.name) {
            return Ok(VersionCmp::KeepSelf);
        }

        let Some(peer_entry) = Self::sanitize_peer_entry(peer_id, peer_entry) else {
            return Ok(VersionCmp::KeepSelf);
        };

        match self.get_entry(&peer_entry.name).await? {
            Some(mut local_entry) => {
                self.compare_and_resolve_conflict(&mut local_entry, &peer_entry, peer_id)
                    .await
            }
            None => Ok(VersionCmp::KeepOther),
        }
    }

    fn sanitize_peer_entry(peer_id: Uuid, entry: &EntryInfo) -> Option<EntryInfo> {
        let pv = entry.version.get(&peer_id).copied().unwrap_or(0);
        if pv > MAX_TRUSTED_COUNTER {
            warn!(
                entry = %entry.name,
                peer = %peer_id,
                counter = pv,
                "rejecting poisoned peer version counter"
            );
            return None;
        }

        let mut sanitized = entry.clone();
        sanitized.version = entry
            .version
            .get(&peer_id)
            .map(|pv| HashMap::from([(peer_id, *pv)]))
            .unwrap_or_default();
        Some(sanitized)
    }

    /// Merges the peer's version counter for **its own axis** into
    /// the local copy and persists.
    ///
    /// Only `peer_entry.version[peer_id]` is trusted — foreign axes in
    /// the inbound vector are ignored to prevent a buggy or hostile
    /// peer from inflating another device's counter. A counter above
    /// `MAX_TRUSTED_COUNTER` is treated as poisoned and the merge is
    /// skipped (warn-and-drop) instead of erroring, so one bad entry
    /// cannot tear down the connection.
    pub async fn merge_versions_and_insert(
        &self,
        local_entry: &mut EntryInfo,
        peer_entry: &EntryInfo,
        peer_id: Uuid,
    ) -> io::Result<()> {
        local_entry.version.entry(peer_id).or_insert(0);

        if let Some(&pv) = peer_entry.version.get(&peer_id) {
            if pv > MAX_TRUSTED_COUNTER {
                warn!(
                    entry = %local_entry.name,
                    peer = %peer_id,
                    counter = pv,
                    "rejecting poisoned peer version counter"
                );
                return Ok(());
            }
            let local_version = local_entry.version.entry(peer_id).or_insert(0);
            *local_version = (*local_version).max(pv);
        }

        trace!(entry = %local_entry.name, peer = %peer_id, "merging versions");
        self.db.insert_or_replace_entry(local_entry).await?;
        Ok(())
    }

    /// Commit a staged Transfer into `home_path` after re-validating
    /// against the current local view.
    ///
    /// Issue #33 B1: the TCP adapter writes the verified payload to a
    /// staging directory; this method is the application-layer commit
    /// gate. It runs `EntryInfo::compare` (with the peer entry
    /// sanitized to the sender's own axis) and:
    ///
    /// - on `Equal | KeepOther` or `Conflict → KeepOther`: atomically
    ///   renames the staging file into the user's tree and persists
    ///   the sanitized peer metadata via `insert_peer_entry`. For the
    ///   `Conflict → KeepOther` case, also copies the local file aside
    ///   as a `_CONFLICT_` artefact first.
    /// - on `KeepSelf` (including `Conflict → KeepSelf`): drops the
    ///   staged bytes, leaves both DB and disk untouched. This is the
    ///   B2-aligned no-merge-on-conflict-keep-self path.
    ///
    /// The caller MUST hold the per-entry inflight lock around this
    /// call so two concurrent commits for the same path cannot
    /// interleave.
    #[tracing::instrument(skip_all, fields(entry = %peer_entry.name, peer = %peer_id))]
    pub async fn commit_staged_transfer(
        &self,
        peer_id: Uuid,
        peer_entry: EntryInfo,
        mut staging: StagedTransfer,
    ) -> io::Result<CommitOutcome> {
        let Some(sanitized) = Self::sanitize_peer_entry(peer_id, &peer_entry) else {
            return Ok(CommitOutcome::Dropped("peer entry rejected by sanitizer"));
        };

        // Determine the local view. Equal/KeepOther → commit; KeepSelf
        // → drop. Conflict tiebreaks live in `handle_conflict`.
        let local_entry = self.get_entry(&sanitized.name).await?;

        let action = match &local_entry {
            None => CommitAction::Apply,
            Some(local) => match local.compare(&sanitized) {
                VersionCmp::Equal | VersionCmp::KeepOther => CommitAction::Apply,
                VersionCmp::KeepSelf => CommitAction::Drop("local newer than peer"),
                VersionCmp::Conflict => {
                    let mut local_clone = local.clone();
                    match self
                        .handle_conflict(&mut local_clone, &sanitized, peer_id)
                        .await?
                    {
                        VersionCmp::KeepOther => CommitAction::Apply,
                        VersionCmp::KeepSelf | VersionCmp::Equal => {
                            CommitAction::Drop("local wins conflict tiebreak")
                        }
                        VersionCmp::Conflict => CommitAction::Drop("conflict unresolved"),
                    }
                }
            },
        };

        match action {
            CommitAction::Drop(reason) => Ok(CommitOutcome::Dropped(reason)),
            CommitAction::Apply => {
                let target = sanitized.name.to_canonical(self.state.home_path());
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).await?;
                }

                let staging_path = staging
                    .take_path()
                    .ok_or_else(|| io::Error::other("staged transfer has no file"))?;
                match fs::rename(&staging_path, &target).await {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
                        fs::copy(&staging_path, &target).await?;
                        fs::remove_file(&staging_path).await?;
                    }
                    Err(e) => return Err(e),
                }
                // staging's Drop will rmdir the now-empty staging root.

                match self.insert_peer_entry(peer_id, sanitized).await? {
                    Some(persisted) => Ok(CommitOutcome::Committed(persisted)),
                    None => Ok(CommitOutcome::Dropped("peer entry rejected post-rename")),
                }
            }
        }
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

    /// Marks an entry as deleted and persists the tombstone.
    ///
    /// The row is kept in the DB with `REMOVED_HASH` and a bumped local
    /// counter so the deletion is durable across restarts and propagates
    /// to peers via the handshake entry list. A plain `delete_entry`
    /// would lose the tombstone the moment we crash or a late-joining
    /// peer connects, letting the file silently resurrect from any peer
    /// that still has the live copy. (issue #33 finding B3)
    pub async fn delete_and_update_entry(&self, mut entry: EntryInfo) -> io::Result<EntryInfo> {
        bump_local_counter(&mut entry.version, self.state.local_id())?;
        entry.set_removed_hash();

        self.db.insert_or_replace_entry(&entry).await?;
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

/// Increment the local axis of a version vector with overflow checking.
///
/// With foreign-axis poisoning prevented in `merge_versions_and_insert`,
/// this counter only grows via honest local edits, so overflow is
/// unreachable in practice. `checked_add` is a cheap defense-in-depth.
fn bump_local_counter(
    version: &mut crate::domain::VersionVector,
    local_id: Uuid,
) -> io::Result<()> {
    let v = version.entry(local_id).or_insert(0);
    *v = v
        .checked_add(1)
        .ok_or_else(|| io::Error::other("version counter overflow"))?;
    Ok(())
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
    async fn get_entries_to_request_ignores_foreign_axes_before_compare() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let local_id = manager.state.local_id();
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
        let name = dir_relative(&sync_root, "notes.txt");

        manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("local-hash".into()),
                version: HashMap::from([(local_id, 2), (peer_id, 1)]),
            })
            .await
            .unwrap();

        let peer_entry = EntryInfo {
            name: name.clone(),
            kind: EntryKind::File,
            hash: Some("peer-hash".into()),
            version: HashMap::from([(local_id, 99), (peer_id, 1)]),
        };

        let entries = manager
            .get_entries_to_request(&peer, HashMap::from([(name, peer_entry)]))
            .await
            .unwrap();

        assert!(
            entries.is_empty(),
            "foreign local-axis claim must not make peer entry win"
        );
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

    /// A directory peer-vs-self conflict can't be materialized as a file,
    /// so it falls through to the peer-wins branch. Use files so we can
    /// also assert the on-disk conflict-file side effect.
    fn dir_relative(sync_root: &RelativePath, leaf: &str) -> RelativePath {
        format!("{}/{}", &**sync_root, leaf).into()
    }

    #[tokio::test]
    async fn entry_modified_bumps_local_version_counter() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let local_id = manager.state.local_id();

        let initial = manager
            .insert_entry(EntryInfo {
                name: dir_relative(&sync_root, "file.txt"),
                kind: EntryKind::File,
                hash: Some("v1".into()),
                version: HashMap::from([(local_id, 3)]),
            })
            .await
            .unwrap();

        let bumped = manager
            .entry_modified(initial, Some("v2".into()))
            .await
            .unwrap();

        assert_eq!(bumped.version.get(&local_id), Some(&4));
        assert_eq!(bumped.hash.as_deref(), Some("v2"));
    }

    #[tokio::test]
    async fn compare_and_resolve_conflict_keeps_self_when_local_id_lower_than_peer() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let local_id = manager.state.local_id();
        // Force peer_id > local_id so the deterministic tiebreak keeps self.
        let peer_id = Uuid::from_u128(u128::MAX);

        let name = dir_relative(&sync_root, "doc.txt");
        let mut local = EntryInfo {
            name: name.clone(),
            kind: EntryKind::File,
            hash: Some("local-hash".into()),
            version: HashMap::from([(local_id, 1)]),
        };
        let peer = EntryInfo {
            name,
            kind: EntryKind::File,
            hash: Some("peer-hash".into()),
            version: HashMap::from([(peer_id, 1)]),
        };

        let cmp = manager
            .compare_and_resolve_conflict(&mut local, &peer, peer_id)
            .await
            .unwrap();

        assert!(matches!(cmp, VersionCmp::KeepSelf));
    }

    #[tokio::test]
    async fn handle_conflict_writes_conflict_file_when_local_id_higher_and_file_exists() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        // peer_id < any real Uuid::new_v4 so local must give way.
        let peer_id = Uuid::nil();

        let rel: RelativePath = dir_relative(&sync_root, "report.md");
        let absolute = rel.to_canonical(manager.state.home_path());
        fs::write(&absolute, b"local contents").unwrap();

        let mut local = EntryInfo {
            name: rel.clone(),
            kind: EntryKind::File,
            hash: Some("local-hash".into()),
            version: HashMap::from([(manager.state.local_id(), 1)]),
        };
        let peer = EntryInfo {
            name: rel,
            kind: EntryKind::File,
            hash: Some("peer-hash".into()),
            version: HashMap::from([(peer_id, 1)]),
        };

        let cmp = manager
            .handle_conflict(&mut local, &peer, peer_id)
            .await
            .unwrap();

        assert!(matches!(cmp, VersionCmp::KeepOther));

        let siblings: Vec<String> = fs::read_dir(&sync_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert!(
            siblings.iter().any(|n| n.contains("_CONFLICT_")),
            "expected a _CONFLICT_ file in {sync_dir:?}, found: {siblings:?}",
        );
    }

    #[tokio::test]
    async fn handle_conflict_removed_local_vs_live_peer_keeps_peer() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let peer_id = Uuid::new_v4();
        let name = dir_relative(&sync_root, "x.txt");

        let mut local = EntryInfo {
            name: name.clone(),
            kind: EntryKind::File,
            hash: None,
            version: HashMap::from([(manager.state.local_id(), 1)]),
        };
        local.set_removed_hash();

        let peer = EntryInfo {
            name,
            kind: EntryKind::File,
            hash: Some("live-peer".into()),
            version: HashMap::from([(peer_id, 1)]),
        };

        let cmp = manager
            .handle_conflict(&mut local, &peer, peer_id)
            .await
            .unwrap();
        assert!(matches!(cmp, VersionCmp::KeepOther));
    }

    #[tokio::test]
    async fn handle_conflict_live_local_vs_removed_peer_keeps_local() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let peer_id = Uuid::new_v4();
        let name = dir_relative(&sync_root, "y.txt");

        let mut local = EntryInfo {
            name: name.clone(),
            kind: EntryKind::File,
            hash: Some("live-local".into()),
            version: HashMap::from([(manager.state.local_id(), 1)]),
        };
        let mut peer = EntryInfo {
            name,
            kind: EntryKind::File,
            hash: None,
            version: HashMap::from([(peer_id, 1)]),
        };
        peer.set_removed_hash();

        let cmp = manager
            .handle_conflict(&mut local, &peer, peer_id)
            .await
            .unwrap();
        assert!(matches!(cmp, VersionCmp::KeepSelf));
    }

    #[tokio::test]
    async fn handle_metadata_unknown_entry_returns_keep_other() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let peer_id = Uuid::new_v4();
        let peer_entry = entry(
            dir_relative(&sync_root, "new.txt"),
            Some("peer-hash"),
            peer_id,
        );

        let cmp = manager.handle_metadata(peer_id, &peer_entry).await.unwrap();
        assert!(matches!(cmp, VersionCmp::KeepOther));
    }

    /// When the local copy already dominates on every axis, the peer's
    /// metadata is rejected (no overwrite, no conflict file).
    #[tokio::test]
    async fn handle_metadata_local_strictly_newer_returns_keep_self() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let local_id = manager.state.local_id();
        let peer_id = Uuid::new_v4();
        let name = dir_relative(&sync_root, "doc.txt");

        manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("new-local".into()),
                version: HashMap::from([(local_id, 5), (peer_id, 1)]),
            })
            .await
            .unwrap();

        let peer_entry = EntryInfo {
            name,
            kind: EntryKind::File,
            hash: Some("old-peer".into()),
            version: HashMap::from([(local_id, 3), (peer_id, 1)]),
        };

        let cmp = manager.handle_metadata(peer_id, &peer_entry).await.unwrap();
        assert!(matches!(cmp, VersionCmp::KeepSelf));
    }

    #[tokio::test]
    async fn handle_metadata_ignores_foreign_axes_before_compare() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let local_id = manager.state.local_id();
        let peer_id = Uuid::new_v4();
        let name = dir_relative(&sync_root, "doc.txt");

        manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("local-hash".into()),
                version: HashMap::from([(local_id, 2), (peer_id, 1)]),
            })
            .await
            .unwrap();

        let peer_entry = EntryInfo {
            name,
            kind: EntryKind::File,
            hash: Some("peer-hash".into()),
            // This would force KeepOther if compared before sanitizing.
            version: HashMap::from([(local_id, 99), (peer_id, 1)]),
        };

        let cmp = manager.handle_metadata(peer_id, &peer_entry).await.unwrap();

        assert!(matches!(cmp, VersionCmp::KeepSelf));
    }

    /// After a non-conflict handle_metadata, only the peer's own axis
    /// (`peer_entry.version[peer_id]`) is merged into the local vector.
    /// Foreign axes the peer claims to know about are ignored, because
    /// an unauthenticated peer can advertise arbitrary values for them
    /// (issue #32 finding #3).
    #[tokio::test]
    async fn handle_metadata_merges_only_peer_own_axis() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let local_id = manager.state.local_id();
        let peer_id = Uuid::new_v4();
        let third_id = Uuid::new_v4();
        let name = dir_relative(&sync_root, "shared.txt");

        manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("same-hash".into()),
                version: HashMap::from([(local_id, 2)]),
            })
            .await
            .unwrap();

        let peer_entry = EntryInfo {
            name: name.clone(),
            kind: EntryKind::File,
            hash: Some("same-hash".into()),
            version: HashMap::from([(local_id, 1), (peer_id, 4), (third_id, 7)]),
        };

        let cmp = manager.handle_metadata(peer_id, &peer_entry).await.unwrap();
        assert!(matches!(cmp, VersionCmp::Equal));

        let stored = manager.get_entry(&name).await.unwrap().unwrap();
        assert_eq!(stored.version.get(&local_id), Some(&2));
        assert_eq!(stored.version.get(&peer_id), Some(&4));
        // Third-device axis must NOT have been adopted from the peer's
        // claim — we'd only learn it by hearing from `third_id` directly.
        assert!(
            !stored.version.contains_key(&third_id),
            "foreign axis must not be merged from peer report"
        );
    }

    /// A peer-supplied counter above `MAX_TRUSTED_COUNTER` is treated as
    /// poisoned: the merge is skipped (warn-and-drop) so future
    /// legitimate updates from that device don't look stale forever
    /// (issue #32 finding #5).
    #[tokio::test]
    async fn merge_versions_skips_poisoned_peer_counter() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let local_id = manager.state.local_id();
        let peer_id = Uuid::new_v4();
        let name = dir_relative(&sync_root, "shared.txt");

        manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("same-hash".into()),
                version: HashMap::from([(local_id, 2)]),
            })
            .await
            .unwrap();

        let peer_entry = EntryInfo {
            name: name.clone(),
            kind: EntryKind::File,
            hash: Some("same-hash".into()),
            version: HashMap::from([(peer_id, u64::MAX)]),
        };

        // Equal kind + hash would normally converge metadata, but the
        // poisoned sender-owned counter must be dropped before compare
        // or persistence can write `u64::MAX`.
        manager.handle_metadata(peer_id, &peer_entry).await.unwrap();

        let stored = manager.get_entry(&name).await.unwrap().unwrap();
        let observed = stored.version.get(&peer_id).copied().unwrap_or(0);
        assert!(
            observed < crate::domain::MAX_TRUSTED_COUNTER,
            "poisoned counter must not be persisted; got {observed}"
        );
    }

    #[tokio::test]
    async fn insert_peer_entry_preserves_existing_local_axis() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let local_id = manager.state.local_id();
        let peer_id = Uuid::new_v4();
        let third_id = Uuid::new_v4();
        let name = dir_relative(&sync_root, "shared.txt");

        manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("local-old".into()),
                version: HashMap::from([(local_id, 5), (peer_id, 2)]),
            })
            .await
            .unwrap();

        let accepted_peer_entry = EntryInfo {
            name: name.clone(),
            kind: EntryKind::File,
            hash: Some("peer-copy".into()),
            version: HashMap::from([(peer_id, 3), (third_id, 99)]),
        };

        let stored = manager
            .insert_peer_entry(peer_id, accepted_peer_entry)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(stored.version.get(&local_id), Some(&5));
        assert_eq!(stored.version.get(&peer_id), Some(&3));
        assert!(
            !stored.version.contains_key(&third_id),
            "foreign axis must not be persisted from peer entry"
        );

        let locally_modified = manager
            .entry_modified(stored, Some("local-new".into()))
            .await
            .unwrap();
        let stale_peer_entry = EntryInfo {
            name,
            kind: EntryKind::File,
            hash: Some("peer-copy".into()),
            version: HashMap::from([(peer_id, 3)]),
        };

        assert!(
            matches!(
                locally_modified.compare(&stale_peer_entry),
                VersionCmp::KeepSelf
            ),
            "local edit after accepting a transfer must not look stale"
        );
    }

    #[tokio::test]
    async fn remove_dir_marks_all_descendants_as_removed() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let peer_id = Uuid::new_v4();

        for leaf in ["a.txt", "sub/b.txt", "sub/deep/c.txt"] {
            manager
                .insert_entry(entry(dir_relative(&sync_root, leaf), Some("h"), peer_id))
                .await
                .unwrap();
        }
        // Sibling that lives outside the removed directory and must survive.
        let outside: RelativePath = "Other Folder/keep.txt".into();
        manager
            .insert_entry(entry(outside.clone(), Some("h"), peer_id))
            .await
            .unwrap();

        let removed = manager.remove_dir(&sync_root).await.unwrap();
        assert_eq!(removed.len(), 3);
        for e in &removed {
            assert!(e.is_removed(), "{} should carry tombstone hash", e.name);
        }
        assert!(manager.get_entry(&outside).await.unwrap().is_some());
    }

    /// Issue #33 B2: on a true concurrent-edit conflict resolved by the
    /// `local_id < peer_id` tiebreak, the local row must NOT absorb the
    /// peer's axis. If we merged, our vector would dominate the peer's
    /// on the next exchange, and the peer would silently overwrite its
    /// own edit (no conflict file anywhere). Leaving our vector
    /// untouched lets the peer re-detect the conflict from its side
    /// and preserve its edit through the existing `KeepOther`
    /// conflict-file path.
    #[tokio::test]
    async fn conflict_keep_self_does_not_merge_peer_axis() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let local_id = manager.state.local_id();
        // Force peer_id > local_id so the deterministic tiebreak keeps self.
        let peer_id = Uuid::from_u128(u128::MAX);
        let name = dir_relative(&sync_root, "report.md");

        manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("local-edit".into()),
                version: HashMap::from([(local_id, 1)]),
            })
            .await
            .unwrap();

        let mut local = manager.get_entry(&name).await.unwrap().unwrap();
        let peer = EntryInfo {
            name: name.clone(),
            kind: EntryKind::File,
            hash: Some("peer-edit".into()),
            version: HashMap::from([(peer_id, 1)]),
        };

        let cmp = manager
            .compare_and_resolve_conflict(&mut local, &peer, peer_id)
            .await
            .unwrap();
        assert!(matches!(cmp, VersionCmp::KeepSelf));

        // The persisted local row must NOT have absorbed the peer axis,
        // otherwise the peer's next exchange would see our vector
        // dominate and accept our bytes without writing a conflict file.
        let stored = manager.get_entry(&name).await.unwrap().unwrap();
        assert!(
            !stored.version.contains_key(&peer_id),
            "peer axis must not be merged on conflict-resolved-as-KeepSelf"
        );
        assert_eq!(stored.version.get(&local_id), Some(&1));
        assert_eq!(stored.hash.as_deref(), Some("local-edit"));
    }

    /// Equal-vs-equal (no real conflict) still merges the peer's axis —
    /// that's the safe convergence path the no-merge rule above does
    /// NOT regress.
    #[tokio::test]
    async fn equal_compare_still_merges_peer_axis() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let local_id = manager.state.local_id();
        let peer_id = Uuid::new_v4();
        let name = dir_relative(&sync_root, "shared.txt");

        manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("same".into()),
                version: HashMap::from([(local_id, 1)]),
            })
            .await
            .unwrap();

        let mut local = manager.get_entry(&name).await.unwrap().unwrap();
        let peer = EntryInfo {
            name: name.clone(),
            kind: EntryKind::File,
            hash: Some("same".into()),
            version: HashMap::from([(peer_id, 4)]),
        };

        let cmp = manager
            .compare_and_resolve_conflict(&mut local, &peer, peer_id)
            .await
            .unwrap();
        assert!(matches!(cmp, VersionCmp::Equal));

        let stored = manager.get_entry(&name).await.unwrap().unwrap();
        assert_eq!(stored.version.get(&peer_id), Some(&4));
    }

    /// Issue #33 B3: a deletion must persist as a durable tombstone, not
    /// vanish from the DB. Without this, a crash between the row-delete
    /// and the metadata broadcast — or any late-joining peer — would
    /// silently re-sync the file back from a peer that still has the
    /// live copy.
    #[tokio::test]
    async fn remove_entry_persists_tombstone_in_db() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let local_id = manager.state.local_id();
        let name = dir_relative(&sync_root, "doc.txt");

        manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("live".into()),
                version: HashMap::from([(local_id, 1)]),
            })
            .await
            .unwrap();

        let removed = manager.remove_entry(&name).await.unwrap().unwrap();
        assert!(removed.is_removed());
        assert_eq!(removed.version.get(&local_id), Some(&2));

        let stored = manager
            .get_entry(&name)
            .await
            .unwrap()
            .expect("tombstone must be persisted in DB");
        assert!(stored.is_removed());
        assert_eq!(stored.version.get(&local_id), Some(&2));
    }

    /// Issue #33 B3: build_db must preserve tombstones whose files are
    /// (correctly) missing on disk. The pre-fix code deleted any row
    /// whose file was missing, which erased every tombstone on every
    /// restart and let deleted files resurrect from peers.
    #[tokio::test]
    async fn build_db_preserves_tombstones_when_file_missing() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let local_id = manager.state.local_id();
        let name = dir_relative(&sync_root, "gone.txt");

        let mut tombstone = EntryInfo {
            name: name.clone(),
            kind: EntryKind::File,
            hash: None,
            version: HashMap::from([(local_id, 3)]),
        };
        tombstone.set_removed_hash();
        manager.insert_entry(tombstone.clone()).await.unwrap();

        manager.build_db(HashMap::new()).await.unwrap();

        let stored = manager
            .get_entry(&name)
            .await
            .unwrap()
            .expect("tombstone must survive build_db with no fs entry");
        assert!(stored.is_removed());
    }

    /// Issue #33 B3: if a previously-tombstoned file is restored on disk
    /// (e.g. user pastes it back) before the next startup scan, the
    /// existing hash-mismatch branch replaces the tombstone with the
    /// live entry and bumps the local counter so the resurrection
    /// dominates the tombstone for any peer that still holds it.
    #[tokio::test]
    async fn build_db_resurrects_file_over_tombstone_when_present() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let local_id = manager.state.local_id();
        let name = dir_relative(&sync_root, "restored.txt");

        let mut tombstone = EntryInfo {
            name: name.clone(),
            kind: EntryKind::File,
            hash: None,
            version: HashMap::from([(local_id, 1)]),
        };
        tombstone.set_removed_hash();
        manager.insert_entry(tombstone).await.unwrap();

        let live = EntryInfo {
            name: name.clone(),
            kind: EntryKind::File,
            hash: Some("live-again".into()),
            version: HashMap::from([(local_id, 0)]),
        };
        manager
            .build_db(HashMap::from([(name.clone(), live)]))
            .await
            .unwrap();

        let stored = manager.get_entry(&name).await.unwrap().unwrap();
        assert_eq!(stored.hash.as_deref(), Some("live-again"));
        assert!(!stored.is_removed());
        assert_eq!(stored.version.get(&local_id), Some(&2));
    }

    /// Issue #33 B3: tombstones must reach peers via the handshake entry
    /// list so a late-joining or recently-online peer learns the delete
    /// and removes its own live copy.
    #[tokio::test]
    async fn get_handshake_data_includes_tombstones() {
        let (_env, _temp_dir, sync_dir, manager) = setup().await;
        let sync_root = add_sync_dir(&manager, &sync_dir).await;
        let local_id = manager.state.local_id();
        let name = dir_relative(&sync_root, "deleted.txt");

        manager
            .insert_entry(EntryInfo {
                name: name.clone(),
                kind: EntryKind::File,
                hash: Some("was-live".into()),
                version: HashMap::from([(local_id, 1)]),
            })
            .await
            .unwrap();
        manager.remove_entry(&name).await.unwrap();

        let data = manager.get_handshake_data().await.unwrap();
        let advertised = data
            .entries
            .get(&name)
            .expect("tombstone must be advertised");
        assert!(advertised.is_removed());
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
