//! When and where recordings are saved, and their automatic names
//! (design D11).
//!
//! A live game is recorded from its first frame by the core's recorder,
//! which the play scene owns. When the game ends, however it ends, the
//! complete text, with its end-of-game marker, becomes the shell's last
//! game until it is saved.

use digger_core::GameSummary;
use digger_core::recording::RecordingKind;
use digger_core::replay::Recorder;

/// The recording of a game that has ended (design D11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinishedRecording {
    /// The format of `text`.
    pub(crate) kind: RecordingKind,
    /// The complete recording.
    pub(crate) text: String,
    /// How the game ended.
    pub(crate) summary: GameSummary,
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
}

impl LiveRecording {
    /// Records with `recorder` from the game's first frame.
    pub(crate) const fn new(recorder: Recorder) -> Self {
        Self {
            recorder,
            text: String::new(),
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
        }
    }
}
