use std::{
    io,
    ops::Deref,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct CanonicalPath(PathBuf);

impl CanonicalPath {
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Ok(Self(path.as_ref().canonicalize()?))
    }

    pub fn join<P: AsRef<Path>>(&self, path: P) -> CanonicalPath {
        let buf = self.0.join(path);
        CanonicalPath(buf)
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

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct RelativePath(String);

impl RelativePath {
    pub fn new(path: &CanonicalPath, base_dir_path: &CanonicalPath) -> Self {
        let relative = path
            .strip_prefix(base_dir_path)
            .unwrap_or_else(|_| {
                panic!(
                    "Path isn`t from a sync directory: {}",
                    path.to_string_lossy()
                )
            })
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
