//! Where settings and high scores are kept (add-web-build-and-releases,
//! design decision 4, `Store` seam).
//!
//! The program keeps two texts: the settings file's and the score file's
//! (`config.toml` and `scores.toml` as the desktop writes them). A
//! [`Store`] loads and saves them by [`StoreKey`]; the desktop's
//! [`NativeStore`] keeps them as files in the configuration and data
//! directories, the browser's keeps them in local storage, and
//! [`MemoryStore`] keeps them in memory (tests, and a frontend without any
//! storage). The serialisation is shared: every store holds the same text.
//!
//! [`Storage`] is what the shell owns: a store and the rule that a failed
//! save raises one [`Notice::StorageUnavailable`] while play carries on
//! with the values in memory (`web-build` spec, play without usable
//! storage).

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::notice::Notice;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod native;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::NativeStore;

/// What is stored. A closed set, so no caller can invent a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum StoreKey {
    /// The settings the player changed (the configuration file's text).
    Settings,
    /// The high-score tables (the score file's text).
    Scores,
}

/// Why a store could not load or save.
#[derive(Debug)]
pub(crate) enum StoreError {
    /// The storage cannot be used at all (the browser denies it).
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(dead_code, reason = "raised by the browser's store of task 3.3")
    )]
    Unavailable(String),
    /// The storage is full.
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(dead_code, reason = "raised by the browser's store of task 3.3")
    )]
    Full,
    /// A file could not be read or written.
    Io {
        /// The path that could not be read, written or renamed.
        path: PathBuf,
        /// What went wrong.
        source: io::Error,
    },
    /// The stored value is not UTF-8 text.
    NotText,
    /// A test store refuses every save.
    #[cfg(test)]
    Refused,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(reason) => write!(f, "storage is unavailable: {reason}"),
            Self::Full => f.write_str("storage is full"),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::NotText => f.write_str("is not UTF-8 text"),
            #[cfg(test)]
            Self::Refused => f.write_str("the test store refuses saves"),
        }
    }
}

impl std::error::Error for StoreError {}

/// A place that keeps the settings and score texts.
pub(crate) trait Store: fmt::Debug {
    /// The text kept under `key`; `None` when nothing is kept there yet.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when it is kept but cannot be read.
    fn load(&self, key: StoreKey) -> Result<Option<String>, StoreError>;

    /// Keeps `text` under `key`, replacing what was there, so that an
    /// interrupted save leaves the old or the new text whole.
    ///
    /// # Errors
    ///
    /// [`StoreError`]; what was kept before stays.
    fn save(&mut self, key: StoreKey, text: &str) -> Result<(), StoreError>;

    /// Keeps the damaged value under `key` aside before it is first
    /// replaced (the desktop renames `scores.toml` to `scores.toml.bad`);
    /// by default the damaged value is simply replaced.
    ///
    /// # Errors
    ///
    /// [`StoreError`] when it cannot be kept aside.
    fn set_aside(&mut self, key: StoreKey) -> Result<(), StoreError> {
        let _ = key;
        Ok(())
    }

    /// Where the value of `key` is kept, for messages: a file path or a
    /// storage key.
    fn location(&self, key: StoreKey) -> String;

    /// Where [`Store::set_aside`] keeps a damaged value, for messages.
    fn aside_location(&self, key: StoreKey) -> Option<String> {
        let _ = key;
        None
    }
}

/// A store in memory: the values live as long as the store.
#[derive(Debug, Default)]
pub(crate) struct MemoryStore {
    values: BTreeMap<StoreKey, String>,
    /// Every save fails (tests of the failure path).
    #[cfg(test)]
    refuse_saves: bool,
}

impl MemoryStore {
    /// An empty store.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A store holding `text` under `key`.
    #[cfg(test)]
    pub(crate) fn with(mut self, key: StoreKey, text: &str) -> Self {
        self.values.insert(key, text.to_owned());
        self
    }

    /// A store whose every save fails with [`StoreError::Refused`].
    #[cfg(test)]
    pub(crate) fn refusing() -> Self {
        Self {
            refuse_saves: true,
            ..Self::default()
        }
    }
}

impl Store for MemoryStore {
    fn load(&self, key: StoreKey) -> Result<Option<String>, StoreError> {
        Ok(self.values.get(&key).cloned())
    }

    fn save(&mut self, key: StoreKey, text: &str) -> Result<(), StoreError> {
        #[cfg(test)]
        if self.refuse_saves {
            return Err(StoreError::Refused);
        }
        self.values.insert(key, text.to_owned());
        Ok(())
    }

    fn location(&self, key: StoreKey) -> String {
        match key {
            StoreKey::Settings => String::from("settings in memory"),
            StoreKey::Scores => String::from("high scores in memory"),
        }
    }
}

/// The shell's store and the rule for its failures (`web-build` spec, play
/// without usable storage): the first failed save raises one
/// [`Notice::StorageUnavailable`]; later failures raise none, and the
/// values stay in memory for the rest of the run.
#[derive(Debug)]
pub(crate) struct Storage {
    store: Box<dyn Store>,
    /// A failure was reported already.
    failed: bool,
    /// Every failed save raises its notice, not only the first (the
    /// desktop, whose `high-scores` spec prints a notice for each score
    /// file that cannot be written).
    every_failure: bool,
    /// The notices raised since they were last taken.
    notices: Vec<Notice>,
}

impl Storage {
    /// The rule around `store`.
    pub(crate) fn new(store: impl Store + 'static) -> Self {
        Self {
            store: Box::new(store),
            failed: false,
            every_failure: false,
            notices: Vec::new(),
        }
    }

    /// The desktop's rule around `store`: as [`Storage::new`], except
    /// that every failed save raises its notice, naming the file and the
    /// cause, as phase 6 printed it.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn reporting_every_failure(store: impl Store + 'static) -> Self {
        Self {
            every_failure: true,
            ..Self::new(store)
        }
    }

    /// An empty store in memory.
    pub(crate) fn memory() -> Self {
        Self::new(MemoryStore::new())
    }

    /// The store, to load from.
    pub(crate) fn store(&self) -> &dyn Store {
        self.store.as_ref()
    }

    /// The store, to save to directly; a failure is then handed to
    /// [`Storage::failed`].
    pub(crate) fn store_mut(&mut self) -> &mut dyn Store {
        self.store.as_mut()
    }

    /// Saves `text` under `key`; a failure is reported by
    /// [`Storage::failed`]. Returns whether the save succeeded.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the shell saves through store_mut")
    )]
    pub(crate) fn save(&mut self, key: StoreKey, text: &str) -> bool {
        match self.store.save(key, text) {
            Ok(()) => true,
            Err(error) => {
                self.failed(key, error);
                false
            }
        }
    }

    /// A save of `key` failed with `error`: the first failure raises the
    /// notice, later ones are dropped unless every failure is reported.
    pub(crate) fn failed(&mut self, key: StoreKey, error: StoreError) {
        if !std::mem::replace(&mut self.failed, true) || self.every_failure {
            self.notices.push(Notice::StorageUnavailable { key, error });
        }
    }

    /// Raises another kind of notice about the stored values (stored
    /// settings that became unusable).
    pub(crate) fn notice(&mut self, notice: Notice) {
        self.notices.push(notice);
    }

    /// The notices raised since the last call.
    pub(crate) fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation is a failed test"
)]
mod tests {
    use super::{MemoryStore, Storage, Store, StoreError, StoreKey};
    use crate::notice::Notice;

    #[test]
    fn store_missing_returns_none() {
        let store = MemoryStore::new();
        assert!(matches!(store.load(StoreKey::Settings), Ok(None)));
        assert!(matches!(store.load(StoreKey::Scores), Ok(None)));
        let store = MemoryStore::new().with(StoreKey::Scores, "format = 1\n");
        assert!(matches!(store.load(StoreKey::Settings), Ok(None)));
    }

    #[test]
    fn storage_memory_keeps_saved_values() {
        let mut storage = Storage::memory();
        assert!(storage.save(StoreKey::Settings, "a"));
        assert!(storage.save(StoreKey::Scores, "b"));
        assert!(storage.save(StoreKey::Settings, "c"));
        assert_eq!(
            storage
                .store()
                .load(StoreKey::Settings)
                .ok()
                .flatten()
                .as_deref(),
            Some("c")
        );
        assert_eq!(
            storage
                .store()
                .load(StoreKey::Scores)
                .ok()
                .flatten()
                .as_deref(),
            Some("b")
        );
        assert!(storage.take_notices().is_empty());
    }

    #[test]
    fn storage_failure_notice_names_the_cause() {
        let mut storage = Storage::new(MemoryStore::refusing());
        assert!(!storage.save(StoreKey::Scores, "x"));
        let notices = storage.take_notices();
        let [Notice::StorageUnavailable { key, error }] = notices.as_slice() else {
            panic!("{notices:?}");
        };
        assert_eq!(*key, StoreKey::Scores);
        assert!(matches!(error, StoreError::Refused));
        assert_eq!(
            notices[0].to_string(),
            "high scores not saved: the test store refuses saves; they are kept until the \
             program ends"
        );
    }

    #[test]
    fn storage_desktop_reports_every_failure() {
        let mut storage = Storage::reporting_every_failure(MemoryStore::refusing());
        assert!(!storage.save(StoreKey::Scores, "x"));
        assert!(!storage.save(StoreKey::Scores, "y"));
        assert!(!storage.save(StoreKey::Settings, "z"));
        assert_eq!(storage.take_notices().len(), 3);
        let mut storage = Storage::new(MemoryStore::refusing());
        assert!(!storage.save(StoreKey::Scores, "x"));
        assert!(!storage.save(StoreKey::Settings, "z"));
        assert_eq!(storage.take_notices().len(), 1);
    }
}
