use crate::{
    domain::{
        AppPorts, BroadcastChannel, CanonicalPath, Config, ConfigDirectory, Peer, RelativePath,
        ServerEvent, SyncDirectory,
    },
    utils::fs::{config_file, device_id_file},
};
use std::{collections::HashMap, net::IpAddr, path::PathBuf, sync::Arc};
use tokio::{
    fs, io,
    sync::{RwLock, broadcast},
};
use uuid::Uuid;

const HTTP_PORT: u16 = 42880;
const PRESENCE_PORT: u16 = 42881;
const TRANSPORT_PORT: u16 = 42882;

pub struct AppState {
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
    pub async fn new() -> Arc<Self> {
        let config = Config::init().await.unwrap();

        let local_ip = local_ip_address::local_ip().unwrap();
        let (local_id, instance_id) = Self::init_ids().await.unwrap();

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

        let ports = AppPorts {
            http: HTTP_PORT,
            presence: PRESENCE_PORT,
            transport: TRANSPORT_PORT,
        };

        Arc::new(Self {
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

    pub async fn init_ids() -> io::Result<(Uuid, Uuid)> {
        let file = device_id_file();

        let local_id = if !file.exists() {
            let id = Uuid::new_v4();
            fs::write(file, id.to_string()).await?;
            id
        } else {
            let id = fs::read_to_string(file).await?;
            Uuid::parse_str(&id).map_err(io::Error::other)?
        };

        Ok((local_id, Uuid::new_v4()))
    }

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
        fs::write(config_file(), contents).await
    }

    pub async fn contains_sync_dir(&self, name: &RelativePath) -> bool {
        self.sync_dirs.read().await.contains_key(name)
    }

    pub async fn is_under_sync_dir(&self, path: &RelativePath) -> bool {
        let dirs = self.sync_dirs.read().await;
        dirs.keys().any(|d| path.starts_with(&**d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_state_getters() {
        let state = AppState::new().await;

        let _ports = state.ports();
        let _local_id = state.local_id();
        let _instance_id = state.instance_id();
        let _hostname = state.hostname();
        let _home_path = state.home_path();
        let _local_ip = state.local_ip().await;

        let instance1 = state.instance_id();
        let state2 = AppState::new().await;
        let instance2 = state2.instance_id();

        assert_ne!(
            instance1, instance2,
            "Instance IDs should be unique per AppState instance"
        );
    }

    #[tokio::test]
    async fn test_local_id_persistence() {
        let state1 = AppState::new().await;
        let local_id1 = state1.local_id();

        let state2 = AppState::new().await;
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
        let state = AppState::new().await;

        assert!(!new_dir.exists(), "Directory should not exist initially");

        let result = state.validate_home_path(new_dir.to_str().unwrap()).await;
        assert!(result.is_ok(), "Should create missing directory");

        assert!(new_dir.exists(), "Directory should have been created");
        assert!(new_dir.is_dir(), "Created path should be a directory");
    }

    #[tokio::test]
    async fn test_validate_home_path_creates_nested_dirs() {
        let temp = TempDir::new().unwrap();
        let nested_dir = temp.path().join("level1").join("level2").join("level3");
        let state = AppState::new().await;

        assert!(!nested_dir.exists());

        let result = state.validate_home_path(nested_dir.to_str().unwrap()).await;
        assert!(result.is_ok(), "Should create nested directories");

        assert!(nested_dir.exists());
        assert!(nested_dir.is_dir());
    }

    #[tokio::test]
    async fn test_validate_home_path_rejects_file() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("file.txt");
        tokio::fs::write(&file_path, "test content").await.unwrap();
        let state = AppState::new().await;

        assert!(file_path.exists());
        assert!(file_path.is_file());

        let result = state.validate_home_path(file_path.to_str().unwrap()).await;
        assert!(result.is_err(), "Should reject file path");

        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("not a directory"));
    }

    #[tokio::test]
    async fn test_validate_home_path_relative_path() {
        let state = AppState::new().await;

        let result = state.validate_home_path("./test_relative").await;
        assert!(result.is_ok(), "Should handle relative paths");

        fs::remove_dir_all("./test_relative").await.unwrap();
    }

    #[tokio::test]
    async fn test_validate_home_path_with_spaces() {
        let temp = TempDir::new().unwrap();
        let dir_with_spaces = temp.path().join("my sync folder");
        let state = AppState::new().await;

        let result = state
            .validate_home_path(dir_with_spaces.to_str().unwrap())
            .await;
        assert!(result.is_ok(), "Should handle paths with spaces");

        assert!(dir_with_spaces.exists());
    }

    #[tokio::test]
    async fn test_validate_home_path_valid_existing_dir() {
        let temp = TempDir::new().unwrap();
        let state = AppState::new().await;

        let result = state
            .validate_home_path(temp.path().to_str().unwrap())
            .await;
        assert!(result.is_ok(), "Should accept existing directory");

        let canonical = result.unwrap();
        assert!(canonical.exists());
        assert!(canonical.is_dir());
    }

    #[tokio::test]
    async fn test_contains_sync_dir_existing() {
        let state = AppState::new().await;

        let dirs: Vec<RelativePath> = state.sync_dirs.read().await.keys().cloned().collect();

        if let Some(dir) = dirs.first() {
            assert!(
                state.contains_sync_dir(dir).await,
                "Should find existing sync directory"
            );
        }
    }

    #[tokio::test]
    async fn test_is_under_sync_dir_direct_child() {
        let state = AppState::new().await;

        let dirs: Vec<RelativePath> = state.sync_dirs.read().await.keys().cloned().collect();

        if let Some(dir) = dirs.first() {
            let child_path = RelativePath::from(format!(
                "{}/subdir/file.txt",
                <RelativePath as AsRef<str>>::as_ref(dir)
            ));
            assert!(
                state.is_under_sync_dir(&child_path).await,
                "File under sync dir should be detected"
            );
        }
    }

    #[tokio::test]
    async fn test_is_under_sync_dir_exact_match() {
        let state = AppState::new().await;

        let dirs: Vec<RelativePath> = state.sync_dirs.read().await.keys().cloned().collect();

        if let Some(dir) = dirs.first() {
            assert!(
                state.is_under_sync_dir(dir).await,
                "Sync dir itself should match is_under_sync_dir"
            );
        }
    }

    #[tokio::test]
    async fn test_add_dir_to_config_duplicate_prevention() {
        let state = AppState::new().await;

        let dirs: Vec<RelativePath> = state.sync_dirs.read().await.keys().cloned().collect();

        if let Some(existing_dir) = dirs.first() {
            let result = state.add_dir_to_config(existing_dir).await;

            assert!(result.is_ok(), "Should not error on duplicate");
            assert!(
                !result.unwrap(),
                "Should return false for duplicate directory"
            );
        }
    }

    #[tokio::test]
    async fn test_remove_dir_from_config_non_existent() {
        let state = AppState::new().await;
        let non_existent = RelativePath::from(format!("non_existent_dir_{}", Uuid::new_v4()));

        let result = state.remove_dir_from_config(&non_existent).await;
        assert!(
            result.is_ok(),
            "Removing non-existent directory should be OK (idempotent)"
        );
    }

    #[tokio::test]
    async fn test_sse_broadcast_channels() {
        let state = AppState::new().await;

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
