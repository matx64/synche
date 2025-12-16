use crate::{
    application::AppState,
    domain::{CanonicalPath, RelativePath},
};
use ignore::gitignore::Gitignore;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tracing::{info, warn};

pub struct IgnoreHandler {
    state: Arc<AppState>,
    gis: RwLock<HashMap<String, Gitignore>>,
}

impl IgnoreHandler {
    pub fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            gis: Default::default(),
        }
    }

    pub async fn insert_gitignore(&self, gitignore_path: &CanonicalPath) {
        let (gi, err) = Gitignore::new(gitignore_path);

        if let Some(err) = err {
            warn!("Gitignore error: {err}");
        }

        if gi.is_empty() {
            return;
        };

        if let Some(relative) =
            RelativePath::new(gitignore_path, self.state.home_path()).strip_suffix("/.gitignore")
        {
            self.gis.write().await.insert(relative.to_string(), gi);
            info!("⭕  Inserted or Updated .gitignore: {relative}");
        }
    }

    pub async fn is_ignored(&self, path: &CanonicalPath, relative: &RelativePath) -> bool {
        {
            if self.gis.read().await.is_empty() {
                return false;
            }
        }

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

            if let Some(gi) = self.gis.read().await.get(&current_path)
                && gi.matched_path_or_any_parents(path, is_dir).is_ignore()
            {
                return true;
            }
        }
        false
    }

    pub async fn remove_gitignore(&self, relative: &RelativePath) {
        if let Some(key) = relative.strip_suffix("/.gitignore") {
            self.gis.write().await.remove(key);
        }
    }
}
