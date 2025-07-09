use crate::{
    entry::EntryService,
    models::{
        device::Device,
        entry::{ConfigEntry, Entry},
    },
    utils::fs::{get_file_data, get_last_modified_date, get_relative_path},
};
use std::{
    collections::HashMap,
    fs::{self},
    net::IpAddr,
    sync::RwLock,
};
use std::{
    io,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub struct AppState {
    pub entry_service: EntryService,
    pub devices: RwLock<HashMap<IpAddr, Device>>,
    pub constants: AppConstants,
}

pub struct AppConstants {
    pub files_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub tcp_port: u16,
    pub broadcast_port: u16,
    pub broadcast_interval_secs: u64,
    pub device_timeout_secs: u64,
}

pub fn init() -> AppState {
    let cfg_path = ".cfg.json";
    let files_dir = "synche-files";

    let files = load_config_file(cfg_path);
    let (files_dir, tmp_dir) = create_dirs(files_dir);

    tracing_subscriber::fmt::init();

    let synched_entries = build_synched_files(files, &files_dir).unwrap();

    AppState {
        entry_service: EntryService::new(synched_entries),
        devices: RwLock::new(HashMap::<IpAddr, Device>::new()),
        constants: AppConstants {
            files_dir: files_dir.to_owned(),
            tmp_dir: tmp_dir.to_owned(),
            tcp_port: 8889,
            broadcast_port: 8888,
            broadcast_interval_secs: 5,
            device_timeout_secs: 15,
        },
    }
}

fn load_config_file(path: &str) -> Vec<ConfigEntry> {
    let contents = fs::read_to_string(path).expect("Failed to read config file");
    serde_json::from_str(&contents).expect("Failed to parse config file")
}

fn create_dirs(files_dir: &str) -> (PathBuf, PathBuf) {
    let tmp_dir = std::env::temp_dir().join(files_dir);
    let files_dir = PathBuf::new().join(files_dir);

    fs::create_dir_all(&tmp_dir).unwrap();
    fs::create_dir_all(&files_dir).unwrap();

    (files_dir, tmp_dir)
}

fn build_synched_files(files: Vec<ConfigEntry>, dir: &Path) -> io::Result<HashMap<String, Entry>> {
    let mut result = HashMap::new();

    let abs_base_path = dir.canonicalize()?;

    for file in files {
        let path = dir.join(&file.name);

        if !path.exists() {
            result.insert(file.name.clone(), Entry::absent(&file.name, path.is_dir()));
            continue;
        }

        let relative_path = get_relative_path(&path, &abs_base_path)?;

        if path.is_file() {
            build_file(&path, relative_path, &mut result)?;
        } else if path.is_dir() {
            build_dir(&path, &abs_base_path, &mut result)?;
        }
    }

    Ok(result)
}

fn build_file(
    path: &PathBuf,
    relative_path: String,
    result: &mut HashMap<String, Entry>,
) -> io::Result<()> {
    let (hash, last_modified_at) = get_file_data(path)?;
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
        let relative_path = get_relative_path(path, abs_base_path)?;

        if path == dir_path {
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
            build_file(&path.to_path_buf(), relative_path, result)?;
        } else if path.is_dir() {
            build_dir(&path.to_path_buf(), abs_base_path, result)?;
        }
    }

    Ok(())
}
