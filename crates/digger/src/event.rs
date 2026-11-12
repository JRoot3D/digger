//! What the platform tells the application from outside a frame
//! (add-web-build-and-releases, design decision 4).
//!
//! Asynchronous results (a picked file, a visibility change, a failure
//! found late) arrive as one [`AppEvent`] through the event loop's user
//! events, and the application handles them as it handles input: there are
//! no callbacks reaching into game state.

use crate::store::{StoreError, StoreKey};
use crate::transfer::TransferError;

/// An event from the platform.
#[derive(Debug)]
pub(crate) enum AppEvent {
    /// The page became visible (`true`) or hidden (`false`).
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(dead_code, reason = "only a browser page is hidden")
    )]
    Visibility(bool),
    /// The player picked a recording to play back.
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(
            dead_code,
            reason = "the desktop plays recordings named on its command line"
        )
    )]
    RecordingPicked {
        /// The file's name, as a hint to its format.
        name: String,
        /// Its contents.
        bytes: Vec<u8>,
    },
    /// Handing a recording out, or reading a picked one, failed.
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(dead_code, reason = "the desktop reports a failed save at once")
    )]
    TransferFailed(TransferError),
    /// A save that the platform finished later failed. Both stores save
    /// at once and report through their result, so none sends it yet.
    #[allow(dead_code, reason = "for a store that finishes its saves later")]
    StorageFailed(StoreKey, StoreError),
}
