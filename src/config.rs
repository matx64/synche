use crate::{
    application::IgnoreHandler,
    domain::{
        CanonicalPath, ConfigFileDirectory, EntryInfo, EntryKind, RelativePath, SyncDirectory,
    },
    utils::fs::{compute_hash, is_ds_store},
};
use std::{
    collections::HashMap,
    fs::{self},
};
use std::{io, path::PathBuf};
use uuid::Uuid;
use walkdir::WalkDir;

const BASE_DIR: &str = "./synche-files";
const TMP_DIR: &str = "./.tmp";
const CFG_DIR: &str = "./.synche";
const CFG_FILE: &str = "config.json";
const DEVICE_ID_FILE: &str = "device.id";

pub struct Config {
    pub local_id: Uuid,
    pub sync_directories: HashMap<String, SyncDirectory>,
    pub filesystem_entries: HashMap<RelativePath, EntryInfo>,
    pub ignore_handler: IgnoreHandler,
    pub required_dirs: ConfigRequiredDirs,
}

pub struct ConfigRequiredDirs {
    pub base_dir_path: CanonicalPath,
    pub tmp_dir_path: CanonicalPath,
    pub cfg_dir_path: CanonicalPath,
}

pub fn init() -> Config {
    let required_dirs = create_required_dirs();
    let (local_id, configured_dirs) = load_config_file(&required_dirs.cfg_dir_path);

    tracing_subscriber::fmt::init();

    let mut ignore_handler = IgnoreHandler::new(required_dirs.base_dir_path.clone());

    let (sync_directories, filesystem_entries) = build_entries(
        local_id,
        configured_dirs,
        &required_dirs.base_dir_path,
        &mut ignore_handler,
    )
    .unwrap();

    Config {
        local_id,
        sync_directories,
        filesystem_entries,
        ignore_handler,
        required_dirs,
    }
}

fn create_required_dirs() -> ConfigRequiredDirs {
    let cfg_dir_path = CanonicalPath::new(CFG_DIR).unwrap();
    let tmp_dir_path = CanonicalPath::new(TMP_DIR).unwrap();
    let base_dir_path = CanonicalPath::new(BASE_DIR).unwrap();

    fs::create_dir_all(&cfg_dir_path).unwrap();
    fs::create_dir_all(&tmp_dir_path).unwrap();
    fs::create_dir_all(&base_dir_path).unwrap();

    ConfigRequiredDirs {
        base_dir_path,
        tmp_dir_path,
        cfg_dir_path,
    }
}

fn load_config_file(cfg_dir_path: &CanonicalPath) -> (Uuid, Vec<ConfigFileDirectory>) {
    let cfg_file_path = cfg_dir_path.join(CFG_FILE);

    if !cfg_file_path.exists() {
        fs::write(&cfg_file_path, "[{\"folder_name\": \"myfolder\"}]").unwrap();
    }

    let cfg_json = fs::read_to_string(cfg_file_path).expect("Failed to read config file");
    let cfg_dirs = serde_json::from_str(&cfg_json).expect("Failed to parse config file");

    let id_path = PathBuf::from(CFG_DIR).join(DEVICE_ID_FILE);
    let local_id = match fs::read_to_string(&id_path) {
        Ok(id) => Uuid::parse_str(&id).unwrap(),
        Err(_) => {
            let id = Uuid::new_v4();
            fs::write(id_path, id.to_string()).expect("Failed to write device.id file");
            id
        }
    };

    (local_id, cfg_dirs)
}

fn build_entries(
    local_id: Uuid,
    configured_dirs: Vec<ConfigFileDirectory>,
    base_dir_path: &CanonicalPath,
    ignore_handler: &mut IgnoreHandler,
) -> io::Result<(
    HashMap<String, SyncDirectory>,
    HashMap<RelativePath, EntryInfo>,
)> {
    let mut dirs = HashMap::new();
    let mut entries = HashMap::new();

    for dir in configured_dirs {
        let path = base_dir_path.join(&dir.folder_name);

        fs::create_dir_all(&path)?;

        if path.is_dir() {
            dirs.insert(
                dir.folder_name.clone(),
                SyncDirectory {
                    name: dir.folder_name,
                },
            );
            build_dir(local_id, path, base_dir_path, &mut entries, ignore_handler)?;
        }
    }

    Ok((dirs, entries))
}

fn build_dir(
    local_id: Uuid,
    dir_path: CanonicalPath,
    base_dir_path: &CanonicalPath,
    entries: &mut HashMap<RelativePath, EntryInfo>,
    ignore_handler: &mut IgnoreHandler,
) -> io::Result<()> {
    let gitignore_path = dir_path.join(".gitignore");

    if gitignore_path.exists() {
        ignore_handler.insert_gitignore(&gitignore_path)?;
    }

    for entry in WalkDir::new(&dir_path).into_iter().filter_map(Result::ok) {
        let path = CanonicalPath::new(entry.path())?;

        if path == dir_path {
            continue;
        }

        let relative_path = RelativePath::new(&path, base_dir_path);

        if ignore_handler.is_ignored(&path, &relative_path) {
            continue;
        }

        if path.is_file() && !is_ds_store(&path) {
            build_file(local_id, path, relative_path, entries)?;
        } else if path.is_dir() {
            entries.insert(
                relative_path.clone(),
                EntryInfo {
                    name: relative_path,
                    kind: EntryKind::Directory,
                    hash: None,
                    version: HashMap::from([(local_id, 0)]),
                },
            );
            build_dir(local_id, path, base_dir_path, entries, ignore_handler)?;
        }
    }

    Ok(())
}

fn build_file(
    local_id: Uuid,
    path: CanonicalPath,
    relative_path: RelativePath,
    entries: &mut HashMap<RelativePath, EntryInfo>,
) -> io::Result<()> {
    let hash = compute_hash(&path)?;

    entries.insert(
        relative_path.clone(),
        EntryInfo {
            name: relative_path,
            kind: EntryKind::File,
            hash: Some(hash),
            version: HashMap::from([(local_id, 0)]),
        },
    );
    Ok(())
}
