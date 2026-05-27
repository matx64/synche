use crate::{
    domain::{
        AppPorts, BroadcastChannel, CanonicalPath, Config, ConfigDirectory, Peer, RelativePath,
        ServerEvent, SyncDirectory,
    },
    utils::dirs::SyncheDirs,
};
use std::{collections::HashMap, net::IpAddr, path::PathBuf, sync::Arc};
use tokio::{
    fs, io,
    sync::{RwLock, broadcast},
};
use uuid::Uuid;

pub const DEFAULT_HTTP_PORT: u16 = 42880;
pub const DEFAULT_PRESENCE_PORT: u16 = 42881;
pub const DEFAULT_TRANSPORT_PORT: u16 = 42882;

/// Returns the production port assignment. Tests inject their own
/// `AppPorts { http: 0, ... }` to avoid collisions with a running
/// instance and with each other.
pub fn default_ports() -> AppPorts {
    AppPorts {
        http: DEFAULT_HTTP_PORT,
        presence: DEFAULT_PRESENCE_PORT,
        transport: DEFAULT_TRANSPORT_PORT,
    }
}

/// Process-wide runtime hub shared as `Arc<AppState>` across every
/// subsystem.
///
/// Holds the device's identities (`local_id` persists across
/// restarts; `instance_id` is regenerated per process), the active
/// `home_path` and port assignments, the live peer and sync-dir maps,
/// and the SSE broadcast channel used to push events to the GUI.
///
/// All on-disk paths are resolved through the injected `SyncheDirs`
/// rather than global statics — see `CLAUDE.md` (Runtime / data
/// files) for why tests depend on per-test isolation here.
pub struct AppState {
    dirs: SyncheDirs,
    ports: AppPorts,
    local_id: Uuid,
    instance_id: Uuid,
    hostname: String,
    home_path: CanonicalPath,
    local_ip: RwLock<IpAddr>,
    sse_broadcast: BroadcastChannel<ServerEvent>,
    pub(super) peers: RwLock<HashMap<Uuid, Peer>>,
    pub(super) sync_dirs: RwLock<HashMap<RelativePath, SyncDirectory>>,
}

impl AppState {
    /// Build an `AppState` from explicit directories and ports.
    ///
    /// `main` builds `SyncheDirs::from_os()` once and threads it through
    /// `Synchronizer::run_default_with_restart`; tests construct their own
    /// `SyncheDirs` rooted in a per-test `TempDir` for isolation.
    pub async fn new(dirs: SyncheDirs, ports: AppPorts) -> Arc<Self> {
        let config = Config::init(&dirs).await.unwrap();

        let local_ip = local_ip_address::local_ip().unwrap();
        let (local_id, instance_id) = Self::init_ids(&dirs).await.unwrap();

        let hostname = hostname::get().unwrap().to_string_lossy().to_string();
        let hostname = hostname
            .strip_suffix(".local")
            .unwrap_or(&hostname)
            .to_string();

        let sync_dirs = RwLock::new(
            config
                .directory
                .iter()
                .map(|d| (d.name.clone(), d.to_sync()))
                .collect(),
        );

        Arc::new(Self {
            dirs,
            ports,
            hostname,
            local_id,
            instance_id,
            peers: Default::default(),
            sse_broadcast: BroadcastChannel::new(100),
            home_path: config.home_path,
            local_ip: RwLock::new(local_ip),
            sync_dirs,
        })
    }

    pub fn dirs(&self) -> &SyncheDirs {
        &self.dirs
    }

    pub fn ports(&self) -> &AppPorts {
        &self.ports
    }

    pub fn local_id(&self) -> Uuid {
        self.local_id
    }

    pub fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    pub fn hostname(&self) -> &String {
        &self.hostname
    }

    pub fn home_path(&self) -> &CanonicalPath {
        &self.home_path
    }

    pub async fn local_ip(&self) -> IpAddr {
        *self.local_ip.read().await
    }

    pub fn sse_sender(&self) -> broadcast::Sender<ServerEvent> {
        self.sse_broadcast.sender()
    }

    pub fn sse_subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.sse_broadcast.subscribe()
    }

    async fn init_ids(dirs: &SyncheDirs) -> io::Result<(Uuid, Uuid)> {
        let file = dirs.device_id_file();

        let local_id = if !file.exists() {
            let id = Uuid::new_v4();
            fs::write(&file, id.to_string()).await?;
            id
        } else {
            let id = fs::read_to_string(&file).await?;
            Uuid::parse_str(&id).map_err(io::Error::other)?
        };

        Ok((local_id, Uuid::new_v4()))
    }

    /// Adds `name` to `config.toml` and the in-memory `sync_dirs`
    /// map. Returns `Ok(false)` if the directory was already present
    /// (idempotent, no rewrite).
    pub async fn add_dir_to_config(&self, name: &RelativePath) -> io::Result<bool> {
        let mut directory: Vec<ConfigDirectory> = {
            let dirs = self.sync_dirs.read().await;

            if dirs.contains_key(name) {
                return Ok(false);
            }

            dirs.values().map(|d| d.to_config()).collect()
        };

        directory.push(ConfigDirectory {
            name: name.to_owned(),
        });

        self.write_config(&Config {
            directory,
            home_path: self.home_path.clone(),
        })
        .await
        .map(|_| true)
    }

    /// Removes `name` from `config.toml`. No-op if the directory was
    /// not configured.
    pub async fn remove_dir_from_config(&self, name: &RelativePath) -> io::Result<()> {
        let directory: Vec<ConfigDirectory> = {
            let dirs = self.sync_dirs.read().await;

            if !dirs.contains_key(name) {
                return Ok(());
            }

            dirs.iter()
                .filter(|(path, _)| *path != name)
                .map(|(_, d)| d.to_config())
                .collect()
        };

        self.write_config(&Config {
            directory,
            home_path: self.home_path.clone(),
        })
        .await
    }

    /// Validates `new_path` and rewrites `config.toml` with it. The
    /// running synchronizer observes the change through its config
    /// watcher and triggers the `HOME_PATH_CHANGED:` restart loop in
    /// `Synchronizer::run_default_with_restart`.
    pub async fn set_home_path_in_config(&self, new_path: String) -> io::Result<()> {
        let new_home_path = self.validate_home_path(&new_path).await?;

        let directory: Vec<ConfigDirectory> = {
            self.sync_dirs
                .read()
                .await
                .values()
                .map(|d| d.to_config())
                .collect()
        };

        self.write_config(&Config {
            directory,
            home_path: new_home_path,
        })
        .await
    }

    /// Canonicalizes `path_str`, creating the directory (and parents)
    /// if it does not exist. Errors if the path exists but is not a
    /// directory.
    pub async fn validate_home_path(&self, path_str: &str) -> io::Result<CanonicalPath> {
        let path_buf = PathBuf::from(path_str);

        if !path_buf.exists() {
            fs::create_dir_all(&path_buf).await?;
        } else if !path_buf.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Path exists but is not a directory",
            ));
        }

        CanonicalPath::new(&path_buf)
    }

    async fn write_config(&self, config: &Config) -> io::Result<()> {
        let contents = toml::to_string_pretty(config).map_err(io::Error::other)?;
        fs::write(self.dirs.config_file(), contents).await
    }

    /// Returns `true` if `name` is an exact match for a configured
    /// sync directory.
    pub async fn contains_sync_dir(&self, name: &RelativePath) -> bool {
        self.sync_dirs.read().await.contains_key(name)
    }

    /// Returns `true` if `path` falls under any configured sync
    /// directory — the boundary check that decides whether a watcher
    /// event is relevant.
    ///
    /// Uses component-aware matching via `RelativePath::starts_with_dir`,
    /// so a configured dir `foo` does **not** match a sibling path
    /// `foobar/file.txt`.
    pub async fn is_under_sync_dir(&self, path: &RelativePath) -> bool {
        let dirs = self.sync_dirs.read().await;
        dirs.keys().any(|d| path.starts_with_dir(d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::test_env;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_state_getters() {
        let env = test_env().await;
        let state = &env.state;

        let _ports = state.ports();
        let _local_id = state.local_id();
        let _instance_id = state.instance_id();
        let _hostname = state.hostname();
        let _home_path = state.home_path();
        let _local_ip = state.local_ip().await;

        let instance1 = state.instance_id();
        let env2 = test_env().await;
        let instance2 = env2.state.instance_id();

        assert_ne!(
            instance1, instance2,
            "Instance IDs should be unique per AppState instance"
        );
    }

    #[tokio::test]
    async fn test_local_id_persistence() {
        // Reuse the same SyncheDirs so the on-disk device_id file is shared
        // between two AppState constructions — mirrors a process restart.
        let env = test_env().await;
        let local_id1 = env.state.local_id();

        let state2 = AppState::new(env.dirs.clone(), default_ports()).await;
        let local_id2 = state2.local_id();

        assert_eq!(
            local_id1, local_id2,
            "Local ID should persist across AppState instances"
        );
    }

    #[tokio::test]
    async fn test_validate_home_path_creates_missing_dir() {
        let temp = TempDir::new().unwrap();
        let new_dir = temp.path().join("new_directory");
        let env = test_env().await;

        assert!(!new_dir.exists(), "Directory should not exist initially");

        let result = env
            .state
            .validate_home_path(new_dir.to_str().unwrap())
            .await;
        assert!(result.is_ok(), "Should create missing directory");

        assert!(new_dir.exists(), "Directory should have been created");
        assert!(new_dir.is_dir(), "Created path should be a directory");
    }

    #[tokio::test]
    async fn test_validate_home_path_creates_nested_dirs() {
        let temp = TempDir::new().unwrap();
        let nested_dir = temp.path().join("level1").join("level2").join("level3");
        let env = test_env().await;

        assert!(!nested_dir.exists());

        let result = env
            .state
            .validate_home_path(nested_dir.to_str().unwrap())
            .await;
        assert!(result.is_ok(), "Should create nested directories");

        assert!(nested_dir.exists());
        assert!(nested_dir.is_dir());
    }

    #[tokio::test]
    async fn test_validate_home_path_rejects_file() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("file.txt");
        tokio::fs::write(&file_path, "test content").await.unwrap();
        let env = test_env().await;

        assert!(file_path.exists());
        assert!(file_path.is_file());

        let result = env
            .state
            .validate_home_path(file_path.to_str().unwrap())
            .await;
        assert!(result.is_err(), "Should reject file path");

        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("not a directory"));
    }

    #[tokio::test]
    async fn test_validate_home_path_accepts_unique_directory() {
        // Previously this test wrote to `./test_relative` in the workspace
        // CWD, which is process-global and races between concurrent tests.
        // Test the same code path (`fs::create_dir_all` + `canonicalize`)
        // with a guaranteed-unique absolute path instead.
        let temp = TempDir::new().unwrap();
        let unique = temp.path().join(format!("test_path_{}", Uuid::new_v4()));
        let env = test_env().await;

        let result = env.state.validate_home_path(unique.to_str().unwrap()).await;
        assert!(result.is_ok(), "Should handle freshly-created paths");
        assert!(unique.exists());
    }

    #[tokio::test]
    async fn test_validate_home_path_with_spaces() {
        let temp = TempDir::new().unwrap();
        let dir_with_spaces = temp.path().join("my sync folder");
        let env = test_env().await;

        let result = env
            .state
            .validate_home_path(dir_with_spaces.to_str().unwrap())
            .await;
        assert!(result.is_ok(), "Should handle paths with spaces");

        assert!(dir_with_spaces.exists());
    }

    #[tokio::test]
    async fn test_validate_home_path_valid_existing_dir() {
        let temp = TempDir::new().unwrap();
        let env = test_env().await;

        let result = env
            .state
            .validate_home_path(temp.path().to_str().unwrap())
            .await;
        assert!(result.is_ok(), "Should accept existing directory");

        let canonical = result.unwrap();
        assert!(canonical.exists());
        assert!(canonical.is_dir());
    }

    #[tokio::test]
    async fn test_contains_sync_dir_existing() {
        let env = test_env().await;

        let dirs: Vec<RelativePath> = env.state.sync_dirs.read().await.keys().cloned().collect();

        if let Some(dir) = dirs.first() {
            assert!(
                env.state.contains_sync_dir(dir).await,
                "Should find existing sync directory"
            );
        }
    }

    #[tokio::test]
    async fn test_is_under_sync_dir_direct_child() {
        let env = test_env().await;

        let dirs: Vec<RelativePath> = env.state.sync_dirs.read().await.keys().cloned().collect();

        if let Some(dir) = dirs.first() {
            let child_path = RelativePath::from(format!(
                "{}/subdir/file.txt",
                <RelativePath as AsRef<str>>::as_ref(dir)
            ));
            assert!(
                env.state.is_under_sync_dir(&child_path).await,
                "File under sync dir should be detected"
            );
        }
    }

    #[tokio::test]
    async fn test_is_under_sync_dir_exact_match() {
        let env = test_env().await;

        let dirs: Vec<RelativePath> = env.state.sync_dirs.read().await.keys().cloned().collect();

        if let Some(dir) = dirs.first() {
            assert!(
                env.state.is_under_sync_dir(dir).await,
                "Sync dir itself should match is_under_sync_dir"
            );
        }
    }

    /// Verifies the component-aware boundary check (issue #32 finding
    /// #4). A configured sync dir `foo` must NOT match a sibling path
    /// like `foobar/file.txt`.
    #[tokio::test]
    async fn is_under_sync_dir_does_not_match_string_prefix_siblings() {
        let env = crate::utils::test_support::test_env_with_dirs(&["foo"]).await;

        assert!(
            env.state.is_under_sync_dir(&"foo/file.txt".into()).await,
            "child path under foo/ should match"
        );
        assert!(
            !env.state.is_under_sync_dir(&"foobar/file.txt".into()).await,
            "string-prefix sibling foobar/ must NOT match foo"
        );
    }

    #[tokio::test]
    async fn test_add_dir_to_config_duplicate_prevention() {
        let env = test_env().await;

        let dirs: Vec<RelativePath> = env.state.sync_dirs.read().await.keys().cloned().collect();

        if let Some(existing_dir) = dirs.first() {
            let result = env.state.add_dir_to_config(existing_dir).await;

            assert!(result.is_ok(), "Should not error on duplicate");
            assert!(
                !result.unwrap(),
                "Should return false for duplicate directory"
            );
        }
    }

    #[tokio::test]
    async fn test_remove_dir_from_config_non_existent() {
        let env = test_env().await;
        let non_existent = RelativePath::from(format!("non_existent_dir_{}", Uuid::new_v4()));

        let result = env.state.remove_dir_from_config(&non_existent).await;
        assert!(
            result.is_ok(),
            "Removing non-existent directory should be OK (idempotent)"
        );
    }

    #[tokio::test]
    async fn test_sse_broadcast_channels() {
        let env = test_env().await;
        let state = &env.state;

        let sender = state.sse_sender();
        let mut receiver1 = state.sse_subscribe();
        let mut receiver2 = state.sse_subscribe();

        let test_event = ServerEvent::ServerRestart;
        sender.send(test_event.clone()).ok();

        let recv1 =
            tokio::time::timeout(std::time::Duration::from_millis(100), receiver1.recv()).await;
        let recv2 =
            tokio::time::timeout(std::time::Duration::from_millis(100), receiver2.recv()).await;

        assert!(recv1.is_ok(), "First receiver should get event");
        assert!(recv2.is_ok(), "Second receiver should get event");
    }
}
