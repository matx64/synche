use crate::{
    application::IgnoreHandler,
    domain::{CanonicalPath, ConfiguredDirectory, Directory, EntryInfo, EntryKind, RelativePath},
    utils::fs::{compute_hash, is_ds_store},
};
use std::{
    collections::HashMap,
    fs::{self},
};
use std::{io, path::PathBuf};
use uuid::Uuid;
use walkdir::WalkDir;

pub struct Config {
    pub local_id: Uuid,
    pub sync_directories: HashMap<String, Directory>,
    pub filesystem_entries: HashMap<RelativePath, EntryInfo>,
    pub ignore_handler: IgnoreHandler,
    pub base_dir_path: CanonicalPath,
    pub tmp_dir_path: CanonicalPath,
}

pub fn init() -> Config {
    let (local_id, configured_dirs) = load_config_file();
    let (base_dir_path, tmp_dir_path) = create_required_dirs();

    let mut ignore_handler = IgnoreHandler::new(base_dir_path.clone());
    let (sync_directories, filesystem_entries) = build_entries(
        local_id,
        configured_dirs,
        &base_dir_path,
        &mut ignore_handler,
    )
    .unwrap();

    tracing_subscriber::fmt::init();

    Config {
        local_id,
        sync_directories,
        filesystem_entries,
        ignore_handler,
        base_dir_path,
        tmp_dir_path,
    }
}

fn load_config_file() -> (Uuid, Vec<ConfiguredDirectory>) {
    let cfg_path = ".synche";

    let settings_path = PathBuf::from(cfg_path).join("settings.json");
    let settings_json = fs::read_to_string(settings_path).expect("Failed to read config file");
    let settings_dirs = serde_json::from_str(&settings_json).expect("Failed to parse config file");

    let id_path = PathBuf::from(cfg_path).join("device.id");
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

fn create_required_dirs() -> (CanonicalPath, CanonicalPath) {
    let base_dir_path = CanonicalPath::new("synche-files").unwrap();
    let tmp_dir_path = CanonicalPath::new(".tmp").unwrap();

    fs::create_dir_all(&tmp_dir_path).unwrap();
    fs::create_dir_all(&base_dir_path).unwrap();

    (base_dir_path, tmp_dir_path)
}

fn build_entries(
    local_id: Uuid,
    configured_dirs: Vec<ConfiguredDirectory>,
    base_dir_path: &CanonicalPath,
    ignore_handler: &mut IgnoreHandler,
) -> io::Result<(HashMap<String, Directory>, HashMap<RelativePath, EntryInfo>)> {
    let mut dirs = HashMap::new();
    let mut entries = HashMap::new();

    for dir in configured_dirs {
        let path = base_dir_path.join(&dir.name);

        fs::create_dir_all(&path).unwrap();

        if path.is_dir() {
            dirs.insert(dir.name.clone(), Directory { name: dir.name });
            build_dir(local_id, &path, base_dir_path, &mut entries, ignore_handler)?;
        }
    }

    Ok((dirs, entries))
}

fn build_dir(
    local_id: Uuid,
    dir_path: &CanonicalPath,
    base_dir_path: &CanonicalPath,
    entries: &mut HashMap<RelativePath, EntryInfo>,
    ignore_handler: &mut IgnoreHandler,
) -> io::Result<()> {
    let gitignore_path = dir_path.join(".gitignore");

    if gitignore_path.exists() {
        ignore_handler.insert_gitignore(&gitignore_path)?;
    }

    for entry in WalkDir::new(dir_path).into_iter().filter_map(Result::ok) {
        let path = CanonicalPath::new(entry.path())?;

        if path == *dir_path {
            continue;
        }

        let relative_path = RelativePath::new(&path, base_dir_path);

        if ignore_handler.is_ignored(&path, &relative_path) {
            continue;
        }

        if path.is_file() && !is_ds_store(&path) {
            build_file(local_id, &path, relative_path, entries)?;
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
            build_dir(local_id, &path, base_dir_path, entries, ignore_handler)?;
        }
    }

    Ok(())
}

fn build_file(
    local_id: Uuid,
    path: &CanonicalPath,
    relative_path: RelativePath,
    entries: &mut HashMap<RelativePath, EntryInfo>,
) -> io::Result<()> {
    let hash = compute_hash(path)?;

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
