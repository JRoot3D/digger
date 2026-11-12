//! Conditions the game carries on through, as values (add-web-build-and-
//! releases, design decision 16): the shell raises them, and the
//! platform's page interface shows them, as a `digger: notice:` line on
//! the desktop or a page element in the browser.

use std::fmt;

use crate::store::{StoreError, StoreKey};
use crate::transfer::TransferError;

/// A condition the player is told about while the game carries on.
#[derive(Debug)]
pub(crate) enum Notice {
    /// A save of the settings or the high scores failed: the values stay
    /// in memory for the rest of the run (raised once per run).
    StorageUnavailable {
        /// What could not be saved.
        key: StoreKey,
        /// Why.
        error: StoreError,
    },
    /// The stored settings have become unusable since the program started;
    /// they are left as they are and the changes are not saved.
    SettingsNotSaved(String),
    /// The last game's recording could not be handed out.
    RecordingNotSaved(TransferError),
    /// A picked recording cannot be played back.
    RecordingRejected {
        /// The file's name.
        name: String,
        /// Why.
        reason: Rejection,
    },
}

/// Why a picked recording is not played back (`web-build` spec, loading a
/// recording through a file picker): each reason has its own notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Rejection {
    /// The file is larger than [`MAX_IMPORT_BYTES`](crate::transfer::MAX_IMPORT_BYTES).
    TooLarge {
        /// Its size in bytes.
        size: u64,
    },
    /// It does not begin with a recording header.
    NotARecording,
    /// A recording of a kind the game does not play (pre-1998).
    Unsupported(String),
    /// A recording whose header or body is damaged.
    Malformed(String),
    /// The file could not be read.
    Unreadable(String),
    /// Recordings are picked on the title screen only.
    NotOnTitle,
}

impl Rejection {
    /// The reason a recording that does not parse is rejected.
    pub(crate) fn of(error: &digger_core::recording::RecordingError) -> Self {
        use digger_core::recording::RecordingError;
        match error {
            RecordingError::NotARecording => Self::NotARecording,
            RecordingError::UnsupportedVersion { .. } => Self::Unsupported(error.to_string()),
            // A header, record or body that cannot be read (the error
            // type is open to new kinds of damage).
            _ => Self::Malformed(error.to_string()),
        }
    }
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { size } => write!(
                f,
                "the file is too large ({size} bytes; recordings are at most 16 MiB)"
            ),
            Self::NotARecording => f.write_str(
                "the file is not a recording (it does not begin with a DRF or eDRF header)",
            ),
            Self::Unsupported(reason) => write!(f, "unsupported recording: {reason}"),
            Self::Malformed(reason) => write!(f, "the recording is damaged: {reason}"),
            Self::Unreadable(reason) => write!(f, "the file cannot be read: {reason}"),
            Self::NotOnTitle => f.write_str("recordings are played from the title screen"),
        }
    }
}

impl fmt::Display for Notice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StorageUnavailable {
                key: StoreKey::Settings,
                error,
            } => write!(f, "settings not saved: {error}"),
            Self::StorageUnavailable {
                key: StoreKey::Scores,
                error,
            } => write!(
                f,
                "high scores not saved: {error}; they are kept until the program ends"
            ),
            Self::SettingsNotSaved(reason) => write!(f, "settings not saved: {reason}"),
            Self::RecordingNotSaved(TransferError::Io { path, source }) => {
                write!(f, "{}: recording not saved: {source}", path.display())
            }
            Self::RecordingNotSaved(error) => write!(f, "recording not saved: {error}"),
            Self::RecordingRejected { name, reason } => {
                write!(f, "cannot play back {name}: {reason}")
            }
        }
    }
}
