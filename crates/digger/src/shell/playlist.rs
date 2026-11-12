//! The `--play` list: the recordings shown before the title screen, in the
//! order given, repeated with `--loop` (`configuration` spec, play
//! options; design D11).
//!
//! Each file is read and parsed the first time the list reaches it, and
//! kept for later laps of a loop. A file that cannot be read, or is not a
//! recording, is reported once on standard error as a notice and skipped
//! from then on. A lap that finds nothing to play ends the list, so a loop
//! over unreadable files never spins.

use std::fs;
use std::path::{Path, PathBuf};

use digger_core::recording::{self, Recording};

use crate::report;

/// Where one file of the list stands.
#[derive(Debug, Clone)]
enum State {
    /// Not reached yet.
    Unread,
    /// Read and parsed.
    Ready(Box<Recording>),
    /// Reported and skipped.
    Bad,
}

/// One file of the list.
#[derive(Debug, Clone)]
struct Item {
    path: PathBuf,
    state: State,
}

/// The recordings to play before the title screen.
#[derive(Debug, Clone, Default)]
pub(crate) struct PlayList {
    items: Vec<Item>,
    /// The index of the item to try next.
    next: usize,
    /// `--loop`: start over after the last item.
    looping: bool,
    /// The list was stopped (by the quit action): nothing more is played.
    stopped: bool,
    /// The notices given for skipped files, in order.
    notices: Vec<String>,
}

impl PlayList {
    /// The list of `files`, repeated if `looping`.
    pub(crate) fn new(files: &[PathBuf], looping: bool) -> Self {
        Self {
            items: files
                .iter()
                .map(|path| Item {
                    path: path.clone(),
                    state: State::Unread,
                })
                .collect(),
            next: 0,
            looping,
            stopped: false,
            notices: Vec::new(),
        }
    }

    /// Moves on to the next playable recording and returns its file and
    /// the recording, or `None` when the list is over: its end reached
    /// without `--loop`, stopped, or a whole lap without a playable file.
    pub(crate) fn advance(&mut self) -> Option<(&Path, &Recording)> {
        let len = self.items.len();
        let mut tried = 0;
        let index = loop {
            if self.stopped || tried >= len {
                return None;
            }
            if self.next >= len {
                if !self.looping {
                    return None;
                }
                self.next = 0;
            }
            let index = self.next;
            self.next += 1;
            tried += 1;
            if self.load(index) {
                break index;
            }
        };
        let item = self.items.get(index)?;
        match &item.state {
            State::Ready(recording) => Some((&item.path, recording)),
            State::Unread | State::Bad => None,
        }
    }

    /// Reads item `index` if it was not read yet; whether it is playable.
    fn load(&mut self, index: usize) -> bool {
        let Some(item) = self.items.get_mut(index) else {
            return false;
        };
        if let State::Unread = item.state {
            item.state = match read(&item.path) {
                Ok(recording) => State::Ready(Box::new(recording)),
                Err(message) => {
                    report::notice(&message);
                    self.notices.push(message);
                    State::Bad
                }
            };
        }
        matches!(item.state, State::Ready(_))
    }

    /// Stops the list: nothing more is played, the loop included.
    pub(crate) const fn stop(&mut self) {
        self.stopped = true;
    }

    /// The notices given for skipped files so far.
    pub(crate) fn notices(&self) -> &[String] {
        &self.notices
    }
}

/// Reads and parses the recording at `path`, or the notice that skips it.
fn read(path: &Path) -> Result<Recording, String> {
    let bytes = fs::read(path).map_err(|error| skip_notice(path, &error))?;
    recording::parse(&bytes).map_err(|error| skip_notice(path, &error))
}

/// The notice for a `--play` file that is skipped.
fn skip_notice(path: &Path, error: &dyn std::fmt::Display) -> String {
    format!("cannot play back {}: {error}; skipped", path.display())
}
