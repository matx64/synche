use std::{
    io,
    ops::Deref,
    path::{Path, PathBuf},
};

pub struct SyncPath {
    pub relative: RelativePath,
    pub canonical: Option<CanonicalPath>,
    pub exists: bool,
    pub is_file: bool,
    pub is_dir: bool,
}

impl SyncPath {
    pub fn new<P: AsRef<Path>>(path: P, base_dir_path: &CanonicalPath) -> io::Result<Option<Self>> {
        let path = path.as_ref();

        let relative = RelativePath::new(&path, base_dir_path);

        let exists = path.exists();
        let mut canonical = None;
        let mut is_file = false;
        let mut is_dir = false;

        if exists {
            if path.is_file() {
                is_file = true;
            } else if path.is_dir() {
                is_dir = true;
            } else {
                return Ok(None);
            }
            canonical = Some(CanonicalPath::new(path)?);
        }

        Ok(Some(Self {
            relative,
            canonical,
            exists,
            is_file,
            is_dir,
        }))
    }
}

pub struct RelativePath(String);

impl RelativePath {
    pub fn new<P: AsRef<Path>>(path: P, base_dir_path: &CanonicalPath) -> Self {
        let path = path.as_ref();

        let relative = path
            .strip_prefix(base_dir_path)
            .expect(&format!(
                "Path isn`t from a sync directory: {}",
                path.to_string_lossy()
            ))
            .display()
            .to_string()
            .replace('\\', "/");

        Self(relative)
    }
}

impl Deref for RelativePath {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for RelativePath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

pub struct CanonicalPath(PathBuf);

impl CanonicalPath {
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Ok(Self(path.as_ref().canonicalize()?))
    }
}

impl Deref for CanonicalPath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<Path> for CanonicalPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}
