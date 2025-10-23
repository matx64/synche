use serde::{Deserialize, Serialize};
use std::{
    io,
    ops::Deref,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalPath(PathBuf);

impl CanonicalPath {
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Ok(Self(path.as_ref().canonicalize()?))
    }

    pub fn from_canonical<P: AsRef<Path>>(path: P) -> Self {
        Self(path.as_ref().to_path_buf())
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelativePath(String);

impl RelativePath {
    pub fn new(path: &CanonicalPath, home_path: &CanonicalPath) -> Self {
        let relative = path
            .strip_prefix(home_path)
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

impl From<String> for RelativePath {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for RelativePath {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}
