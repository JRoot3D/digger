//! `--verify --headless` of the built binary on small upstream recordings:
//! the exit statuses 0, 1, 2 and 4, the result line, and re-recording with
//! `--record` (`configuration` spec, play-and-verify, verification result
//! line, verifying without a window; design D11 of `add-desktop-app`).

#![allow(
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The upstream corpus of the repository.
fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/recordings")
}

/// The recording `name` of the corpus.
fn recording(name: &str) -> PathBuf {
    corpus().join("data").join(name)
}

/// The expected result line of the corpus for the base name `stem`.
fn expected(stem: &str) -> String {
    let path = corpus().join("results").join(format!("{stem}.out"));
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Runs `digger --verify <file> --headless` with `extra` arguments, with
/// the configuration and data directories pointed at `dir`.
fn verify(file: &Path, extra: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_digger"))
        .arg("--verify")
        .arg(file)
        .arg("--headless")
        .args(extra)
        .arg("--config")
        .arg(dir.join("config.toml"))
        .arg("--data-dir")
        .arg(dir)
        .output()
        .unwrap_or_else(|e| panic!("cannot run digger: {e}"))
}

fn status(output: &Output) -> i32 {
    output
        .status
        .code()
        .unwrap_or_else(|| panic!("digger was killed: {output:?}"))
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A fresh directory for one test.
fn temp_dir(label: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("digger-verify-cli-{}-{label}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    dir
}

/// Reads `name` from the corpus as text.
fn corpus_text(name: &str) -> String {
    let path = recording(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn good_recordings_verify_with_their_result_line() {
    let dir = temp_dir("good");
    for (name, stem) in [
        ("sobodig.drf", "sobodig"),
        ("sobodig.edrf", "sobodig"),
        ("2test01.edrf", "2test01"),
    ] {
        let output = verify(&recording(name), &[], &dir);
        assert_eq!(status(&output), 0, "{name}: {output:?}");
        assert_eq!(stdout(&output), expected(stem), "{name}");
        assert!(stderr(&output).is_empty(), "{name}: {output:?}");
    }
    assert_eq!(expected("sobodig"), "score=3100 level=2 ticks=1378\n");
    assert_eq!(
        expected("2test01"),
        "score=18100 score2=1100 level=4 ticks=4489\n"
    );
}

#[test]
fn text_file_is_status_1() {
    let dir = temp_dir("text");
    let file = dir.join("notes.txt");
    fs::write(&file, "shopping list\nmilk\n").unwrap_or_else(|e| panic!("{e}"));
    let output = verify(&file, &[], &dir);
    assert_eq!(status(&output), 1, "{output:?}");
    assert!(stdout(&output).is_empty(), "{output:?}");
    let err = stderr(&output);
    assert!(err.starts_with("digger: error: "), "{err}");
    assert!(err.contains("notes.txt"), "{err}");
    // A file that does not exist is status 1 as well.
    let output = verify(&dir.join("missing.drf"), &[], &dir);
    assert_eq!(status(&output), 1, "{output:?}");
    assert!(stderr(&output).contains("missing.drf"), "{output:?}");
}

#[test]
fn altered_checkpoint_is_status_2() {
    let dir = temp_dir("altered");
    let text = corpus_text("sobodig.edrf");
    let altered = text.replacen("C E27A3C09", "C E27A3C0A", 1);
    assert_ne!(altered, text, "the checkpoint is in the recording");
    let file = dir.join("altered.edrf");
    fs::write(&file, altered).unwrap_or_else(|e| panic!("{e}"));
    let output = verify(&file, &[], &dir);
    assert_eq!(status(&output), 2, "{output:?}");
    assert!(stdout(&output).is_empty(), "{output:?}");
    let err = stderr(&output);
    assert!(
        err.contains("the game diverges from the recording before tick 256"),
        "{err}"
    );
    assert!(err.contains("altered.edrf FAILED"), "{err}");
}

#[test]
fn edrf_without_end_marker_is_status_4() {
    let dir = temp_dir("cut");
    let text = corpus_text("sobodig.edrf");
    let mut cut = String::new();
    for line in text.lines().filter(|line| !line.starts_with("Z ")) {
        cut.push_str(line);
        cut.push('\n');
    }
    assert_ne!(cut, text, "the end marker was there");
    let file = dir.join("cut.edrf");
    fs::write(&file, cut).unwrap_or_else(|e| panic!("{e}"));
    let output = verify(&file, &[], &dir);
    assert_eq!(status(&output), 4, "{output:?}");
    assert!(stdout(&output).is_empty(), "{output:?}");
    assert!(
        stderr(&output).contains("before its game does"),
        "{output:?}"
    );
}

#[test]
fn record_while_verifying_converts() {
    let dir = temp_dir("convert");
    let out = dir.join("out.edrf");
    let output = Command::new(env!("CARGO_BIN_EXE_digger"))
        .arg("--record")
        .arg(&out)
        .arg("--verify")
        .arg(recording("sobodig.drf"))
        .arg("--headless")
        .arg("--config")
        .arg(dir.join("config.toml"))
        .arg("--data-dir")
        .arg(&dir)
        .output()
        .unwrap_or_else(|e| panic!("cannot run digger: {e}"));
    assert_eq!(status(&output), 0, "{output:?}");
    assert_eq!(stdout(&output), expected("sobodig"));
    let written = fs::read_to_string(&out).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    assert!(written.starts_with("eDRF 2\n"), "{written}");
    // The converted file verifies with status 0 and the same game.
    let again = verify(&out, &[], &dir);
    assert_eq!(status(&again), 0, "{again:?}");
    assert_eq!(stdout(&again), expected("sobodig"));
}
