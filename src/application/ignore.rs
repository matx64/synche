use crate::utils::fs::get_relative_path;
use ignore::gitignore::Gitignore;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio::io;
use tracing::warn;

pub struct IgnoreHandler {
    gis: HashMap<String, Gitignore>,
    base_dir_absolute: PathBuf,
}

impl IgnoreHandler {
    pub fn new(base_dir_absolute: PathBuf) -> Self {
        Self {
            gis: HashMap::new(),
            base_dir_absolute,
        }
    }

    pub fn insert_gitignore<P: AsRef<Path>>(&mut self, gitignore_path: P) -> io::Result<bool> {
        let (gi, err) = Gitignore::new(&gitignore_path);

        if let Some(err) = err {
            warn!("Gitignore error: {err}");
        }

        if gi.is_empty() {
            return Ok(false);
        }

        if let Some(rel) = get_relative_path(gitignore_path.as_ref(), &self.base_dir_absolute)?
            .strip_suffix("/.gitignore")
        {
            self.gis.insert(rel.to_string(), gi);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
