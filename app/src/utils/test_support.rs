//! Per-test filesystem and port isolation.
//!
//! Tests must never touch the real OS data/config dirs (e.g.
//! `~/.config/synche/config.toml`, `~/.local/share/synche/device_id`) or the
//! real `~/Synche/` home directory — when `cargo test` runs them in parallel
//! they race on those shared files and flake.
//!
//! [`test_env`] gives each test its own `TempDir`-rooted [`SyncheDirs`], a
//! seeded `config.toml`, and an `AppState` bound to ephemeral ports (`0`).
use crate::{
    application::AppState,
    domain::{AppPorts, CanonicalPath, Config, ConfigDirectory},
    utils::dirs::SyncheDirs,
};
use std::sync::Arc;
use tempfile::TempDir;

pub struct TestEnv {
    /// Holds the temp dir alive for the lifetime of the test. Dropping
    /// `TestEnv` removes everything created under it. Field is read by
    /// the `Drop` impl on `TempDir`; the linter doesn't see that.
    #[allow(dead_code)]
    pub temp: TempDir,
    pub dirs: SyncheDirs,
    pub home: CanonicalPath,
    pub state: Arc<AppState>,
}

impl TestEnv {
    /// Returns the home path the seeded `AppState` is using. Tests that need
    /// to create entries / sync dirs should use this rather than
    /// `state.home_path()` (which is identical, but this is clearer).
    pub fn home_path(&self) -> &CanonicalPath {
        &self.home
    }
}

/// Build an isolated [`AppState`] for tests.
///
/// Layout under the per-test temp dir:
/// - `data/`   — data directory (device_id, data.db)
/// - `config/` — config directory (config.toml)
/// - `home/`   — Synche home directory
///
/// `config.toml` is seeded so `Config::init` reads it instead of resolving
/// the real `default_home_dir()` (`~/Synche`).
pub async fn test_env() -> TestEnv {
    test_env_with_dirs(&["Default Folder"]).await
}

/// Like [`test_env`] but seeds the config with the given sync directory names.
pub async fn test_env_with_dirs(dirs: &[&str]) -> TestEnv {
    let temp = TempDir::new().expect("create test temp dir");

    let home_path = temp.path().join("home");
    std::fs::create_dir_all(&home_path).expect("create test home dir");
    let home = CanonicalPath::new(&home_path).expect("canonicalize test home dir");

    let dirs_struct = SyncheDirs::rooted_at(temp.path()).expect("init SyncheDirs");

    let seeded = Config {
        home_path: home.clone(),
        directory: dirs.iter().map(|name| ConfigDirectory::new(name)).collect(),
    };
    let contents = toml::to_string_pretty(&seeded).expect("serialize seeded config");
    std::fs::write(dirs_struct.config_file(), contents).expect("write seeded config");

    let state = AppState::new(
        dirs_struct.clone(),
        AppPorts {
            http: 0,
            presence: 0,
            transport: 0,
        },
    )
    .await;

    TestEnv {
        temp,
        dirs: dirs_struct,
        home,
        state,
    }
}
