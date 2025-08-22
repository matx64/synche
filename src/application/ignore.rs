use crate::domain::{CanonicalPath, RelativePath};
use ignore::gitignore::Gitignore;
use std::collections::HashMap;
use tokio::io;
use tracing::warn;

pub struct IgnoreHandler {
    gis: HashMap<String, Gitignore>,
    base_dir_path: CanonicalPath,
}

impl IgnoreHandler {
    pub fn new(base_dir_path: CanonicalPath) -> Self {
        Self {
            gis: HashMap::new(),
            base_dir_path,
        }
    }

    pub fn insert_gitignore(&mut self, gitignore_path: &CanonicalPath) -> io::Result<bool> {
        let (gi, err) = Gitignore::new(gitignore_path);

        if let Some(err) = err {
            warn!("Gitignore error: {err}");
        }

        if gi.is_empty() {
            return Ok(false);
        };

        if let Some(relative) =
            RelativePath::new(gitignore_path, &self.base_dir_path).strip_suffix("/.gitignore")
        {
            self.gis.insert(relative.to_string(), gi);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn is_ignored(&self, path: &CanonicalPath, relative: &RelativePath) -> bool {
        if self.gis.is_empty() {
            return false;
        };

        let is_dir = path.is_dir();

        let mut current_path = String::with_capacity(relative.len());

        let mut parts = relative.split('/').peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                // skip last/self path
                break;
            }

            if !current_path.is_empty() {
                current_path.push('/');
            }
            current_path.push_str(part);

            if let Some(gi) = self.gis.get(&current_path)
                && gi.matched_path_or_any_parents(path, is_dir).is_ignore()
            {
                return true;
            }
        }
        false
    }

    pub fn remove_gitignore(&mut self, relative: &RelativePath) {
        if let Some(key) = relative.strip_suffix("/.gitignore") {
            self.gis.remove(key);
        }
    }
}
