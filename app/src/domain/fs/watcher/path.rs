use crate::domain::{CanonicalPath, RelativePath};

#[derive(Debug, Clone)]
pub struct WatcherEventPath {
    pub relative: RelativePath,
    pub canonical: CanonicalPath,
}

impl WatcherEventPath {
    pub fn is_file(&self) -> bool {
        self.canonical.is_file()
    }
}
