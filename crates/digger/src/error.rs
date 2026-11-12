//! Errors that end the program, each with its exit status (design D12).

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the variants are built by the command line, configuration, window and verify code of later task groups"
    )
)]

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Exit status of a command-line usage error (`EX_USAGE`).
pub(crate) const EXIT_USAGE: u8 = 64;
/// Exit status of an unreadable input file (`EX_NOINPUT`).
pub(crate) const EXIT_NO_INPUT: u8 = 66;
/// Exit status when no display can be opened (`EX_UNAVAILABLE`).
pub(crate) const EXIT_UNAVAILABLE: u8 = 69;
/// Exit status of a bad configuration file (`EX_CONFIG`).
pub(crate) const EXIT_CONFIG: u8 = 78;

/// Why the program stops with a status other than 0.
#[derive(Debug)]
pub(crate) enum AppError {
    /// A command-line error: an unknown or refused option, a bad value or
    /// a forbidden combination. The message quotes the option as typed.
    /// Exit status 64.
    Usage(String),
    /// A level file that cannot be read or is too short. Exit status 66.
    LevelFile {
        /// The file as named on the command line.
        path: PathBuf,
        /// What went wrong.
        source: io::Error,
    },
    /// No window can be opened: no display server. Exit status 69.
    NoDisplay(String),
    /// A configuration file that cannot be used. Exit status 78.
    Config {
        /// The configuration file.
        file: PathBuf,
        /// The offending key path (`sound.sample_rate`), when there is one.
        key: Option<String>,
        /// What is wrong with it.
        problem: String,
    },
    /// A verify run that did not succeed, with the status of the core's
    /// replay report (1 unreadable, 2 failed, 3 stopped, 4 truncated).
    /// Only a recording that could not be read or parsed (1, or 2 for a
    /// bad record) carries a message; the outcomes of a playback print the
    /// report's own diagnostics.
    Verify {
        /// The status, 1 to 4.
        status: u8,
        /// The message for a recording that could not be read or parsed.
        message: Option<String>,
    },
}

impl AppError {
    /// The process exit status for this error: 64, 66, 69, 78, or the
    /// verify status 1 to 4 (a verify status outside 1–4 is reported as
    /// the nearest of them, so an error never exits with 0).
    pub(crate) const fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) => EXIT_USAGE,
            Self::LevelFile { .. } => EXIT_NO_INPUT,
            Self::NoDisplay(_) => EXIT_UNAVAILABLE,
            Self::Config { .. } => EXIT_CONFIG,
            Self::Verify { status, .. } => {
                if *status < 1 {
                    1
                } else if *status > 4 {
                    4
                } else {
                    *status
                }
            }
        }
    }

    /// Whether `main` prints a `digger: error:` line for this error. A
    /// verify run that played to a divergence, a stop or a truncation
    /// prints the replay's diagnostics instead.
    pub(crate) const fn has_message(&self) -> bool {
        match self {
            Self::Verify { message, .. } => message.is_some(),
            _ => true,
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) | Self::NoDisplay(message) => f.write_str(message),
            Self::LevelFile { path, source } => {
                write!(f, "cannot read level file {}: {source}", path.display())
            }
            Self::Config { file, key, problem } => match key {
                Some(key) => write!(f, "{}: {key}: {problem}", file.display()),
                None => write!(f, "{}: {problem}", file.display()),
            },
            Self::Verify { status, message } => match message {
                Some(message) => f.write_str(message),
                None => write!(f, "verification ended with status {status}"),
            },
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::LevelFile { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::PathBuf;

    use super::AppError;

    #[test]
    fn app_error_exit_codes() {
        assert_eq!(
            AppError::Usage("unknown option --turbo".into()).exit_code(),
            64
        );
        let level = AppError::LevelFile {
            path: PathBuf::from("levels.dlf"),
            source: io::Error::from(io::ErrorKind::NotFound),
        };
        assert_eq!(level.exit_code(), 66);
        assert_eq!(AppError::NoDisplay("no display".into()).exit_code(), 69);
        let config = AppError::Config {
            file: PathBuf::from("config.toml"),
            key: Some("sound.sample_rate".into()),
            problem: "999 is outside 8000–192000".into(),
        };
        assert_eq!(config.exit_code(), 78);
        for status in 1..=4 {
            let verify = AppError::Verify {
                status,
                message: None,
            };
            assert_eq!(verify.exit_code(), status);
        }
        // Never a success status, never above the verify range.
        let zero = AppError::Verify {
            status: 0,
            message: None,
        };
        assert_eq!(zero.exit_code(), 1);
        let high = AppError::Verify {
            status: 9,
            message: None,
        };
        assert_eq!(high.exit_code(), 4);
    }

    #[test]
    fn app_error_messages() {
        let config = AppError::Config {
            file: PathBuf::from("config.toml"),
            key: Some("sound.sample_rate".into()),
            problem: "999 is outside 8000–192000".into(),
        };
        assert_eq!(
            config.to_string(),
            "config.toml: sound.sample_rate: 999 is outside 8000–192000"
        );
        let level = AppError::LevelFile {
            path: PathBuf::from("x.dlf"),
            source: io::Error::from(io::ErrorKind::NotFound),
        };
        assert!(
            level
                .to_string()
                .starts_with("cannot read level file x.dlf")
        );
        assert!(std::error::Error::source(&level).is_some());
        let unreadable = AppError::Verify {
            status: 1,
            message: Some("cannot read recording a.drf".into()),
        };
        assert!(unreadable.has_message());
        let failed = AppError::Verify {
            status: 2,
            message: None,
        };
        assert!(!failed.has_message());
    }
}
