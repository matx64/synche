use crate::{config::settings::Settings, domain::CanonicalPath};
use std::{fs, io, path::PathBuf};
use uuid::Uuid;

pub struct AppState {
    pub local_id: Uuid,
    pub paths: AppStatePaths,
}

pub struct AppStatePaths {
    pub base_dir_path: CanonicalPath,
    pub tmp_dir_path: CanonicalPath,
    pub cfg_dir_path: CanonicalPath,
    pub cfg_file: CanonicalPath,
    device_id_file: CanonicalPath,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        let paths = Self::create_required_paths(&settings).unwrap();
        let local_id = Self::get_local_id(&paths.device_id_file);
        todo!()
    }

    fn create_required_paths(settings: &Settings) -> io::Result<AppStatePaths> {
        fs::create_dir_all(settings.base_dir_path)?;
        fs::create_dir_all(settings.tmp_dir_path)?;
        fs::create_dir_all(settings.cfg_dir_path)?;

        let base_dir_path = CanonicalPath::new(settings.base_dir_path)?;
        let tmp_dir_path = CanonicalPath::new(settings.tmp_dir_path)?;
        let cfg_dir_path = CanonicalPath::new(settings.cfg_dir_path)?;

        let cfg_file_path = PathBuf::from(cfg_dir_path.as_ref()).join(settings.cfg_file);

        if !cfg_file_path.exists() {
            fs::write(&cfg_file_path, "[{\"folder_name\": \"myfolder\"}]")?;
        }

        let id_file_path = PathBuf::from(cfg_dir_path.as_ref()).join(settings.device_id_file);

        if !id_file_path.exists() {
            fs::write(&id_file_path, "")?;
        }

        Ok(AppStatePaths {
            base_dir_path,
            tmp_dir_path,
            cfg_dir_path,
            cfg_file: CanonicalPath::from_canonical(cfg_file_path),
            device_id_file: CanonicalPath::from_canonical(id_file_path),
        })
    }

    fn get_local_id(file: &CanonicalPath) -> Uuid {
        match fs::read_to_string(file) {
            Ok(id) if !id.trim().is_empty() => Uuid::parse_str(&id).unwrap(),
            _ => {
                let id = Uuid::new_v4();
                fs::write(file, id.to_string()).expect("Failed to write device.id file");
                id
            }
        }
    }
}

pub fn new_default(settings: Settings) -> AppState {
    AppState::new(settings)
}
