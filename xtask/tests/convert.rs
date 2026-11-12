//! `cargo xtask convert` end to end (task 8.6 of
//! `add-recording-and-conformance`; game-recording spec, "Re-recording and
//! conversion between formats").

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A scratch file for this test.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("xtask-convert");
    fs::create_dir_all(&dir).expect("a scratch directory");
    dir.join(name)
}

fn xtask(args: &[&Path]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .output()
        .expect("xtask runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A short hand-written DRF: one round of a few runs, ended with `EOL`
/// and `EOG` as a game quit by Exit is.
fn short_drf() -> String {
    let mut drf = String::from("DRF\nAJ DOS 19990506\n1\n20000\n");
    // Plan 1: a horizontal tunnel along the top row between two emeralds,
    // otherwise earth.
    drf.push_str("CSHHHHHHHHHHHSC\n");
    drf.push_str(&"\n".repeat(79));
    drf.push_str("1A2B3C4D\nr12U3l20d5s4R\nEOL\nEOG\n");
    drf
}

#[test]
fn convert_short_drf_to_verifiable_edrf() {
    let input = scratch("short.drf");
    let output = scratch("short.edrf");
    let _ = fs::remove_file(&output);
    fs::write(&input, short_drf()).expect("written");

    let converted = xtask(&[Path::new("convert"), &input, &output]);
    assert_eq!(
        converted.status.code(),
        Some(0),
        "{}",
        text(&converted.stderr)
    );
    let edrf = fs::read_to_string(&output).expect("the converted file");
    assert!(
        edrf.starts_with("eDRF 2\nRS DIG 20261111\n1\n20000\n"),
        "{edrf}"
    );
    assert!(edrf.contains("\nR 1A2B3C4D\n"), "{edrf}");

    let verified = xtask(&[Path::new("verify"), &output]);
    assert_eq!(
        verified.status.code(),
        Some(0),
        "{}",
        text(&verified.stderr)
    );
    // The DRF itself plays to the same result line.
    let original = xtask(&[Path::new("verify"), &input]);
    assert_eq!(
        original.status.code(),
        Some(0),
        "{}",
        text(&original.stderr)
    );
    assert_eq!(text(&verified.stdout), text(&original.stdout));
    assert!(
        text(&verified.stdout).ends_with(" ticks=46\n"),
        "{}",
        text(&verified.stdout)
    );

    // And back to DRF: `.drf` (or any other name) selects DRF.
    let back = scratch("short-again.drf");
    let reconverted = xtask(&[Path::new("convert"), &output, &back]);
    assert_eq!(
        reconverted.status.code(),
        Some(0),
        "{}",
        text(&reconverted.stderr)
    );
    let drf = fs::read_to_string(&back).expect("the converted file");
    assert!(drf.starts_with("DRF\nRS DIG 20261111\n1\n20000\nCSHHHHHHHHHHHSC\n"));
    let replayed = xtask(&[Path::new("verify"), &back]);
    assert_eq!(
        replayed.status.code(),
        Some(0),
        "{}",
        text(&replayed.stderr)
    );
}

#[test]
fn convert_refuses_its_own_input_and_bad_files() {
    let input = scratch("self.drf");
    fs::write(&input, short_drf()).expect("written");
    let output = xtask(&[Path::new("convert"), &input, &input]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("cannot record over"));
    assert_eq!(fs::read_to_string(&input).expect("kept"), short_drf());

    let missing = scratch("no-such-file.drf");
    let _ = fs::remove_file(&missing);
    let out = scratch("never-written.edrf");
    let _ = fs::remove_file(&out);
    let output = xtask(&[Path::new("convert"), &missing, &out]);
    assert_eq!(output.status.code(), Some(1));
    assert!(!out.exists(), "nothing written for an unreadable input");
}
