//! The browser's store: the settings and high-score texts in the page's
//! local storage under `digger:settings` and `digger:scores` (`web-build`
//! spec, settings and high scores persist in local storage). No other key
//! is ever used.
//!
//! Local storage can be missing or refused (a browser set to deny site
//! storage throws a `SecurityError` on access) and can be full (a
//! `QuotaExceededError` on write); both are [`StoreError`]s, so the shell's
//! rule applies: one notice, and play carries on with the values in memory.

use wasm_bindgen::JsValue;

use super::{Store, StoreError, StoreKey};

/// The local-storage key of `key`.
pub(crate) const fn storage_key(key: StoreKey) -> &'static str {
    match key {
        StoreKey::Settings => "digger:settings",
        StoreKey::Scores => "digger:scores",
    }
}

/// The page's local storage, or why there is none.
#[derive(Debug)]
pub(crate) struct WebStore {
    storage: Result<web_sys::Storage, String>,
}

impl WebStore {
    /// The local storage of the page's window.
    pub(crate) fn open() -> Self {
        let storage = match web_sys::window().map(|window| window.local_storage()) {
            None => Err(String::from("there is no browser window")),
            Some(Ok(Some(storage))) => Ok(storage),
            Some(Ok(None)) => Err(String::from("the browser offers no local storage")),
            Some(Err(error)) => Err(describe(&error)),
        };
        Self { storage }
    }

    /// Why local storage cannot be used, if it cannot. The reason also
    /// says what follows for the player, since the one storage notice of
    /// the page is raised from it (`web-build` spec, play without usable
    /// storage: the notice says settings and scores will not be kept).
    pub(crate) fn unavailable(&self) -> Option<StoreError> {
        self.storage.as_ref().err().map(|reason| {
            StoreError::Unavailable(format!(
                "{reason}; settings and high scores are kept only until the page is closed"
            ))
        })
    }
}

/// The name and message of a thrown browser error, as text.
pub(crate) fn describe(error: &JsValue) -> String {
    let field = |name: &str| {
        js_sys::Reflect::get(error, &JsValue::from_str(name))
            .ok()
            .and_then(|value| value.as_string())
            .filter(|text| !text.is_empty())
    };
    match (field("name"), field("message")) {
        (Some(name), Some(message)) => format!("{name}: {message}"),
        (Some(text), None) | (None, Some(text)) => text,
        (None, None) => error
            .as_string()
            .unwrap_or_else(|| String::from("the browser refused")),
    }
}

/// Whether a thrown error says the storage is full.
fn is_quota(error: &JsValue) -> bool {
    js_sys::Reflect::get(error, &JsValue::from_str("name"))
        .ok()
        .and_then(|name| name.as_string())
        .is_some_and(|name| name == "QuotaExceededError" || name == "NS_ERROR_DOM_QUOTA_REACHED")
}

impl Store for WebStore {
    fn load(&self, key: StoreKey) -> Result<Option<String>, StoreError> {
        let storage = self
            .storage
            .as_ref()
            .map_err(|reason| StoreError::Unavailable(reason.clone()))?;
        storage
            .get_item(storage_key(key))
            .map_err(|error| StoreError::Unavailable(describe(&error)))
    }

    fn save(&mut self, key: StoreKey, text: &str) -> Result<(), StoreError> {
        let storage = self
            .storage
            .as_ref()
            .map_err(|reason| StoreError::Unavailable(reason.clone()))?;
        // `setItem` replaces the value whole or throws and keeps the old.
        storage.set_item(storage_key(key), text).map_err(|error| {
            if is_quota(&error) {
                StoreError::Full
            } else {
                StoreError::Unavailable(describe(&error))
            }
        })
    }

    fn location(&self, key: StoreKey) -> String {
        storage_key(key).to_owned()
    }
}
