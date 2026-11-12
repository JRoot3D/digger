//! The triage commands end to end: `dump-state`, `cut`, `bisect` and
//! `trace-diff` (task group 10 of `add-recording-and-conformance`; design
//! D10 and D12).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use digger_core::recording::{Record, Recording, edrf_timeline, parse};
use digger_core::replay::{Outcome, ReplayOptions, TraceLine, verify};

/// The corpus file `name`.
fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/recordings/data")
        .join(name)
}

/// A scratch file for this test.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("xtask-triage");
    fs::create_dir_all(&dir).expect("a scratch directory");
    dir.join(name)
}

fn xtask(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .output()
        .expect("xtask runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn dump_state_of_dror0_ticks_1_to_256() {
    let file = corpus("dror0.edrf");
    let out = scratch("dror0-1-256.trace");
    let output = xtask(&[
        "dump-state".as_ref(),
        file.as_os_str(),
        "--from".as_ref(),
        "1".as_ref(),
        "--to".as_ref(),
        "256".as_ref(),
        "--out".as_ref(),
        out.as_os_str(),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let trace = fs::read_to_string(&out).expect("the trace");
    let lines: Vec<TraceLine> = trace
        .lines()
        .map(|l| TraceLine::parse(l).unwrap_or_else(|| panic!("a trace line: {l}")))
        .collect();
    assert_eq!(lines.len(), 256);
    assert_eq!(lines.first().map(|l| l.tick), Some(1));
    assert_eq!(lines.last().map(|l| l.tick), Some(256));

    // The verifier computed the hash of tick 256 and found it equal to the
    // recording's first `C` record (the recording verifies).
    let bytes = fs::read(&file).expect("the corpus file");
    let report = verify(&bytes, ReplayOptions::default()).expect("playable");
    assert_eq!(report.outcome, Outcome::Verified);
    let Ok(Recording::Edrf(edrf)) = parse(&bytes) else {
        panic!("an eDRF")
    };
    assert_eq!(
        lines.last().map(|l| l.hash),
        edrf.checkpoints().first().copied()
    );
}

#[test]
fn dump_state_to_standard_output_with_fields() {
    let file = corpus("sobodig.edrf");
    let output = xtask(&[
        "dump-state".as_ref(),
        file.as_os_str(),
        "--from".as_ref(),
        "10".as_ref(),
        "--to".as_ref(),
        "11".as_ref(),
        "--fields".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    assert_eq!(stdout.lines().filter_map(TraceLine::parse).count(), 2);
    assert!(stdout.contains("\n  digger.lives[0]="), "{stdout}");
}

/// Cuts `name` at `at` with the command and returns the cut file.
fn cut(name: &str, at: u32) -> PathBuf {
    let file = corpus(name);
    let out = scratch(&format!("{name}-cut-{at}.edrf"));
    let output = xtask(&[
        "cut".as_ref(),
        file.as_os_str(),
        "--at".as_ref(),
        at.to_string().as_ref(),
        "--out".as_ref(),
        out.as_os_str(),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    out
}

#[test]
fn cut_recording_is_truncated_after_exactly_that_many_ticks() {
    // Inside a round, on a checkpoint tick, and in a later level.
    for (name, at) in [
        ("dror0.edrf", 1000),
        ("dror0.edrf", 512),
        ("2test01.edrf", 2500),
    ] {
        let cut = cut(name, at);
        let bytes = fs::read(&cut).expect("the cut file");
        let Ok(Recording::Edrf(edrf)) = parse(&bytes) else {
            panic!("{name} at {at}: the cut file parses as an eDRF")
        };
        assert_eq!(edrf.end(), None);
        assert!(text(&bytes).ends_with("\nQ 0\n"));
        let report = verify(&bytes, ReplayOptions::default()).expect("playable");
        assert_eq!(report.ticks, at, "{name}");
        assert_eq!(report.outcome, Outcome::Truncated, "{name} at {at}");
        let output = xtask(&["verify".as_ref(), cut.as_os_str()]);
        assert_eq!(output.status.code(), Some(4), "{}", text(&output.stderr));
    }
}

#[test]
fn cut_beyond_the_end_is_refused() {
    let file = corpus("dror0.edrf");
    let out = scratch("never-written.edrf");
    let _ = fs::remove_file(&out);
    let output = xtask(&[
        "cut".as_ref(),
        file.as_os_str(),
        "--at".as_ref(),
        "3000".as_ref(),
        "--out".as_ref(),
        out.as_os_str(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!out.exists());
    assert!(
        text(&output.stderr).contains("2947"),
        "{}",
        text(&output.stderr)
    );
}

#[test]
fn bisect_window_ends_at_the_altered_checkpoint() {
    let bytes = fs::read(corpus("dror0.edrf")).expect("the corpus file");
    let timeline = edrf_timeline(&bytes).expect("a timeline");
    // The third `C` record, which follows the first round's `E` record.
    let checkpoint = timeline
        .iter()
        .filter(|r| matches!(r.record, Record::Checkpoint(_)))
        .nth(2)
        .expect("a third checkpoint");
    let round_end = timeline
        .iter()
        .find(|r| matches!(r.record, Record::RoundEnd(_)))
        .expect("a round end");
    assert!(round_end.tick < checkpoint.tick);

    let mut lines: Vec<String> = text(&bytes).lines().map(str::to_owned).collect();
    let index = usize::try_from(checkpoint.line - 1).expect("a line index");
    let line = &mut lines[index];
    assert!(line.starts_with("C "), "{line}");
    let flipped = if line.ends_with('0') { '1' } else { '0' };
    line.pop();
    line.push(flipped);
    let altered = scratch("dror0-altered-c.edrf");
    fs::write(&altered, lines.join("\n") + "\n").expect("written");

    let output = xtask(&["bisect".as_ref(), altered.as_os_str()]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let stdout = text(&output.stdout);
    let window = format!(
        "failing window: ticks {} to {} (",
        round_end.tick + 1,
        checkpoint.tick
    );
    assert!(stdout.contains(&window), "{stdout}");
    assert!(
        stdout.contains(&format!(
            "last matching checkpoint: tick {}",
            round_end.tick
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "dump-state {} --from {} --to {} --fields",
            altered.display(),
            round_end.tick + 1,
            checkpoint.tick
        )),
        "{stdout}"
    );
}

#[test]
fn bisect_of_a_passing_recording_has_nothing_to_report() {
    let output = xtask(&["bisect".as_ref(), corpus("dror0.edrf").as_os_str()]);
    assert_eq!(output.status.code(), Some(0));
    assert!(text(&output.stdout).contains("nothing to bisect"));
    let output = xtask(&["bisect".as_ref(), corpus("dror0.drf").as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("needs an eDRF"));
}

#[test]
fn trace_diff_of_two_dumps() {
    let file = corpus("dror0.edrf");
    let a = scratch("diff-a.trace");
    let output = xtask(&[
        "dump-state".as_ref(),
        file.as_os_str(),
        "--from".as_ref(),
        "513".as_ref(),
        "--to".as_ref(),
        "768".as_ref(),
        "--out".as_ref(),
        a.as_os_str(),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let same = xtask(&["trace-diff".as_ref(), a.as_os_str(), a.as_os_str()]);
    assert_eq!(same.status.code(), Some(0));
    assert_eq!(text(&same.stdout), "traces agree on ticks 513 to 768\n");

    // The same trace with the monster block changed from tick 700 on.
    let trace = fs::read_to_string(&a).expect("the trace");
    let mut changed = String::new();
    for l in trace.lines() {
        let mut line = TraceLine::parse(l).expect("a trace line");
        if line.tick >= 700 {
            line.blocks.hashes[2] ^= 0x5A5A_5A5A;
        }
        changed.push_str(&line.to_string());
        changed.push('\n');
    }
    let b = scratch("diff-b.trace");
    fs::write(&b, changed).expect("written");
    let differs = xtask(&["trace-diff".as_ref(), a.as_os_str(), b.as_os_str()]);
    assert_eq!(differs.status.code(), Some(1));
    assert!(
        text(&differs.stdout).starts_with("first difference: tick 700, monster block\n"),
        "{}",
        text(&differs.stdout)
    );
}
