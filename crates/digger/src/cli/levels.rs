//! Checking and loading the level file named by `--levels` or a bare
//! argument (`configuration` spec, level file option; research 01 §5.5).
//!
//! A level file holds a 2-byte bonus-life score and the 1200 bytes of the
//! eight layouts; bytes after the first 1202 are ignored. If the named file
//! does not exist and its name has no extension, `<name>.dlf` is tried.
//! Upstream drops a bad level file silently; here it ends the program with
//! exit status 66 before a window opens.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use digger_core::{CUSTOM_LEVELS_LEN, LevelSet};

use crate::error::AppError;

/// The extension tried when the named file does not exist.
pub(crate) const RETRY_EXTENSION: &str = "dlf";

/// A level file that was read and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LevelFile {
    /// The file actually read: the name given, or it with `.dlf` added.
    pub(crate) path: PathBuf,
    /// The layouts and bonus-life score it holds.
    pub(crate) levels: LevelSet,
}

/// Reads and checks the level file `name`.
///
/// # Errors
///
/// [`AppError::LevelFile`] (status 66), naming `name` as given, when the
/// file (and its `.dlf` variant) cannot be read or is shorter than 1202
/// bytes.
pub(crate) fn load(name: &Path) -> Result<LevelFile, AppError> {
    let fail = |source: io::Error| AppError::LevelFile {
        path: name.to_path_buf(),
        source,
    };
    let (path, bytes) = match fs::read(name) {
        Ok(bytes) => (name.to_path_buf(), bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound && name.extension().is_none() => {
            let retry = name.with_extension(RETRY_EXTENSION);
            match fs::read(&retry) {
                Ok(bytes) => (retry, bytes),
                // Report the name as given, with the original error.
                Err(_) => return Err(fail(error)),
            }
        }
        Err(error) => return Err(fail(error)),
    };
    let levels = LevelSet::from_custom_bytes(&bytes).map_err(|_| {
        fail(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "the file is {} bytes long, a level file has at least {CUSTOM_LEVELS_LEN}",
                bytes.len()
            ),
        ))
    })?;
    Ok(LevelFile { path, levels })
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use std::fs;

    use super::load;
    use crate::config::file::tests::TempDir;
    use crate::error::AppError;

    /// The bytes of a level file with bonus step 0x1234 and `fill` in every
    /// layout cell, `extra` bytes longer than the minimum.
    fn level_bytes(fill: u8, extra: usize) -> Vec<u8> {
        let mut bytes = vec![0x34, 0x12];
        bytes.resize(1202 + extra, fill);
        bytes
    }

    #[test]
    fn level_file_missing() {
        let dir = TempDir::new("levels-missing");
        let name = dir.path().join("nosuch");
        match load(&name) {
            Err(error @ AppError::LevelFile { .. }) => {
                assert_eq!(error.exit_code(), 66);
                let text = error.to_string();
                assert!(text.starts_with("cannot read level file "), "{text}");
                assert!(text.contains(&name.display().to_string()), "{text}");
                // The `.dlf` retry is not named: the user named `nosuch`.
                assert!(!text.contains("nosuch.dlf"), "{text}");
            }
            other => panic!("{other:?}"),
        }
        // A name with an extension is not retried.
        fs::write(dir.path().join("l.txt.dlf"), level_bytes(b'S', 0))
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(load(&dir.path().join("l.txt")).is_err());
        // A directory is not a level file.
        assert!(matches!(load(dir.path()), Err(AppError::LevelFile { .. })));
    }

    #[test]
    fn level_file_dlf_retry() {
        let dir = TempDir::new("levels-retry");
        fs::write(dir.path().join("mylevels.dlf"), level_bytes(b'C', 0))
            .unwrap_or_else(|e| panic!("{e}"));
        let file = load(&dir.path().join("mylevels")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(file.path, dir.path().join("mylevels.dlf"));
        assert_eq!(file.levels.bonus_step(), 0x1234);
        assert!(file.levels.is_verbatim());
        assert_eq!(file.levels.layouts()[7].rows()[9][14], b'C');
        // The name as given wins when it exists.
        fs::write(dir.path().join("mylevels"), level_bytes(b'B', 10))
            .unwrap_or_else(|e| panic!("{e}"));
        let file = load(&dir.path().join("mylevels")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(file.path, dir.path().join("mylevels"));
        assert_eq!(file.levels.layouts()[0].rows()[0][0], b'B');
    }

    #[test]
    fn level_file_too_short() {
        let dir = TempDir::new("levels-short");
        let name = dir.path().join("short.dlf");
        let mut bytes = level_bytes(b'S', 0);
        bytes.pop();
        fs::write(&name, &bytes).unwrap_or_else(|e| panic!("{e}"));
        match load(&name) {
            Err(error @ AppError::LevelFile { .. }) => {
                assert_eq!(error.exit_code(), 66);
                let text = error.to_string();
                assert!(text.contains("short.dlf"), "{text}");
                assert!(text.contains("1201 bytes"), "{text}");
                assert!(text.contains("1202"), "{text}");
            }
            other => panic!("{other:?}"),
        }
        // Exactly 1202 bytes is enough.
        bytes.push(b'S');
        fs::write(&name, &bytes).unwrap_or_else(|e| panic!("{e}"));
        assert!(load(&name).is_ok());
    }
}
