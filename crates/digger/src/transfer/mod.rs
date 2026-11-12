//! Recordings handed out of and into the program (add-web-build-and-
//! releases, design decisions 4 and 8, `FileTransfer` seam).
//!
//! The save-recording action exports the last game's recording under the
//! name the desktop gives it; the desktop's [`NativeFileTransfer`] writes
//! it into the recordings folder, the browser's hands it to the browser as
//! a download. A recording to play back comes in as an
//! [`AppEvent::RecordingPicked`](crate::event::AppEvent): on the desktop
//! from the command line, in the browser from the file picker that
//! [`FileTransfer::request_import`] opens.

use std::fmt;
use std::io;
use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
mod native;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::NativeFileTransfer;

/// Why a recording could not be handed out or taken in.
#[derive(Debug)]
pub(crate) enum TransferError {
    /// A file or folder could not be written.
    Io {
        /// The folder the recording was to be saved in.
        path: PathBuf,
        /// What went wrong.
        source: io::Error,
    },
    /// The browser refused (no download, the picked file unreadable).
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(dead_code, reason = "raised by the browser's transfer of task 3.7")
    )]
    Refused(String),
}

impl fmt::Display for TransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Refused(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for TransferError {}

/// Hands recordings to the player and asks for one to play.
pub(crate) trait FileTransfer: fmt::Debug {
    /// Hands out `bytes` as the file `name` (`ABC50.drf`).
    ///
    /// # Errors
    ///
    /// [`TransferError`]; the caller shows it as a notice.
    fn export(&mut self, name: &str, bytes: &[u8]) -> Result<(), TransferError>;

    /// Asks the player for a recording to play; the answer arrives later
    /// as an [`AppEvent`](crate::event::AppEvent). The desktop takes its
    /// recordings from the command line and does nothing here.
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(dead_code, reason = "the page's play-recording button (task 3.7)")
    )]
    fn request_import(&mut self) {}
}

/// The files a [`RecordingTransfer`] was given, by name, in order.
#[cfg(test)]
pub(crate) type Exported = std::rc::Rc<std::cell::RefCell<Vec<(String, Vec<u8>)>>>;

/// A transfer that keeps what it was given, for tests.
#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub(crate) struct RecordingTransfer {
    /// The exported files, by name, in order; shared between clones.
    pub(crate) exported: Exported,
    /// How many imports were asked for.
    pub(crate) imports: std::rc::Rc<std::cell::Cell<u32>>,
}

#[cfg(test)]
impl FileTransfer for RecordingTransfer {
    fn export(&mut self, name: &str, bytes: &[u8]) -> Result<(), TransferError> {
        self.exported
            .borrow_mut()
            .push((name.to_owned(), bytes.to_vec()));
        Ok(())
    }

    fn request_import(&mut self) {
        self.imports.set(self.imports.get() + 1);
    }
}
