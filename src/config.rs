use crate::{
    entry::EntryManager,
    models::entry::{ConfigEntry, Entry},
    peer::PeerManager,
    utils::fs::{get_file_data, get_last_modified_date, get_relative_path},
};
use std::{
    collections::HashMap,
    fs::{self},
};
use std::{
    io,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub struct AppState {
    pub entry_manager: EntryManager,
    pub peer_manager: PeerManager,
    pub constants: AppConstants,
}

pub struct AppConstants {
    pub entries_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub tcp_port: u16,
    pub broadcast_port: u16,
    pub broadcast_interval_secs: u64,
}

pub fn init() -> AppState {
    let cfg_path = ".cfg.json";
    let entries_dir = "synche-files";

    let entries = load_config_file(cfg_path);
    let (entries_dir, tmp_dir) = create_dirs(entries_dir);

    tracing_subscriber::fmt::init();

    let entries = build_entries(entries, &entries_dir).unwrap();

    AppState {
        entry_manager: EntryManager::new(entries),
        peer_manager: PeerManager::new(),
        constants: AppConstants {
            entries_dir: entries_dir.to_owned(),
            tmp_dir: tmp_dir.to_owned(),
            tcp_port: 8889,
            broadcast_port: 8888,
            broadcast_interval_secs: 5,
        },
    }
}

fn load_config_file(path: &str) -> Vec<ConfigEntry> {
    let contents = fs::read_to_string(path).expect("Failed to read config file");
    serde_json::from_str(&contents).expect("Failed to parse config file")
}

fn create_dirs(entries_dir: &str) -> (PathBuf, PathBuf) {
    let tmp_dir = std::env::temp_dir().join(entries_dir);
    let entries_dir = PathBuf::new().join(entries_dir);

    fs::create_dir_all(&tmp_dir).unwrap();
    fs::create_dir_all(&entries_dir).unwrap();

    (entries_dir, tmp_dir)
}

fn build_entries(entries: Vec<ConfigEntry>, dir: &Path) -> io::Result<HashMap<String, Entry>> {
    let mut result = HashMap::new();

    let abs_base_path = dir.canonicalize()?;

    for entry in entries {
        let path = dir.join(&entry.name);

        if !path.exists() {
            result.insert(
                entry.name.clone(),
                Entry::absent(&entry.name, path.is_dir()),
            );
            continue;
        }

        if path.is_file() {
            build_file(&path, &abs_base_path, &mut result)?;
        } else if path.is_dir() {
            build_dir(&path, &abs_base_path, &mut result)?;
        }
    }

    Ok(result)
}

fn build_file(
    path: &PathBuf,
    abs_base_path: &PathBuf,
    result: &mut HashMap<String, Entry>,
) -> io::Result<()> {
    let (hash, last_modified_at) = get_file_data(path)?;
    let relative_path = get_relative_path(path, abs_base_path)?;

    result.insert(
        relative_path.clone(),
        Entry {
            name: relative_path,
            exists: true,
            is_dir: false,
            hash,
            last_modified_at,
        },
    );
    Ok(())
}

fn build_dir(
    dir_path: &PathBuf,
    abs_base_path: &PathBuf,
    result: &mut HashMap<String, Entry>,
) -> io::Result<()> {
    for entry in WalkDir::new(dir_path).into_iter().filter_map(Result::ok) {
        let path = entry.path();

        if path == dir_path {
            let relative_path = get_relative_path(path, abs_base_path)?;
            result.insert(
                relative_path.clone(),
                Entry {
                    name: relative_path,
                    exists: true,
                    is_dir: true,
                    hash: String::new(),
                    last_modified_at: get_last_modified_date(path)?,
                },
            );
            continue;
        }

        if path.is_file() {
            build_file(&path.to_path_buf(), abs_base_path, result)?;
        } else if path.is_dir() {
            build_dir(&path.to_path_buf(), abs_base_path, result)?;
        }
    }

    Ok(())
}
