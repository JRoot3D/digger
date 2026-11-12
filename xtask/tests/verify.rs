//! `cargo xtask verify` end to end (task 7.3 of
//! `add-recording-and-conformance`; replay-verification spec, "Verify exit
//! statuses" and "Result line").

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The corpus file `name`.
fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/recordings/data")
        .join(name)
}

/// A scratch file for this test.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("xtask-verify");
    fs::create_dir_all(&dir).expect("a scratch directory");
    dir.join(name)
}

fn verify(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("verify")
        .arg(path)
        .output()
        .expect("xtask runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The lines of `dror0.edrf`.
fn dror0_lines() -> Vec<String> {
    let bytes = fs::read(corpus("dror0.edrf")).expect("the corpus file");
    text(&bytes).lines().map(str::to_owned).collect()
}

#[test]
fn verified_recording_prints_the_result_line() {
    let output = verify(&corpus("dror0.edrf"));
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "score=0 level=2 ticks=2947\n");
    assert_eq!(text(&output.stderr), "");
}

#[test]
fn missing_path_is_status_1() {
    let path = scratch("no-such-recording.edrf");
    let _ = fs::remove_file(&path);
    let output = verify(&path);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text(&output.stdout), "");
    let stderr = text(&output.stderr);
    assert!(stderr.contains("no-such-recording.edrf"), "{stderr}");
}

#[test]
fn bad_record_is_status_2() {
    let mut lines = dror0_lines();
    // After the header and well inside the body.
    lines.insert(10, "X 1".to_owned());
    let path = scratch("bad-record.edrf");
    fs::write(&path, lines.join("\n") + "\n").expect("written");
    let output = verify(&path);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    let stderr = text(&output.stderr);
    assert!(stderr.contains("\"X 1\""), "{stderr}");
    assert!(stderr.contains("line 11"), "{stderr}");
}

#[test]
fn cut_after_a_checkpoint_is_status_4() {
    let lines = dror0_lines();
    let first_c = lines
        .iter()
        .position(|line| line.starts_with("C "))
        .expect("a checkpoint");
    let path = scratch("cut.edrf");
    fs::write(&path, lines[..=first_c].join("\n") + "\n").expect("written");
    let output = verify(&path);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(4), "{stderr}");
    assert_eq!(text(&output.stdout), "");
    assert!(
        stderr.contains("the recording ends on tick 256, before its game does"),
        "{stderr}"
    );
}

#[test]
fn corrupted_checkpoint_is_status_2() {
    let mut lines = dror0_lines();
    let second_c = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("C "))
        .nth(1)
        .map(|(i, _)| i)
        .expect("two checkpoints");
    let line = &mut lines[second_c];
    let last = line.pop().expect("a hex digit");
    line.push(if last == '0' { '1' } else { '0' });
    let path = scratch("corrupted.edrf");
    fs::write(&path, lines.join("\n") + "\n").expect("written");
    let output = verify(&path);
    let stderr = text(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert_eq!(text(&output.stdout), "");
    assert!(stderr.contains("before tick 512 (tick state"), "{stderr}");
    assert!(
        stderr.contains("last matching checkpoint: tick"),
        "{stderr}"
    );
    assert!(stderr.contains("eDRF: playback of"), "{stderr}");
}
