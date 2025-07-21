use crate::{
    domain::{ConfiguredDirectory, Directory, EntryManager, FileInfo, PeerManager},
    utils::fs::{compute_hash, get_relative_path},
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
    pub base_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub broadcast_interval_secs: u64,
}

pub fn init() -> AppState {
    let cfg_path = ".cfg.json";
    let base_dir = "synche-files";

    let configured_dirs = load_config_file(cfg_path);
    let (base_dir, tmp_dir) = create_dirs(base_dir);

    tracing_subscriber::fmt::init();

    let (dirs, files) = build_entries(configured_dirs, &base_dir).unwrap();

    AppState {
        entry_manager: EntryManager::new(dirs, files),
        peer_manager: PeerManager::new(),
        constants: AppConstants {
            base_dir: base_dir.to_owned(),
            tmp_dir: tmp_dir.to_owned(),
            broadcast_interval_secs: 5,
        },
    }
}

fn load_config_file(path: &str) -> Vec<ConfiguredDirectory> {
    let contents = fs::read_to_string(path).expect("Failed to read config file");
    serde_json::from_str(&contents).expect("Failed to parse config file")
}

fn create_dirs(base_dir: &str) -> (PathBuf, PathBuf) {
    let tmp_dir = std::env::temp_dir().join(base_dir);
    let base_dir = PathBuf::new().join(base_dir);

    fs::create_dir_all(&tmp_dir).unwrap();
    fs::create_dir_all(&base_dir).unwrap();

    (base_dir, tmp_dir)
}

fn build_entries(
    directories: Vec<ConfiguredDirectory>,
    base_dir: &Path,
) -> io::Result<(HashMap<String, Directory>, HashMap<String, FileInfo>)> {
    let mut dirs = HashMap::new();
    let mut files = HashMap::new();

    let abs_base_path = base_dir.canonicalize()?;

    for dir in directories {
        let path = base_dir.join(&dir.name);

        fs::create_dir_all(&path).unwrap();

        if path.is_dir() {
            dirs.insert(dir.name.clone(), Directory { name: dir.name });
            build_dir(&path, &abs_base_path, &mut files)?;
        }
    }

    Ok((dirs, files))
}

fn build_dir(
    dir_path: &PathBuf,
    abs_base_path: &PathBuf,
    files: &mut HashMap<String, FileInfo>,
) -> io::Result<()> {
    for entry in WalkDir::new(dir_path).into_iter().filter_map(Result::ok) {
        let path = entry.path();

        if path == dir_path {
            continue;
        }

        if path.is_file() {
            build_file(&path.to_path_buf(), abs_base_path, files)?;
        } else if path.is_dir() {
            build_dir(&path.to_path_buf(), abs_base_path, files)?;
        }
    }

    Ok(())
}

fn build_file(
    path: &PathBuf,
    abs_base_path: &PathBuf,
    files: &mut HashMap<String, FileInfo>,
) -> io::Result<()> {
    let hash = compute_hash(path)?;
    let relative_path = get_relative_path(&path.canonicalize()?, abs_base_path)?;

    files.insert(
        relative_path.clone(),
        FileInfo {
            name: relative_path,
            hash,
            version: 0,
            last_modified_by: None,
        },
    );
    Ok(())
}
