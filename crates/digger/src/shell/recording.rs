//! When and where recordings are saved, and their automatic names
//! (design D11).
//!
//! A live game is recorded from its first frame by the core's recorder,
//! which the play scene owns. When the game ends, however it ends, the
//! complete text, with its end-of-game marker, becomes the shell's last
//! game until it is saved.
//!
//! With `--record` the recording is written to that file as soon as the
//! game ends, replacing what the file held, in the format its extension
//! selects. Otherwise the save-recording action on the title screen writes
//! the last game, once, as `<initials><score>.drf` through the platform's
//! file transfer: on the desktop into the `recordings` folder of the data
//! directory, with `-2`, `-3`, … before the extension when the name is
//! taken (`configuration` spec, record option; design D11).

use std::fs;
use std::io;
use std::path::Path;

use digger_core::GameSummary;
use digger_core::recording::RecordingKind;
use digger_core::replay::Recorder;

use crate::report;
use crate::scores::Entry;

/// The initials of an automatic name when none were entered.
const NO_INITIALS: &str = "___";

/// The recording format a `--record` file name selects: eDRF for a name
/// ending in `.edrf` in any letter case, DRF for any other.
pub(crate) fn kind_for_path(path: &Path) -> RecordingKind {
    match path.extension() {
        Some(ext) if ext.eq_ignore_ascii_case("edrf") => RecordingKind::Edrf,
        _ => RecordingKind::Drf,
    }
}

/// The recording of a game that has ended (design D11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinishedRecording {
    /// The format of `text`.
    pub(crate) kind: RecordingKind,
    /// The complete recording.
    pub(crate) text: String,
    /// How the game ended.
    pub(crate) summary: GameSummary,
    /// The last high-score entry made in the game, if any.
    pub(crate) entry: Option<Entry>,
}

impl FinishedRecording {
    /// The automatic file name without directory: the initials just
    /// entered (`___` without an entry) followed by the score that went
    /// with them (without an entry, the first Digger's six-digit score)
    /// and `.drf` (design D11; upstream's 8.3 squeezing is dropped).
    pub(crate) fn auto_stem(&self) -> String {
        if let Some(entry) = self.entry {
            return format!("{}{}", entry.initials_text(), entry.score);
        }
        let score = self
            .summary
            .scores
            .first()
            .map_or(0, |score| score.visible.max(0));
        format!("{NO_INITIALS}{score}")
    }

    /// Writes the recording to `path`, replacing the file.
    pub(crate) fn write_to(&self, path: &Path) -> io::Result<()> {
        fs::write(path, &self.text)
    }

    /// The automatic file name: [`FinishedRecording::auto_stem`] and
    /// `.drf`.
    pub(crate) fn auto_name(&self) -> String {
        format!("{}.drf", self.auto_stem())
    }
}

/// Writes `recording` to the `--record` file `path`, reporting a failure
/// as a notice.
pub(crate) fn save_record_file(recording: &FinishedRecording, path: &Path) {
    if let Err(error) = recording.write_to(path) {
        report::notice(format!("{}: recording not saved: {error}", path.display()));
    }
}

/// The text of a live game's recording as it is written.
///
/// An eDRF recorder hands out its lines as they are finished, so they are
/// collected frame by frame; a DRF is written whole, so its text is taken
/// once, at the end.
#[derive(Debug, Clone)]
pub(crate) struct LiveRecording {
    /// The core's recorder, observing the game.
    recorder: Recorder,
    /// The eDRF lines taken so far.
    text: String,
    /// The last high-score entry made in the game.
    entry: Option<Entry>,
}

impl LiveRecording {
    /// Records with `recorder` from the game's first frame.
    pub(crate) const fn new(recorder: Recorder) -> Self {
        Self {
            recorder,
            text: String::new(),
            entry: None,
        }
    }

    /// The recorder, to observe the game's steps.
    pub(crate) const fn recorder_mut(&mut self) -> &mut Recorder {
        &mut self.recorder
    }

    /// Collects what the recorder finished during the last step.
    pub(crate) fn collect(&mut self) {
        if self.recorder.kind() == RecordingKind::Edrf {
            self.text.push_str(&self.recorder.take_pending());
        }
    }

    /// Hands the initials of a high-score `entry` to the recording: a
    /// classic recording gets the line `*ABC`; an extended recording has
    /// no initials line (`game-recording` spec).
    pub(crate) fn add_initials(&mut self, entry: Entry) {
        if let Recorder::Drf(drf) = &mut self.recorder {
            drf.add_initials(&entry.initials_text());
        }
        self.entry = Some(entry);
    }

    /// The complete recording of the game that ended as `summary`.
    pub(crate) fn finish(mut self, summary: GameSummary) -> FinishedRecording {
        let kind = self.recorder.kind();
        let text = match kind {
            RecordingKind::Edrf => {
                self.collect();
                self.text
            }
            RecordingKind::Drf => self.recorder.take_pending(),
        };
        FinishedRecording {
            kind,
            text,
            summary,
            entry: self.entry,
        }
    }
}
