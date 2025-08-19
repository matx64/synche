use std::{
    io,
    ops::Deref,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalPath(PathBuf);

impl CanonicalPath {
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let canonical = path.as_ref().canonicalize()?;
        Ok(Self(canonical))
    }

    pub fn join<P: AsRef<Path>>(&self, path: P) -> Self {
        let buf = self.0.join(path);
        Self(buf)
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RelativePath(pub String);

impl RelativePath {
    pub fn new(path: &CanonicalPath, base: &CanonicalPath) -> io::Result<Self> {
        let relative = path
            .strip_prefix(base)
            .map_err(|err| io::Error::other(format!("Couldn't extract relative path: {err}")))?;

        Ok(Self(relative.display().to_string().replace('\\', "/")))
    }
}

impl Deref for RelativePath {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
