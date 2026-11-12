//! `cargo xtask bisect <file.edrf>`: where to look for a divergence
//! (design D10 and D12 of `add-recording-and-conformance`).
//!
//! The recording is verified as `cargo xtask verify` does. When it fails
//! (or stops early) the report names the failing window, from the tick
//! after the last matching checkpoint to the tick of the failure; the
//! round-end checkpoints the recording places inside that window (the
//! failing one included, when it is a round end); and the `dump-state` and
//! `cut` commands to run next. When a round-end checkpoint fails on the
//! very tick whose tick checkpoint matched, the window is that one tick and
//! the report says that the difference arises in the round's clean-up. It works only from the
//! recording's own checkpoints and the port's playback.
//!
//! The report goes to standard output; the exit status is 0 when the
//! recording was played (whether or not it diverges) and 1 when it could
//! not be read or is not an eDRF.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;
use digger_core::recording::{Record, RecordingKind, TimedRecord, edrf_timeline};
use digger_core::replay::{Divergence, Failure, Outcome, ReplayOptions, ReplayReport, verify};

/// Arguments of `cargo xtask bisect`.
#[derive(Debug, Args)]
pub(crate) struct BisectArgs {
    /// The eDRF recording to examine.
    pub(crate) file: PathBuf,
}

/// The ticks to examine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Window {
    /// The first tick of the window: the one after the last matching
    /// checkpoint.
    pub(crate) from: u32,
    /// The tick of the failure, the last of the window.
    pub(crate) to: u32,
}

/// The failing window of a finished playback, if it did not pass.
pub(crate) fn window(report: &ReplayReport) -> Option<Window> {
    let to = match &report.outcome {
        Outcome::Failed(failure) => failure.tick().unwrap_or(report.ticks),
        Outcome::Stopped => report.ticks,
        Outcome::Verified | Outcome::Completed | Outcome::Truncated | Outcome::TakenOver => {
            return None;
        }
    };
    let from = report
        .last_good_checkpoint
        .map_or(1, |tick| tick.saturating_add(1))
        .min(to.max(1));
    Some(Window {
        from,
        to: to.max(from),
    })
}

/// The `E` records the recording places inside `window`.
fn round_ends_in(timeline: &[TimedRecord], window: Window) -> Vec<(u32, u32)> {
    timeline
        .iter()
        .filter_map(|timed| match timed.record {
            Record::RoundEnd(hash) if (window.from..=window.to).contains(&timed.tick) => {
                Some((timed.tick, hash))
            }
            _ => None,
        })
        .collect()
}

/// The name a triage file for `file` gets under `target/triage/`.
fn triage_path(file: &Path, suffix: &str) -> String {
    let stem = file
        .file_stem()
        .map_or_else(|| "recording".into(), |s| s.to_string_lossy());
    format!("target/triage/{stem}-{suffix}")
}

/// The report for `file`.
pub(crate) fn bisect(file: &Path, bytes: &[u8]) -> Result<String, String> {
    let timeline = edrf_timeline(bytes).map_err(|error| match error {
        digger_core::recording::RecordingError::NotARecording
            if digger_core::recording::detect_kind(bytes) == Ok(RecordingKind::Drf) =>
        {
            "bisect needs an eDRF; convert the DRF first with `cargo xtask convert`".to_owned()
        }
        error => error.to_string(),
    })?;
    let report = verify(bytes, ReplayOptions::default()).map_err(|e| e.to_string())?;
    let shown = file.display();
    let Some(window) = window(&report) else {
        let what = match report.outcome {
            Outcome::Truncated => "truncated, every checkpoint it has matches",
            _ => "verified",
        };
        return Ok(format!("{shown}: {what}; nothing to bisect\n"));
    };
    let mut text = String::new();
    let _ = writeln!(text, "recording: {shown}");
    match &report.outcome {
        Outcome::Failed(failure) => {
            let _ = writeln!(text, "failure: {failure}");
        }
        _ => {
            let _ = writeln!(
                text,
                "failure: the game ended on tick {}, before the end of the recording",
                report.ticks
            );
        }
    }
    match report.last_good_checkpoint {
        Some(tick) => {
            let _ = writeln!(text, "last matching checkpoint: tick {tick}");
        }
        None => text
            .push_str("last matching checkpoint: none (the window starts at the game's start)\n"),
    }
    let length = window.to - window.from + 1;
    let _ = writeln!(
        text,
        "failing window: ticks {} to {} ({length} tick{})",
        window.from,
        window.to,
        if length == 1 { "" } else { "s" }
    );
    if let Outcome::Failed(Failure::Divergence(Divergence::RoundEnd { tick, .. })) = &report.outcome
        && report.last_good_checkpoint == Some(*tick)
    {
        let _ = writeln!(
            text,
            "note: the state after tick {tick} matched its tick checkpoint, so the \
             difference arises in the round-end clean-up after it (monsters removed, \
             bags tidied, playfield saved) or in the tick count the round-end hash appends"
        );
    }
    let round_ends = round_ends_in(&timeline, window);
    if round_ends.is_empty() {
        text.push_str("recorded round ends inside the window: none\n");
    } else {
        text.push_str("recorded round ends inside the window:\n");
        for (tick, hash) in round_ends {
            let _ = writeln!(text, "  tick {tick}: E {hash:08X}");
        }
    }
    let range = format!("{}-{}", window.from, window.to);
    let _ = writeln!(
        text,
        "next: cargo xtask dump-state {shown} --from {} --to {} --fields --out {}",
        window.from,
        window.to,
        triage_path(file, &format!("{range}.trace"))
    );
    let _ = writeln!(
        text,
        "then: cargo xtask cut {shown} --at {} --out {}",
        window.to,
        triage_path(file, &format!("{}.edrf", window.to))
    );
    Ok(text)
}

/// Runs `cargo xtask bisect` and yields its exit status.
pub(crate) fn run(args: &BisectArgs) -> u8 {
    let result = fs::read(&args.file)
        .map_err(|e| format!("cannot open {}: {e}", args.file.display()))
        .and_then(|bytes| bisect(&args.file, &bytes));
    match result {
        Ok(text) => {
            print!("{text}");
            0
        }
        Err(error) => {
            eprintln!("xtask bisect: {}: {error}", args.file.display());
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use digger_core::recording::RecordingKind;

    fn report(outcome: Outcome, last: Option<u32>, ticks: u32) -> ReplayReport {
        ReplayReport {
            kind: RecordingKind::Edrf,
            outcome,
            scores: [0, 0],
            two_scores: false,
            level: 1,
            ticks,
            last_good_checkpoint: last,
        }
    }

    fn round_end(tick: u32) -> Outcome {
        Outcome::Failed(Failure::Divergence(Divergence::RoundEnd {
            tick,
            expected: 1,
            actual: 2,
        }))
    }

    #[test]
    fn window_runs_from_after_the_last_match_to_the_failure() {
        let tick = Outcome::Failed(Failure::Divergence(Divergence::Tick {
            tick: 768,
            expected: 1,
            actual: 2,
        }));
        assert_eq!(
            window(&report(tick.clone(), Some(700), 768)),
            Some(Window { from: 701, to: 768 })
        );
        assert_eq!(
            window(&report(tick, None, 768)),
            Some(Window { from: 1, to: 768 })
        );
        assert_eq!(
            window(&report(Outcome::Stopped, Some(1024), 2000)),
            Some(Window {
                from: 1025,
                to: 2000
            })
        );
        assert_eq!(window(&report(Outcome::Verified, Some(2947), 2947)), None);
        assert_eq!(window(&report(Outcome::Truncated, Some(512), 600)), None);
    }

    #[test]
    fn round_end_failing_on_a_matched_tick_is_a_one_tick_window() {
        assert_eq!(
            window(&report(round_end(1024), Some(1024), 1024)),
            Some(Window {
                from: 1024,
                to: 1024
            })
        );
    }
}
