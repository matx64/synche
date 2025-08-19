use std::{
    io,
    ops::Deref,
    path::{Path, PathBuf},
};

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
