use serde::{Deserialize, Serialize};
use std::{
    fmt, io,
    ops::Deref,
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

/// Marker embedded in conflict-copy file names by `conflict_file_name`.
const CONFLICT_MARKER: &str = "_CONFLICT_";

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

    pub fn is_safe_sync_path(&self) -> bool {
        !self.0.is_empty()
            && !self.0.contains('\\')
            && !self.0.contains('\0')
            && Path::new(&self.0)
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
    }

    /// Builds the file name of a conflict copy from its parts. This is the
    /// **single source** of the `<stem>_CONFLICT_<ms>_<device>_<rand>.<ext>`
    /// format that `is_conflict_file` / `conflict_origin` parse — keep all
    /// three in lockstep when changing the layout.
    pub fn conflict_file_name(stem: &str, ext: &str, ms: u128, device: Uuid, rand: &str) -> String {
        if ext.is_empty() {
            format!("{stem}{CONFLICT_MARKER}{ms}_{device}_{rand}")
        } else {
            format!("{stem}{CONFLICT_MARKER}{ms}_{device}_{rand}.{ext}")
        }
    }

    /// Returns true if this path's final component is a conflict copy
    /// produced by `conflict_file_name`. Validates the trailing
    /// `<ms>_<device>_<rand>` triple precisely so user files that merely
    /// contain `_CONFLICT_` (or names like `.gitignore`) are not
    /// misclassified.
    pub fn is_conflict_file(&self) -> bool {
        self.conflict_stem_split().is_some()
    }

    /// For a conflict copy, returns the original entry path (same parent
    /// directory and extension, with the `_CONFLICT_...` suffix stripped).
    /// Returns `None` when this is not a conflict copy.
    pub fn conflict_origin(&self) -> Option<RelativePath> {
        let (dir, original_stem, ext) = self.conflict_stem_split()?;
        let file = match ext {
            Some(ext) => format!("{original_stem}.{ext}"),
            None => original_stem.to_string(),
        };
        Some(RelativePath(format!("{dir}{file}")))
    }

    /// Splits a conflict-copy path into `(dir_with_trailing_slash,
    /// original_stem, extension)` when the final component is a valid
    /// conflict copy, else `None`. Shared by `is_conflict_file` and
    /// `conflict_origin`.
    fn conflict_stem_split(&self) -> Option<(&str, &str, Option<&str>)> {
        let (dir, file) = match self.0.rfind('/') {
            Some(i) => (&self.0[..=i], &self.0[i + 1..]),
            None => ("", self.0.as_str()),
        };

        let path = Path::new(file);
        let stem = path.file_stem()?.to_str()?;
        let ext = path.extension().and_then(|e| e.to_str());

        // Match the last marker so a conflict-of-a-conflict still parses,
        // keeping the trailing triple as the final three `_`-separated
        // tokens.
        let marker = stem.rfind(CONFLICT_MARKER)?;
        let suffix = &stem[marker + CONFLICT_MARKER.len()..];
        if !is_valid_conflict_suffix(suffix) {
            return None;
        }

        Some((dir, &stem[..marker], ext))
    }
}

/// Validates a conflict-copy suffix shaped `<ms>_<device>_<rand>`:
/// millisecond digits, a parseable device UUID, and an 8-char hex random.
fn is_valid_conflict_suffix(suffix: &str) -> bool {
    let parts: [&str; 3] = match suffix.split('_').collect::<Vec<_>>().try_into() {
        Ok(parts) => parts,
        Err(_) => return false,
    };
    let [ms, device, rand] = parts;

    !ms.is_empty()
        && ms.bytes().all(|b| b.is_ascii_digit())
        && Uuid::parse_str(device).is_ok()
        && rand.len() == 8
        && rand.bytes().all(|b| b.is_ascii_hexdigit())
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

impl fmt::Display for RelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_path_rejects_unsafe_sync_paths() {
        // One representative input per hostile class the protocol decoder
        // must reject: empty, absolute, parent-directory
        // traversal in several forms, backslash separators, mixed
        // separators, and embedded NUL bytes.
        for path in [
            "",                // empty
            "/tmp/file",       // absolute
            "..",              // bare parent
            "../file",         // leading traversal
            "../etc/passwd",   // deeper leading traversal
            "a/../../b",       // interior traversal escaping the root
            "sync/../../file", // traversal under a sync dir
            "a\\b\\c",         // backslash-separated
            "a/b\\c",          // mixed separators
            "a\0b",            // embedded NUL
            "sync/\0",         // embedded NUL in a later component
        ] {
            assert!(
                !RelativePath::from(path).is_safe_sync_path(),
                "expected reject: {path:?}"
            );
        }
    }

    #[test]
    fn relative_path_accepts_normal_forward_slash_paths() {
        for path in ["sync", "sync/file.txt", "sync/nested/file.txt"] {
            assert!(RelativePath::from(path).is_safe_sync_path(), "{path}");
        }
    }

    #[test]
    fn conflict_file_name_round_trips_through_origin() {
        let device = Uuid::new_v4();
        let cases = [
            // (dir, stem, ext, expected original file)
            ("sync/", "img", "jpg", "img.jpg"),
            ("sync/nested/", "report", "pdf", "report.pdf"),
            ("sync/", "README", "", "README"),
            ("sync/", "archive.tar", "gz", "archive.tar.gz"),
        ];

        for (dir, stem, ext, original_file) in cases {
            let file =
                RelativePath::conflict_file_name(stem, ext, 1_717_000_000_000, device, "a1b2c3d4");
            let conflict = RelativePath::from(format!("{dir}{file}"));

            assert!(conflict.is_conflict_file(), "should detect: {conflict}");
            assert_eq!(
                conflict.conflict_origin(),
                Some(RelativePath::from(format!("{dir}{original_file}"))),
                "origin mismatch for {conflict}"
            );
        }
    }

    #[test]
    fn non_conflict_paths_are_not_misclassified() {
        for path in [
            "sync/file.txt",
            "sync/.gitignore",
            "sync/.gitattributes",
            "sync/nested/photo.png",
            // Contains the marker but the trailing triple is invalid.
            "sync/foo_CONFLICT_notvalid.txt",
            "sync/foo_CONFLICT_123_not-a-uuid_a1b2c3d4.txt",
            "sync/foo_CONFLICT_123_00000000-0000-0000-0000-000000000000_zzz.txt",
            "sync/foo_CONFLICT_abc_00000000-0000-0000-0000-000000000000_a1b2c3d4.txt",
        ] {
            let p = RelativePath::from(path);
            assert!(!p.is_conflict_file(), "should not detect: {path}");
            assert_eq!(p.conflict_origin(), None, "should have no origin: {path}");
        }
    }
}
