//! Platform configuration and data directories, with the `--config` and
//! `--data-dir` overrides (design D9).
//!
//! The configuration file is `config.toml` in the per-user configuration
//! directory (`configuration` spec, configuration file location); score
//! tables and saved recordings live in the per-user data directory
//! (`high-scores` spec, score storage):
//!
//! | Platform | Configuration | Data |
//! |---|---|---|
//! | Linux | `$XDG_CONFIG_HOME/digger/` or `~/.config/digger/` | `$XDG_DATA_HOME/digger/` or `~/.local/share/digger/` |
//! | Windows | `%APPDATA%\digger\config\` | `%APPDATA%\digger\data\` |
//! | macOS | `~/Library/Application Support/digger/` | the same |

use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
use directories::ProjectDirs;

#[cfg(not(target_arch = "wasm32"))]
use crate::error::AppError;

/// The configuration file's name in the configuration directory.
pub(crate) const CONFIG_FILE: &str = "config.toml";
/// The score file's name in the data directory.
pub(crate) const SCORES_FILE: &str = "scores.toml";
/// The folder for recordings saved by key, in the data directory.
pub(crate) const RECORDINGS_DIR: &str = "recordings";

/// Where the program keeps its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Paths {
    /// The configuration file (`--config`, or `config.toml` in the
    /// configuration directory).
    pub(crate) config_file: PathBuf,
    /// The data directory (`--data-dir`, or the platform's).
    pub(crate) data_dir: PathBuf,
}

impl Paths {
    #[cfg(not(target_arch = "wasm32"))]
    /// The paths for this user: each override if given, else the
    /// platform's directory.
    ///
    /// # Errors
    ///
    /// [`AppError::Config`] when a platform directory is needed but the
    /// user has no home directory.
    pub(crate) fn resolve(
        config_override: Option<PathBuf>,
        data_dir_override: Option<PathBuf>,
    ) -> Result<Self, AppError> {
        let platform = || {
            ProjectDirs::from("", "", "digger").ok_or_else(|| AppError::Config {
                file: PathBuf::from(CONFIG_FILE),
                key: None,
                problem: "no home directory to keep settings and scores in; \
                          give --config and --data-dir"
                    .to_owned(),
            })
        };
        let config_file = match config_override {
            Some(file) => file,
            None => platform()?.config_dir().join(CONFIG_FILE),
        };
        let data_dir = match data_dir_override {
            Some(dir) => dir,
            None => platform()?.data_dir().to_path_buf(),
        };
        Ok(Self {
            config_file,
            data_dir,
        })
    }

    /// The configuration file.
    pub(crate) fn config_file(&self) -> &Path {
        &self.config_file
    }

    /// The score file, `scores.toml` in the data directory.
    pub(crate) fn scores_file(&self) -> PathBuf {
        self.data_dir.join(SCORES_FILE)
    }

    /// The folder recordings are saved into by key.
    pub(crate) fn recordings_dir(&self) -> PathBuf {
        self.data_dir.join(RECORDINGS_DIR)
    }
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::Paths;

    #[test]
    fn paths_overrides() {
        let paths = Paths::resolve(
            Some(PathBuf::from("./test.toml")),
            Some(PathBuf::from("/tmp/digger-data")),
        );
        let paths = paths.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(paths.config_file(), Path::new("./test.toml"));
        assert_eq!(paths.data_dir, Path::new("/tmp/digger-data"));
        assert_eq!(
            paths.scores_file(),
            Path::new("/tmp/digger-data").join("scores.toml")
        );
        assert_eq!(
            paths.recordings_dir(),
            Path::new("/tmp/digger-data").join("recordings")
        );
        // Each override applies on its own.
        if let Ok(paths) = Paths::resolve(Some(PathBuf::from("a.toml")), None) {
            assert_eq!(paths.config_file(), Path::new("a.toml"));
            assert_ne!(paths.data_dir, Path::new(""));
        }
        if let Ok(paths) = Paths::resolve(None, Some(PathBuf::from("d"))) {
            assert_eq!(paths.data_dir, Path::new("d"));
            assert!(paths.config_file().ends_with("config.toml"));
        }
    }

    /// The platform directory of `configuration` and `high-scores` specs.
    #[test]
    fn paths_platform_defaults() {
        let Ok(paths) = Paths::resolve(None, None) else {
            // No home directory in this environment: nothing to compare.
            return;
        };
        #[cfg(target_os = "linux")]
        {
            let home = std::env::var_os("HOME").map(PathBuf::from);
            let config_base = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| home.clone().map(|h| h.join(".config")));
            let data_base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| home.map(|h| h.join(".local").join("share")));
            if let Some(base) = config_base {
                assert_eq!(paths.config_file, base.join("digger").join("config.toml"));
            }
            if let Some(base) = data_base {
                assert_eq!(paths.data_dir, base.join("digger"));
            }
        }
        #[cfg(target_os = "windows")]
        {
            assert!(
                paths.config_file.ends_with(r"digger\config\config.toml"),
                "{}",
                paths.config_file.display()
            );
            assert!(paths.data_dir.ends_with(r"digger\data"));
            if let Some(appdata) = std::env::var_os("APPDATA") {
                assert!(paths.config_file.starts_with(&appdata));
                assert!(paths.data_dir.starts_with(&appdata));
            }
        }
        #[cfg(target_os = "macos")]
        {
            let base = std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join("Library").join("Application Support"));
            if let Some(base) = base {
                assert_eq!(paths.config_file, base.join("digger").join("config.toml"));
                assert_eq!(paths.data_dir, base.join("digger"));
            }
        }
        assert!(paths.config_file.ends_with("config.toml"));
    }
}
