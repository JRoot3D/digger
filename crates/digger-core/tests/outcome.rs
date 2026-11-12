//! How a playback ends (task group 7 of `add-recording-and-conformance`;
//! replay-verification spec, "Playback outcome classification",
//! "Checkpoint verification during eDRF playback").
//!
//! The recordings are upstream's `sobodig` pair, played as they are and
//! edited the way each outcome needs: controls added, the end record
//! dropped, a checkpoint altered.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;
use std::path::Path;

use digger_core::recording::RecordingKind;
use digger_core::replay::{Divergence, Failure, Outcome, ReplayOptions, ReplayReport, verify};

/// The bytes of a corpus file.
fn corpus(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/recordings/data")
        .join(name);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    String::from_utf8(bytes).expect("an ASCII recording")
}

fn run(text: &str) -> ReplayReport {
    verify(text.as_bytes(), ReplayOptions::default()).expect("a playable recording")
}

/// `text` with the line starting with `prefix` replaced by `with` lines.
fn replace_line(text: &str, prefix: &str, with: &[&str]) -> String {
    let mut found = false;
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        if !found && line.starts_with(prefix) {
            found = true;
            for new in with {
                out.push_str(new);
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    assert!(found, "no line starts with {prefix:?}");
    out
}

#[test]
fn outcome_verified() {
    let report = run(&corpus("sobodig.edrf"));
    assert_eq!(report.outcome, Outcome::Verified);
    assert_eq!(report.kind, RecordingKind::Edrf);
    assert_eq!(report.result_line(), "score=3100 level=2 ticks=1378\n");
    assert_eq!(report.exit_status(), 0);
    assert_eq!(report.last_good_checkpoint, Some(1378), "the end record");
}

#[test]
fn outcome_completed_drf() {
    let report = run(&corpus("sobodig.drf"));
    assert_eq!(report.outcome, Outcome::Completed);
    assert_eq!(report.kind, RecordingKind::Drf);
    assert!(!report.two_scores);
    assert_eq!(
        (report.scores, report.level, report.ticks),
        ([3100, 0], 2, 1378)
    );
    assert_eq!(report.last_good_checkpoint, None);
}

#[test]
fn outcome_stopped_when_game_ends_early() {
    // The game is over after tick 1378, but the recording now holds
    // controls up to tick 6378: stopped, not compared with `Z`.
    let text = replace_line(
        &corpus("sobodig.edrf"),
        "Z ",
        &["I @5000", "Z 6378 DBB1FC50"],
    );
    let report = run(&text);
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(report.exit_status(), 3);
    assert_eq!(report.ticks, 1378);

    // A DRF with another round after the game ended by itself.
    let drf = corpus("sobodig.drf");
    let tail = drf.rfind("EOL").expect("an EOL") + 3;
    let mut text = drf[..tail].to_owned();
    text.push_str("\n00000000\nr999\nEOL\n");
    let report = run(&text);
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(report.ticks, 1378);

    // A playback the caller ends at its tick limit is stopped as well.
    let options = ReplayOptions {
        max_ticks: 300,
        ..ReplayOptions::default()
    };
    for name in ["sobodig.edrf", "sobodig.drf"] {
        let report = verify(corpus(name).as_bytes(), options).expect("playable");
        assert_eq!(report.outcome, Outcome::Stopped, "{name}");
        assert_eq!(report.ticks, 300, "{name}");
    }
}

#[test]
fn outcome_truncated() {
    let text = replace_line(&corpus("sobodig.edrf"), "Z ", &[]);
    let report = run(&text);
    assert_eq!(report.outcome, Outcome::Truncated);
    assert_eq!(report.exit_status(), 4);
    assert_eq!(report.ticks, 1378);
    // Every checkpoint up to the cut was compared: the last is the final
    // round's end.
    assert_eq!(report.last_good_checkpoint, Some(1378));
}

#[test]
fn outcome_failure_takes_precedence() {
    // Truncated and with a wrong checkpoint for tick 512: failed there.
    let text = replace_line(&corpus("sobodig.edrf"), "Z ", &[]);
    let text = replace_line(&text, "C 79AFB1ED", &["C 79AFB1EE"]);
    let report = run(&text);
    assert_eq!(
        report.outcome,
        Outcome::Failed(Failure::Divergence(Divergence::Tick {
            tick: 512,
            expected: 0x79AF_B1EE,
            actual: 0x79AF_B1ED,
        }))
    );
    assert_eq!(report.exit_status(), 2);
    assert_eq!(report.last_good_checkpoint, Some(256));
    // The playback stops at the first failure.
    assert!(report.ticks < 1378, "stopped at {}", report.ticks);

    // Also over stopped: the wrong checkpoint and extra controls.
    let text = replace_line(
        &corpus("sobodig.edrf"),
        "Z ",
        &["I @5000", "Z 6378 DBB1FC50"],
    );
    let text = replace_line(&text, "C 79AFB1ED", &["C 79AFB1EE"]);
    assert_eq!(run(&text).exit_status(), 2);

    // A wrong end record: failed at the end.
    let text = replace_line(&corpus("sobodig.edrf"), "Z ", &["Z 1379 DBB1FC50"]);
    let report = run(&text);
    let Outcome::Failed(Failure::Divergence(divergence)) = report.outcome else {
        panic!("failed expected: {:?}", report.outcome);
    };
    assert_eq!(divergence.tick(), 1378);
    assert!(
        divergence
            .to_string()
            .contains("(expected 1379/DBB1FC50, got 1378/DBB1FC50)"),
        "{divergence}"
    );
}
