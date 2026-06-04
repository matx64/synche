use crate::{
    domain::{CanonicalPath, ConfigDirectory, PortOverrides},
    utils::{dirs::SyncheDirs, fs::default_home_dir},
};
use serde::{Deserialize, Serialize};
use tokio::{fs, io};

/// On-disk representation of `config.toml`.
///
/// Holds the user's chosen `home_path` and the list of sync
/// directories. Edits to this file are observed by the config watcher
/// and applied live; changing `home_path` triggers the synchronizer's
/// restart loop (see `Synchronizer::run_default_with_restart`).
#[derive(Serialize, Deserialize)]
pub struct Config {
    pub home_path: CanonicalPath,
    /// Optional `[ports]` block. Missing fields fall back to the CLI flags and
    /// then the default constants (see `app_state::resolve_ports`). Omitted from
    /// the serialized file when empty so the default `config.toml` stays minimal.
    #[serde(default, skip_serializing_if = "PortOverrides::is_empty")]
    pub ports: PortOverrides,
    pub directory: Vec<ConfigDirectory>,
}

impl Config {
    /// Load `config.toml` from `dirs.config_file()`, writing a default
    /// file if none exists. The resolved `home_path` directory is created
    /// if missing.
    pub async fn init(dirs: &SyncheDirs) -> io::Result<Self> {
        let path = dirs.config_file();

        if path.exists() {
            let contents = fs::read_to_string(&path).await?;
            let cfg: Self = toml::from_str(&contents).map_err(io::Error::other)?;

            if !cfg.home_path.exists() {
                fs::create_dir_all(&cfg.home_path).await?;
            }

            Ok(cfg)
        } else {
            let cfg = Self::default();

            let contents = toml::to_string_pretty(&cfg).map_err(io::Error::other)?;

            fs::write(&path, contents).await?;
            Ok(cfg)
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            home_path: default_home_dir().unwrap(),
            ports: PortOverrides::default(),
            directory: vec![ConfigDirectory::new("Default Folder")],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME_LINE: &str = "home_path = \"/tmp/synche-home\"\n";
    const DIR_LINES: &str = "[[directory]]\nname = \"Default Folder\"\n";

    #[test]
    fn deserializes_full_ports_block() {
        let toml = format!(
            "{HOME_LINE}[ports]\nhttp = 1111\npresence = 2222\ntransport = 3333\n{DIR_LINES}"
        );
        let cfg: Config = toml::from_str(&toml).unwrap();
        assert_eq!(cfg.ports.http, Some(1111));
        assert_eq!(cfg.ports.presence, Some(2222));
        assert_eq!(cfg.ports.transport, Some(3333));
    }

    #[test]
    fn deserializes_partial_ports_block_leaves_rest_unset() {
        let toml = format!("{HOME_LINE}[ports]\nhttp = 9999\n{DIR_LINES}");
        let cfg: Config = toml::from_str(&toml).unwrap();
        assert_eq!(cfg.ports.http, Some(9999));
        assert_eq!(cfg.ports.presence, None);
        assert_eq!(cfg.ports.transport, None);
    }

    #[test]
    fn deserializes_without_ports_block_as_empty() {
        let toml = format!("{HOME_LINE}{DIR_LINES}");
        let cfg: Config = toml::from_str(&toml).unwrap();
        assert!(cfg.ports.is_empty());
    }

    #[test]
    fn serialization_omits_empty_ports_table() {
        let cfg: Config = toml::from_str(&format!("{HOME_LINE}{DIR_LINES}")).unwrap();
        let out = toml::to_string_pretty(&cfg).unwrap();
        assert!(
            !out.contains("[ports]"),
            "empty [ports] must be omitted: {out}"
        );
    }

    #[test]
    fn round_trips_a_set_ports_block() {
        let cfg: Config = toml::from_str(&format!(
            "{HOME_LINE}[ports]\ntransport = 4444\n{DIR_LINES}"
        ))
        .unwrap();
        let out = toml::to_string_pretty(&cfg).unwrap();
        assert!(out.contains("[ports]"));
        assert!(out.contains("transport = 4444"));

        let reparsed: Config = toml::from_str(&out).unwrap();
        assert_eq!(reparsed.ports.transport, Some(4444));
        assert_eq!(reparsed.ports.http, None);
    }
}
