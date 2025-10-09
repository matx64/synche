pub struct Settings {
    pub base_dir_path: &'static str,
    pub tmp_dir_path: &'static str,
    pub cfg_dir_path: &'static str,
    pub cfg_file: &'static str,
    pub device_id_file: &'static str,
}

pub fn new_default() -> Settings {
    Settings {
        base_dir_path: "./synche-files",
        tmp_dir_path: "./.tmp",
        cfg_dir_path: "./.synche",
        cfg_file: "config.json",
        device_id_file: "device.id",
    }
}
