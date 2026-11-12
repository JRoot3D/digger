//! `cargo xtask convert <in> <out>`: plays a recording and records it again
//! in the format `<out>`'s name selects, `.edrf` for eDRF and anything else
//! for DRF (game-recording spec, "Re-recording and conversion between
//! formats", "Saving and default file names"; design D10 of
//! `add-recording-and-conformance`).
//!
//! The playback is the headless verify run with a recorder attached, so
//! the exit status is the verify run's: 0 when the input verified or
//! completed. The output is written whenever the input could be played,
//! with the playback's diagnostics on standard error when it did not
//! succeed.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;
use digger_core::FrameInput;
use digger_core::recording::{kind_for_name, parse};
use digger_core::replay::{Replay, ReplayOptions, ReplayStep, STATUS_UNREADABLE};

/// Arguments of `cargo xtask convert`.
#[derive(Debug, Args)]
pub(crate) struct ConvertArgs {
    /// The DRF or eDRF recording to play.
    pub(crate) input: PathBuf,
    /// Where to write the new recording: eDRF if the name ends in `.edrf`,
    /// DRF otherwise.
    pub(crate) output: PathBuf,
}

/// Plays `input` and returns the playback's exit status, its diagnostics
/// and the re-recorded text (`None` when `input` could not be played).
fn convert(input: &Path, output: &Path) -> (u8, String, Option<String>) {
    let bytes = match fs::read(input) {
        Ok(bytes) => bytes,
        Err(error) => {
            let message = format!("cannot open {}: {error}\n", input.display());
            return (STATUS_UNREADABLE, message, None);
        }
    };
    let unplayable = |error: digger_core::recording::RecordingError| {
        let message = format!("{}: {error}\n", input.display());
        (error.exit_status(), message, None)
    };
    let recording = match parse(&bytes) {
        Ok(recording) => recording,
        Err(error) => return unplayable(error),
    };
    let kind = kind_for_name(&output.to_string_lossy());
    let mut replay = match Replay::recording_as(&recording, ReplayOptions::default(), kind) {
        Ok(replay) => replay,
        Err(error) => return unplayable(error),
    };
    while let ReplayStep::Running(_) =
        replay.step(&FrameInput::NONE, &mut digger_core::NoopObserver)
    {}
    let (report, text) = replay.finish_with_recording();
    let diagnostics = report.diagnostics(&input.display().to_string());
    (report.exit_status(), diagnostics, text)
}

/// Whether `a` and `b` name the same file.
fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Runs `cargo xtask convert` and yields its exit status.
pub(crate) fn run(args: &ConvertArgs) -> u8 {
    if same_file(&args.input, &args.output) {
        eprintln!(
            "xtask convert: cannot record over {} while playing it back",
            args.input.display()
        );
        return STATUS_UNREADABLE;
    }
    let (status, diagnostics, text) = convert(&args.input, &args.output);
    eprint!("{diagnostics}");
    let Some(text) = text else {
        return status;
    };
    if let Err(error) = fs::write(&args.output, text) {
        eprintln!(
            "xtask convert: cannot write {}: {error}",
            args.output.display()
        );
        return STATUS_UNREADABLE;
    }
    status
}
