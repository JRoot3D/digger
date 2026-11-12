//! Conditions the game carries on through, as values (add-web-build-and-
//! releases, design decision 16): the shell raises them, and the
//! platform's page interface shows them, as a `digger: notice:` line on
//! the desktop or a page element in the browser.

use std::fmt;

use crate::store::{StoreError, StoreKey};

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
        }
    }
}
