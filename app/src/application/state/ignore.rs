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

        if let Ok(path_relative) = RelativePath::new(gitignore_path, self.state.home_path())
            && let Some(relative) = path_relative.strip_suffix("/.gitignore")
        {
            self.gis.write().await.insert(relative.to_string(), gi);
            info!("Inserted or Updated .gitignore: {relative}");
        }
    }

    pub async fn is_ignored(&self, path: &CanonicalPath, relative: &RelativePath) -> bool {
        let gis = self.gis.read().await;
        if gis.is_empty() {
            return false;
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

            if let Some(gi) = gis.get(&current_path)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    async fn setup_test_env() -> (TempDir, Arc<AppState>, IgnoreHandler) {
        let state = AppState::new().await;
        let home_path = state.home_path().clone();

        let temp_dir = TempDir::new_in(&home_path).unwrap();
        let handler = IgnoreHandler::new(state.clone());

        (temp_dir, state, handler)
    }

    fn create_gitignore(dir: &CanonicalPath, patterns: &[&str]) -> CanonicalPath {
        let gitignore_path = dir.join(".gitignore");
        fs::write(&gitignore_path, patterns.join("\n")).unwrap();
        gitignore_path
    }

    #[tokio::test]
    async fn test_insert_gitignore() {
        let (temp_dir, state, handler) = setup_test_env().await;

        let sync_dir = CanonicalPath::from_absolute(temp_dir.path());
        let gitignore_path = create_gitignore(&sync_dir, &["*.log", "temp/"]);

        handler.insert_gitignore(&gitignore_path).await;

        let relative = RelativePath::new(&sync_dir, state.home_path()).unwrap();
        let dir_name: &str = relative.as_ref();

        assert_eq!(handler.gis.read().await.len(), 1);
        assert!(handler.gis.read().await.contains_key(dir_name));
    }

    #[tokio::test]
    async fn test_insert_empty_gitignore() {
        let (temp_dir, _state, handler) = setup_test_env().await;

        let sync_dir = CanonicalPath::from_absolute(temp_dir.path());
        let gitignore_path = create_gitignore(&sync_dir, &[]);

        handler.insert_gitignore(&gitignore_path).await;

        assert_eq!(handler.gis.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_is_ignored_no_gitignore() {
        let (temp_dir, state, handler) = setup_test_env().await;

        let sync_dir = CanonicalPath::from_absolute(temp_dir.path());
        let file_path = sync_dir.join("test.log");
        let relative = RelativePath::new(&file_path, state.home_path()).unwrap();

        let ignored = handler.is_ignored(&file_path, &relative).await;
        assert!(!ignored);
    }

    #[tokio::test]
    async fn test_is_ignored_file_pattern() {
        let (temp_dir, state, handler) = setup_test_env().await;

        let sync_dir = CanonicalPath::from_absolute(temp_dir.path());
        let gitignore_path = create_gitignore(&sync_dir, &["*.log", "*.tmp"]);

        handler.insert_gitignore(&gitignore_path).await;

        let log_file = sync_dir.join("debug.log");
        fs::write(&log_file, "test").unwrap();
        let relative_log = RelativePath::new(&log_file, state.home_path()).unwrap();

        let ignored = handler.is_ignored(&log_file, &relative_log).await;
        assert!(ignored);

        let txt_file = sync_dir.join("readme.txt");
        fs::write(&txt_file, "test").unwrap();
        let relative_txt = RelativePath::new(&txt_file, state.home_path()).unwrap();

        let ignored = handler.is_ignored(&txt_file, &relative_txt).await;
        assert!(!ignored);
    }

    #[tokio::test]
    async fn test_is_ignored_directory_pattern() {
        let (temp_dir, state, handler) = setup_test_env().await;

        let sync_dir = CanonicalPath::from_absolute(temp_dir.path());
        let gitignore_path = create_gitignore(&sync_dir, &["temp/", "build/"]);

        handler.insert_gitignore(&gitignore_path).await;

        let temp_dir_path = sync_dir.join("temp");
        fs::create_dir(&temp_dir_path).unwrap();
        let relative_temp = RelativePath::new(&temp_dir_path, state.home_path()).unwrap();

        let ignored = handler.is_ignored(&temp_dir_path, &relative_temp).await;
        assert!(ignored);

        let src_dir_path = sync_dir.join("src");
        fs::create_dir(&src_dir_path).unwrap();
        let relative_src = RelativePath::new(&src_dir_path, state.home_path()).unwrap();

        let ignored = handler.is_ignored(&src_dir_path, &relative_src).await;
        assert!(!ignored);
    }

    #[tokio::test]
    async fn test_is_ignored_nested_gitignore() {
        let (temp_dir, state, handler) = setup_test_env().await;

        let sync_dir = CanonicalPath::from_absolute(temp_dir.path());
        create_gitignore(&sync_dir, &["*.log"]);

        let nested_dir = sync_dir.join("subdir");
        fs::create_dir(&nested_dir).unwrap();
        create_gitignore(&nested_dir, &["*.tmp"]);

        handler.insert_gitignore(&sync_dir.join(".gitignore")).await;
        handler
            .insert_gitignore(&nested_dir.join(".gitignore"))
            .await;

        let log_file = nested_dir.join("test.log");
        fs::write(&log_file, "test").unwrap();
        let relative_log = RelativePath::new(&log_file, state.home_path()).unwrap();

        let ignored = handler.is_ignored(&log_file, &relative_log).await;
        assert!(ignored);

        let tmp_file = nested_dir.join("test.tmp");
        fs::write(&tmp_file, "test").unwrap();
        let relative_tmp = RelativePath::new(&tmp_file, state.home_path()).unwrap();

        let ignored = handler.is_ignored(&tmp_file, &relative_tmp).await;
        assert!(ignored);

        let txt_file = nested_dir.join("test.txt");
        fs::write(&txt_file, "test").unwrap();
        let relative_txt = RelativePath::new(&txt_file, state.home_path()).unwrap();

        let ignored = handler.is_ignored(&txt_file, &relative_txt).await;
        assert!(!ignored);
    }

    #[tokio::test]
    async fn test_remove_gitignore() {
        let (temp_dir, state, handler) = setup_test_env().await;

        let sync_dir = CanonicalPath::from_absolute(temp_dir.path());
        let gitignore_path = create_gitignore(&sync_dir, &["*.log"]);

        handler.insert_gitignore(&gitignore_path).await;

        assert_eq!(handler.gis.read().await.len(), 1);

        let relative = RelativePath::new(&gitignore_path, state.home_path()).unwrap();
        handler.remove_gitignore(&relative).await;

        assert_eq!(handler.gis.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_remove_nonexistent_gitignore() {
        let (temp_dir, state, handler) = setup_test_env().await;

        let sync_dir = CanonicalPath::from_absolute(temp_dir.path());
        let gitignore_path = sync_dir.join(".gitignore");
        let relative = RelativePath::new(&gitignore_path, state.home_path()).unwrap();

        handler.remove_gitignore(&relative).await;

        assert_eq!(handler.gis.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_update_gitignore() {
        let (temp_dir, state, handler) = setup_test_env().await;

        let sync_dir = CanonicalPath::from_absolute(temp_dir.path());
        let gitignore_path = create_gitignore(&sync_dir, &["*.log"]);

        handler.insert_gitignore(&gitignore_path).await;

        let gitignore_path = create_gitignore(&sync_dir, &["*.log", "*.tmp"]);
        handler.insert_gitignore(&gitignore_path).await;

        assert_eq!(handler.gis.read().await.len(), 1);

        let tmp_file = sync_dir.join("test.tmp");
        fs::write(&tmp_file, "test").unwrap();
        let relative = RelativePath::new(&tmp_file, state.home_path()).unwrap();

        let ignored = handler.is_ignored(&tmp_file, &relative).await;
        assert!(ignored);
    }

    #[tokio::test]
    async fn test_is_ignored_deep_nesting() {
        let (temp_dir, state, handler) = setup_test_env().await;

        let sync_dir = CanonicalPath::from_absolute(temp_dir.path());
        create_gitignore(&sync_dir, &["*.log"]);

        let nested = sync_dir.join("a/b/c");
        fs::create_dir_all(&nested).unwrap();

        handler.insert_gitignore(&sync_dir.join(".gitignore")).await;

        let log_file = nested.join("deep.log");
        fs::write(&log_file, "test").unwrap();
        let relative = RelativePath::new(&log_file, state.home_path()).unwrap();

        let ignored = handler.is_ignored(&log_file, &relative).await;
        assert!(ignored);
    }

    #[tokio::test]
    async fn test_is_ignored_with_negation() {
        let (temp_dir, state, handler) = setup_test_env().await;

        let sync_dir = CanonicalPath::from_absolute(temp_dir.path());
        let gitignore_path = create_gitignore(&sync_dir, &["*.log", "!important.log"]);

        handler.insert_gitignore(&gitignore_path).await;

        let normal_log = sync_dir.join("debug.log");
        fs::write(&normal_log, "test").unwrap();
        let relative_normal = RelativePath::new(&normal_log, state.home_path()).unwrap();

        let ignored = handler.is_ignored(&normal_log, &relative_normal).await;
        assert!(ignored);

        let important_log = sync_dir.join("important.log");
        fs::write(&important_log, "test").unwrap();
        let relative_important = RelativePath::new(&important_log, state.home_path()).unwrap();

        let ignored = handler
            .is_ignored(&important_log, &relative_important)
            .await;
        assert!(!ignored);
    }
}
