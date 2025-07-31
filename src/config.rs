use crate::{
    domain::{ConfiguredDirectory, Directory, EntryInfo, entry::entry::EntryKind},
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
use uuid::Uuid;
use walkdir::WalkDir;

pub struct Config {
    pub directories: HashMap<String, Directory>,
    pub filesystem_entries: HashMap<String, EntryInfo>,
    pub constants: AppConstants,
}

pub struct AppConstants {
    pub local_id: Uuid,
    pub base_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub broadcast_interval_secs: u64,
}

pub fn init() -> Config {
    let cfg_path = ".synche";
    let base_dir = "synche-files";
    let tmp_dir = ".tmp";

    let (local_id, configured_dirs) = load_config_file(cfg_path);
    let (base_dir, tmp_dir) = create_dirs(base_dir, tmp_dir);

    tracing_subscriber::fmt::init();

    let (dirs, entries) = build_entries(local_id, configured_dirs, &base_dir).unwrap();

    Config {
        directories: dirs,
        filesystem_entries: entries,
        constants: AppConstants {
            local_id,
            base_dir: base_dir.to_owned(),
            tmp_dir: tmp_dir.to_owned(),
            broadcast_interval_secs: 5,
        },
    }
}

fn load_config_file(cfg_base: &str) -> (Uuid, Vec<ConfiguredDirectory>) {
    let settings_path = PathBuf::from(cfg_base).join("settings.json");
    let settings_json = fs::read_to_string(settings_path).expect("Failed to read config file");
    let settings_dirs = serde_json::from_str(&settings_json).expect("Failed to parse config file");

    let id_path = PathBuf::from(cfg_base).join("device.id");
    let local_id = match fs::read_to_string(&id_path) {
        Ok(id) => Uuid::parse_str(&id).unwrap(),
        Err(_) => {
            let id = Uuid::new_v4();
            fs::write(id_path, id.to_string()).expect("Failed to write device.id file");
            id
        }
    };

    (local_id, settings_dirs)
}

fn create_dirs(base_dir: &str, tmp_dir: &str) -> (PathBuf, PathBuf) {
    let tmp_dir = PathBuf::new().join(tmp_dir);
    let base_dir = PathBuf::new().join(base_dir);

    fs::create_dir_all(&tmp_dir).unwrap();
    fs::create_dir_all(&base_dir).unwrap();

    (base_dir, tmp_dir)
}

fn build_entries(
    local_id: Uuid,
    configured_dirs: Vec<ConfiguredDirectory>,
    base_dir: &Path,
) -> io::Result<(HashMap<String, Directory>, HashMap<String, EntryInfo>)> {
    let mut dirs = HashMap::new();
    let mut entries = HashMap::new();

    let abs_base_path = base_dir.canonicalize()?;

    for dir in configured_dirs {
        let path = base_dir.join(&dir.name);

        fs::create_dir_all(&path).unwrap();

        if path.is_dir() {
            dirs.insert(dir.name.clone(), Directory { name: dir.name });
            build_dir(local_id, &path, &abs_base_path, &mut entries)?;
        }
    }

    Ok((dirs, entries))
}

fn build_dir(
    local_id: Uuid,
    dir_path: &PathBuf,
    abs_base_path: &PathBuf,
    entries: &mut HashMap<String, EntryInfo>,
) -> io::Result<()> {
    for entry in WalkDir::new(dir_path).into_iter().filter_map(Result::ok) {
        let path = entry.path();

        if path == dir_path {
            continue;
        }

        if path.is_file() {
            build_file(local_id, &path.to_path_buf(), abs_base_path, entries)?;
        } else if path.is_dir() {
            let relative_path = get_relative_path(&path.canonicalize()?, abs_base_path)?;
            entries.insert(
                relative_path.clone(),
                EntryInfo {
                    name: relative_path,
                    kind: EntryKind::Directory,
                    hash: None,
                    is_deleted: false,
                    vv: HashMap::from([(local_id, 0)]),
                },
            );
            build_dir(local_id, &path.to_path_buf(), abs_base_path, entries)?;
        }
    }

    Ok(())
}

fn build_file(
    local_id: Uuid,
    path: &PathBuf,
    abs_base_path: &PathBuf,
    entries: &mut HashMap<String, EntryInfo>,
) -> io::Result<()> {
    let hash = compute_hash(path)?;
    let relative_path = get_relative_path(&path.canonicalize()?, abs_base_path)?;

    entries.insert(
        relative_path.clone(),
        EntryInfo {
            name: relative_path,
            kind: EntryKind::File,
            hash: Some(hash),
            is_deleted: false,
            vv: HashMap::from([(local_id, 0)]),
        },
    );
    Ok(())
}
