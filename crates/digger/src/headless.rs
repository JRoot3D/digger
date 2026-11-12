//! Verification runs: reading the `--verify` recording, turning the core's
//! replay report into the program's exit status and result line, and the
//! `--verify --headless` runner (design D1, D11; `configuration` spec,
//! play-and-verify, verification result line, verifying without a
//! window).
//!
//! The statuses are the core's (`ReplayReport::exit_status`,
//! `RecordingError::exit_status`): 0 verified or completed, 1 a file that
//! cannot be opened or is not a recording, 2 a divergence or a bad record,
//! 3 stopped before the end, 4 an extended recording without its end
//! marker. Only status 0 prints the result line on standard output; every
//! other status explains itself on standard error.
//!
//! The headless runner opens no window and no audio device and never
//! waits: it plays the recording frame after frame as fast as the machine
//! goes. With `--record` the replayed game is re-recorded and written to
//! that file before the run ends, whatever its status (as upstream's
//! `/R:name /E:file`), which converts a classic recording to the extended
//! format.

use std::fs;
use std::path::{Path, PathBuf};

use digger_core::recording::{self, Recording, RecordingError};
use digger_core::replay::{Replay, ReplayOptions, ReplayReport, ReplayStep};
use digger_core::{FrameInput, NoopObserver};

use crate::error::AppError;
use crate::report;
use crate::shell::recording::kind_for_path;

/// A recording to verify, read and parsed, with the file it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifyJob {
    /// The file as named on the command line.
    pub(crate) file: PathBuf,
    /// The recording.
    pub(crate) recording: Recording,
}

impl VerifyJob {
    /// Reads and parses the recording at `file`.
    ///
    /// # Errors
    ///
    /// [`AppError::Verify`] with status 1 when the file cannot be read or
    /// is not a recording, 2 when it has a bad record; the message names
    /// the file.
    pub(crate) fn load(file: &Path) -> Result<Self, AppError> {
        let bytes = fs::read(file).map_err(|error| AppError::Verify {
            status: digger_core::replay::STATUS_UNREADABLE,
            message: Some(format!("cannot play back {}: {error}", file.display())),
        })?;
        let recording = recording::parse(&bytes).map_err(|error| unplayable(file, &error))?;
        Ok(Self {
            file: file.to_path_buf(),
            recording,
        })
    }
}

/// The error of a recording that could not be parsed: status 1, or 2 for
/// a bad record, with a message naming the file.
pub(crate) fn unplayable(file: &Path, error: &RecordingError) -> AppError {
    AppError::Verify {
        status: error.exit_status(),
        message: Some(format!("cannot play back {}: {error}", file.display())),
    }
}

/// What a finished verification run ends the program with: for status 0
/// the result line to print on standard output (with its line end),
/// otherwise the error carrying the status. The report's diagnostics are
/// printed on standard error here.
///
/// # Errors
///
/// [`AppError::Verify`] with the report's status 2, 3 or 4, without a
/// message.
pub(crate) fn conclude(report: &ReplayReport, file: &Path) -> Result<String, AppError> {
    match report.exit_status() {
        0 => Ok(report.result_line()),
        status => {
            let diagnostics = report.diagnostics(&file.display().to_string());
            for line in diagnostics.lines() {
                report::emit_raw(line);
            }
            Err(AppError::Verify {
                status,
                message: None,
            })
        }
    }
}

/// The `--verify <file> --headless` run, re-recording to `record` if
/// given: returns the result line of a successful run.
///
/// # Errors
///
/// [`AppError::Verify`] with status 1 to 4.
pub(crate) fn run(file: &Path, record: Option<&Path>) -> Result<String, AppError> {
    let job = VerifyJob::load(file)?;
    let report = play(&job, record)?;
    conclude(&report, file)
}

/// Plays `job` to its end with no devices and no pacing, re-recording to
/// `record` if given, and returns the report.
fn play(job: &VerifyJob, record: Option<&Path>) -> Result<ReplayReport, AppError> {
    let options = ReplayOptions::default();
    let mut replay = match record {
        Some(path) => Replay::recording_as(&job.recording, options, kind_for_path(path)),
        None => Replay::new(&job.recording, options),
    }
    .map_err(|error| unplayable(&job.file, &error))?;
    while let ReplayStep::Running(_) = replay.step(&FrameInput::NONE, &mut NoopObserver) {}
    let (report, text) = replay.finish_with_recording();
    if let (Some(path), Some(text)) = (record, text)
        && let Err(error) = fs::write(path, text)
    {
        report::notice(format!("{}: recording not saved: {error}", path.display()));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use digger_core::recording::{EndRecord, RecordingError, RecordingKind};
    use digger_core::replay::{Divergence, Failure, Outcome, ReplayReport};

    use super::{conclude, unplayable};
    use crate::error::AppError;

    fn report(outcome: Outcome, kind: RecordingKind) -> ReplayReport {
        ReplayReport {
            kind,
            outcome,
            scores: [3100, 0],
            two_scores: false,
            level: 2,
            ticks: 1378,
            last_good_checkpoint: None,
        }
    }

    /// The status of a run ending as `report`, and its result line if 0.
    fn status(report: &ReplayReport) -> (u8, Option<String>) {
        match conclude(report, Path::new("game.edrf")) {
            Ok(line) => (0, Some(line)),
            Err(error) => {
                assert!(!error.has_message(), "the diagnostics explain it");
                (error.exit_code(), None)
            }
        }
    }

    #[test]
    fn verify_status_mapping() {
        let failed = Failure::Divergence(Divergence::Tick {
            tick: 256,
            expected: 1,
            actual: 2,
        });
        let cases = [
            (Outcome::Verified, RecordingKind::Edrf, 0),
            (Outcome::Completed, RecordingKind::Drf, 0),
            (Outcome::TakenOver, RecordingKind::Drf, 0),
            (Outcome::Failed(failed), RecordingKind::Edrf, 2),
            (Outcome::Stopped, RecordingKind::Drf, 3),
            (Outcome::Truncated, RecordingKind::Edrf, 4),
        ];
        for (outcome, kind, expected) in cases {
            let report = report(outcome.clone(), kind);
            let (code, line) = status(&report);
            assert_eq!(code, expected, "{outcome:?}");
            assert_eq!(line.is_some(), expected == 0, "{outcome:?}");
        }
        // A file that is not a recording is 1; a bad record is 2; both
        // name the file.
        let not = unplayable(Path::new("notes.txt"), &RecordingError::NotARecording);
        assert_eq!(not.exit_code(), 1);
        assert!(not.to_string().contains("notes.txt"), "{not}");
        let bad = unplayable(
            Path::new("x.edrf"),
            &RecordingError::BadRecord {
                line: 7,
                text: "Y".into(),
            },
        );
        assert_eq!(bad.exit_code(), 2);
        assert!(bad.has_message());
        let missing = super::VerifyJob::load(Path::new("/nonexistent/digger/none.drf"));
        assert!(
            matches!(missing, Err(AppError::Verify { status: 1, .. })),
            "{missing:?}"
        );
        // The end-record divergence fails as well.
        let end = Failure::Divergence(Divergence::End {
            expected: EndRecord { ticks: 10, hash: 1 },
            actual: EndRecord { ticks: 11, hash: 1 },
        });
        assert_eq!(
            status(&report(Outcome::Failed(end), RecordingKind::Edrf)).0,
            2
        );
    }

    #[test]
    fn result_line_formats() {
        let one = report(Outcome::Completed, RecordingKind::Drf);
        assert_eq!(
            status(&one).1.as_deref(),
            Some("score=3100 level=2 ticks=1378\n")
        );
        let two = ReplayReport {
            kind: RecordingKind::Edrf,
            outcome: Outcome::Verified,
            scores: [18100, 1100],
            two_scores: true,
            level: 4,
            ticks: 4489,
            last_good_checkpoint: Some(4352),
        };
        assert_eq!(
            status(&two).1.as_deref(),
            Some("score=18100 score2=1100 level=4 ticks=4489\n")
        );
    }
}
