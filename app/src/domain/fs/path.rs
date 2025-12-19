use serde::{Deserialize, Serialize};
use std::{
    io,
    ops::Deref,
    path::{Path, PathBuf},
};

/// Wrapper around an absolute filesystem path.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalPath(PathBuf);

impl CanonicalPath {
    /// Creates a `CanonicalPath` by canonicalizing the path.
    /// Requires the path to exist on the filesystem.
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Ok(Self(path.as_ref().canonicalize()?))
    }

    /// Creates a `CanonicalPath` from an absolute path without validation.
    pub fn from_absolute<P: AsRef<Path>>(path: P) -> Self {
        Self(path.as_ref().to_path_buf())
    }

    /// Joins this path with another component.
    ///
    /// Note: result is not canonicalized. Call `new()` if needed.
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

/// Path relative to home directory with forward-slash separators.
///
/// Always uses `/` on all platforms (Windows backslashes converted).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelativePath(String);

impl RelativePath {
    /// Creates a `RelativePath` by stripping `home_path` prefix from `path`.
    /// Returns error if path is not under home_path.
    pub fn new(path: &CanonicalPath, home_path: &CanonicalPath) -> io::Result<Self> {
        let relative = path
            .strip_prefix(home_path)
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "Path '{}' is not under home directory '{}'",
                        path.display(),
                        home_path.display()
                    ),
                )
            })?
            .display()
            .to_string()
            .replace('\\', "/");

        Ok(Self(relative))
    }

    pub fn to_canonical(&self, home_path: &CanonicalPath) -> CanonicalPath {
        home_path.join(&self.0)
    }

    pub fn sync_dir(&self) -> RelativePath {
        self.0.split('/').next().unwrap_or_default().into()
    }

    pub fn starts_with_dir(&self, dir: &RelativePath) -> bool {
        self.0.starts_with(&format!("{}/", dir.0)) || self.0 == dir.0
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

impl AsRef<Path> for RelativePath {
    fn as_ref(&self) -> &Path {
        Path::new(&self.0)
    }
}
