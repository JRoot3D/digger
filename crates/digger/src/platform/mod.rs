//! The platform layer: the only modules that use `winit`, `softbuffer`,
//! `cpal` and `gilrs` (design D1 of `add-desktop-app`). Nothing outside
//! this module imports them; the test `platform_imports_are_confined`
//! checks it.
//!
//! The layer has one implementation per kind of target, selected here by
//! `cfg` (add-web-build-and-releases, design decision 4): [`native`] for
//! the desktop systems, `web` for `wasm32-unknown-unknown`. Both offer the
//! same names: an `App` that owns the shell and the devices, and
//! `run(app)`, which drives it. What both share (the key-name table and the
//! error of a failed start) lives here.

use std::fmt;

use crate::error::AppError;

mod audio;
mod cpal_output;
mod keys;
mod pads;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::{App, Finished, WindowOptions, run};
#[cfg(target_arch = "wasm32")]
pub(crate) use web::{App, WindowOptions, install_panic_hook, launch_options, run};

/// Why the platform could not run the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunError {
    /// No display can be reached, or the window cannot be opened (exit
    /// status 69 on the desktop).
    NoDisplay(String),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDisplay(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for RunError {}

impl From<RunError> for AppError {
    fn from(error: RunError) -> Self {
        match error {
            RunError::NoDisplay(reason) => Self::NoDisplay(reason),
        }
    }
}

/// The cause of a winit error as a sentence for the user: winit prefixes
/// operating-system errors with the source location they were raised at
/// (`os error at src/…/mod.rs:765: `), which is dropped here.
fn cause(error: &impl fmt::Display) -> String {
    let text = error.to_string();
    match text.strip_prefix("os error at ") {
        Some(rest) => rest
            .split_once(": ")
            .map_or(rest, |(_, cause)| cause)
            .to_owned(),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::{RunError, cause};
    use crate::error::AppError;

    #[test]
    fn winit_source_locations_are_dropped() {
        assert_eq!(
            cause(&"os error at /src/linux/mod.rs:765: DISPLAY is not set."),
            "DISPLAY is not set."
        );
        assert_eq!(
            cause(&"the event loop was exited"),
            "the event loop was exited"
        );
    }

    #[test]
    fn run_error_is_status_69() {
        let error = AppError::from(RunError::NoDisplay(String::from("no display")));
        assert_eq!(error.exit_code(), 69);
    }
}
