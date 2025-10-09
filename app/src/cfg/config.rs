use crate::domain::SyncDirectoryConfigured;
use std::{fs, io, path::PathBuf};
use uuid::Uuid;

pub struct Config {
    pub base_dir_path: &'static str,
    pub tmp_dir_path: &'static str,
    pub cfg_dir_path: &'static str,
    pub cfg_file: &'static str,
    pub device_id_file: &'static str,
}

impl Config {
    pub fn init(&self) -> (Uuid, Vec<SyncDirectoryConfigured>) {
        self.create_required_dirs().unwrap();

        (self.init_device_id(), self.load_config_file())
    }

    fn create_required_dirs(&self) -> io::Result<()> {
        fs::create_dir_all(self.base_dir_path)?;
        fs::create_dir_all(self.tmp_dir_path)?;
        fs::create_dir_all(self.cfg_dir_path)?;
        Ok(())
    }

    fn init_device_id(&self) -> Uuid {
        let file = PathBuf::from(self.cfg_dir_path).join(self.device_id_file);

        match fs::read_to_string(&file) {
            Ok(id) if !id.trim().is_empty() => Uuid::parse_str(&id).unwrap(),
            _ => {
                let id = Uuid::new_v4();
                fs::write(file, id.to_string()).expect("Failed to write device.id file");
                id
            }
        }
    }

    fn load_config_file(&self) -> Vec<SyncDirectoryConfigured> {
        let file = PathBuf::from(self.cfg_dir_path).join(self.cfg_file);

        if !file.exists() {
            fs::write(&file, "[{\"folder_name\": \"myfolder\"}]").unwrap();
        }

        let cfg_json = fs::read_to_string(file).expect("Failed to read config file");

        serde_json::from_str(&cfg_json).expect("Failed to parse config file")
    }
}

pub fn new_default() -> Config {
    Config {
        base_dir_path: "./synche-files",
        tmp_dir_path: "./.tmp",
        cfg_dir_path: "./.synche",
        cfg_file: "config.json",
        device_id_file: "device.id",
    }
}
