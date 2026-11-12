//! The desktop's store: the settings in the configuration file, the high
//! scores in `scores.toml` of the data directory (`configuration` spec,
//! saving settings; `high-scores` spec, score file and safe saving).
//!
//! A save writes `<name>.tmp` beside the file and renames it over the
//! file, creating the directory first, so an interrupted write leaves the
//! old or the new complete file. A damaged score file is kept as
//! `scores.toml.bad` before it is first replaced.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{Store, StoreError, StoreKey};

/// The settings and score files.
#[derive(Debug, Clone)]
pub(crate) struct NativeStore {
    config_file: PathBuf,
    scores_file: PathBuf,
}

impl NativeStore {
    /// The store over these two files.
    pub(crate) const fn new(config_file: PathBuf, scores_file: PathBuf) -> Self {
        Self {
            config_file,
            scores_file,
        }
    }

    /// The file `key` is kept in.
    pub(crate) fn path(&self, key: StoreKey) -> &Path {
        match key {
            StoreKey::Settings => &self.config_file,
            StoreKey::Scores => &self.scores_file,
        }
    }
}

impl Store for NativeStore {
    fn load(&self, key: StoreKey) -> Result<Option<String>, StoreError> {
        let path = self.path(key);
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(StoreError::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| StoreError::NotText)
    }

    fn save(&mut self, key: StoreKey, text: &str) -> Result<(), StoreError> {
        write_atomic(self.path(key), text.as_bytes())
    }

    fn set_aside(&mut self, key: StoreKey) -> Result<(), StoreError> {
        let path = self.path(key);
        let bad = bad_path(path);
        match fs::rename(path, &bad) {
            Ok(()) => Ok(()),
            // Gone since it was read: nothing left to keep.
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(StoreError::Io { path: bad, source }),
        }
    }

    fn location(&self, key: StoreKey) -> String {
        self.path(key).display().to_string()
    }

    fn aside_location(&self, key: StoreKey) -> Option<String> {
        Some(bad_path(self.path(key)).display().to_string())
    }
}

/// Writes `bytes` to `<path>.tmp` and renames it over `path`, creating the
/// directory first.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let io_error = |path: &Path| {
        let path = path.to_path_buf();
        move |source| StoreError::Io { path, source }
    };
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(io_error(dir))?;
    }
    let tmp = temporary_path(path);
    fs::write(&tmp, bytes).map_err(io_error(&tmp))?;
    if let Err(source) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(StoreError::Io {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(())
}

/// `config.toml` → `config.toml.tmp`, in the same directory.
pub(crate) fn temporary_path(path: &Path) -> PathBuf {
    sibling(path, ".tmp")
}

/// `scores.toml` → `scores.toml.bad`, in the same directory.
pub(crate) fn bad_path(path: &Path) -> PathBuf {
    sibling(path, ".bad")
}

/// `path` with `suffix` added to its file name.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map_or_else(OsString::new, OsString::from);
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
#[allow(clippy::panic, reason = "a failed expectation is a failed test")]
mod tests {
    use std::fs;

    use super::{NativeStore, bad_path, temporary_path};
    use crate::config::file::tests::TempDir;
    use crate::store::{Store, StoreKey};

    fn store(dir: &TempDir) -> NativeStore {
        NativeStore::new(
            dir.path().join("config").join("config.toml"),
            dir.path().join("data").join("scores.toml"),
        )
    }

    #[test]
    fn store_roundtrip_settings() {
        let dir = TempDir::new("store-settings");
        let mut files = store(&dir);
        assert!(matches!(files.load(StoreKey::Settings), Ok(None)));
        files
            .save(StoreKey::Settings, "[game]\nmode = \"gauntlet\"\n")
            .unwrap_or_else(|e| panic!("{e}"));
        let path = files.path(StoreKey::Settings).to_path_buf();
        assert_eq!(
            fs::read_to_string(&path).ok().as_deref(),
            Some("[game]\nmode = \"gauntlet\"\n")
        );
        assert!(!temporary_path(&path).exists());
        assert_eq!(
            files.load(StoreKey::Settings).ok().flatten().as_deref(),
            Some("[game]\nmode = \"gauntlet\"\n")
        );
        // The settings are not the scores.
        assert!(matches!(files.load(StoreKey::Scores), Ok(None)));
    }

    #[test]
    fn store_roundtrip_scores() {
        let dir = TempDir::new("store-scores");
        let mut files = store(&dir);
        files
            .save(StoreKey::Scores, "format = 1\n")
            .unwrap_or_else(|e| panic!("{e}"));
        files
            .save(StoreKey::Scores, "format = 1\n# again\n")
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            files.load(StoreKey::Scores).ok().flatten().as_deref(),
            Some("format = 1\n# again\n")
        );
        // A damaged file is kept aside once; a second set-aside with the
        // file gone is not an error.
        let path = files.path(StoreKey::Scores).to_path_buf();
        files
            .set_aside(StoreKey::Scores)
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(!path.exists());
        assert!(bad_path(&path).exists());
        assert!(files.set_aside(StoreKey::Scores).is_ok());
        assert_eq!(
            files.aside_location(StoreKey::Scores),
            Some(bad_path(&path).display().to_string())
        );
    }

    #[test]
    fn store_missing_and_unreadable() {
        let dir = TempDir::new("store-missing");
        let files = store(&dir);
        assert!(matches!(files.load(StoreKey::Settings), Ok(None)));
        assert!(matches!(files.load(StoreKey::Scores), Ok(None)));
        let path = files.path(StoreKey::Scores);
        fs::create_dir_all(path.parent().unwrap_or(path)).unwrap_or_else(|e| panic!("{e}"));
        fs::write(path, b"\xff\xfe").unwrap_or_else(|e| panic!("{e}"));
        assert!(
            files
                .load(StoreKey::Scores)
                .is_err_and(|e| e.to_string() == "is not UTF-8 text")
        );
    }
}
