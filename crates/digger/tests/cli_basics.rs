//! The built binary's command line: help, version, usage errors and a bad
//! configuration file, each with its exit status (`configuration` spec;
//! design D8 and D12 of `add-desktop-app`).

#![allow(
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

/// Runs the binary with `args`, with the configuration and data
/// directories pointed at `dir` so the real user profile is never read.
fn digger(args: &[&str], dir: Option<&PathBuf>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_digger"));
    command.args(args);
    if let Some(dir) = dir {
        command.arg("--data-dir").arg(dir);
        if !args.contains(&"--config") {
            command.arg("--config").arg(dir.join("config.toml"));
        }
    }
    command
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
        std::env::temp_dir().join(format!("digger-cli-basics-{}-{label}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    dir
}

#[test]
fn help_lists_options_with_aliases() {
    for flag in ["--help", "-h", "/?", "/H"] {
        let output = digger(&[flag], None);
        assert_eq!(status(&output), 0, "{flag}: {output:?}");
        let text = stdout(&output);
        let line = text
            .lines()
            .find(|line| line.contains("--speed <N>"))
            .unwrap_or_else(|| panic!("{flag}: no --speed line in\n{text}"));
        assert!(line.contains("/S:N"), "{line}");
        assert!(stderr(&output).is_empty(), "{flag}: {output:?}");
    }
}

#[test]
fn version_prints_name_and_number() {
    for flag in ["--version", "-V"] {
        let output = digger(&[flag], None);
        assert_eq!(status(&output), 0, "{flag}: {output:?}");
        assert_eq!(
            stdout(&output).trim_end(),
            format!("digger {}", env!("CARGO_PKG_VERSION"))
        );
    }
}

#[test]
fn unknown_option_is_a_usage_error() {
    let dir = temp_dir("turbo");
    let output = digger(&["--turbo"], Some(&dir));
    assert_eq!(status(&output), 64, "{output:?}");
    let text = stderr(&output);
    assert!(text.starts_with("digger: error: "), "{text}");
    assert!(text.contains("--turbo"), "{text}");
    assert!(text.contains("--help"), "{text}");
    assert!(stdout(&output).is_empty(), "{output:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unsupported_upstream_option() {
    let dir = temp_dir("cga");
    let output = digger(&["/C"], Some(&dir));
    assert_eq!(status(&output), 64, "{output:?}");
    assert_eq!(
        stderr(&output).trim_end(),
        "digger: error: option /C is not supported: CGA graphics are not part of this port"
    );
    let output = digger(&["/N:host"], Some(&dir));
    assert_eq!(status(&output), 64, "{output:?}");
    assert!(stderr(&output).contains("/N"), "{output:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn usage_errors_quote_what_was_typed() {
    let dir = temp_dir("usage");
    let output = digger(&["/I:abc"], Some(&dir));
    assert_eq!(status(&output), 64, "{output:?}");
    let text = stderr(&output);
    assert!(text.contains("/I") && text.contains("abc"), "{text}");
    let output = digger(&["--players", "2", "--two-diggers"], Some(&dir));
    assert_eq!(status(&output), 64, "{output:?}");
    assert!(
        stderr(&output).contains("--players 2 cannot be combined with --two-diggers"),
        "{output:?}"
    );
    let output = digger(&["--take-over-at", "500"], Some(&dir));
    assert_eq!(status(&output), 64, "{output:?}");
    let text = stderr(&output);
    assert!(
        text.contains("--take-over-at") && text.contains("--play"),
        "{text}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn bad_config_file_exits_78() {
    let dir = temp_dir("config");
    let config = dir.join("bad.toml");
    fs::write(&config, "[game]\nstart_level = 0\n").unwrap_or_else(|e| panic!("{e}"));
    let config_arg = config.to_string_lossy().into_owned();
    let output = digger(&["--config", &config_arg], Some(&dir));
    assert_eq!(status(&output), 78, "{output:?}");
    let text = stderr(&output);
    assert!(text.starts_with("digger: error: "), "{text}");
    assert!(text.contains("bad.toml"), "{text}");
    assert!(text.contains("game.start_level"), "{text}");
    assert!(text.contains('0'), "{text}");

    // Not TOML at all.
    fs::write(&config, "speed = = 3").unwrap_or_else(|e| panic!("{e}"));
    let output = digger(&["--config", &config_arg], Some(&dir));
    assert_eq!(status(&output), 78, "{output:?}");

    // An unknown setting only warns.
    fs::write(&config, "[game]\nturbo = true\n").unwrap_or_else(|e| panic!("{e}"));
    let output = digger(&["--config", &config_arg], Some(&dir));
    assert_eq!(status(&output), 0, "{output:?}");
    assert!(
        stderr(&output).contains("digger: warning: ") && stderr(&output).contains("game.turbo"),
        "{output:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_level_file_exits_66() {
    let dir = temp_dir("levels");
    let name = dir.join("nosuch");
    let name_arg = name.to_string_lossy().into_owned();
    let output = digger(&["--levels", &name_arg], Some(&dir));
    assert_eq!(status(&output), 66, "{output:?}");
    let text = stderr(&output);
    assert!(
        text.starts_with(&format!("digger: error: cannot read level file {name_arg}")),
        "{text}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_config_file_is_not_an_error() {
    let dir = temp_dir("fresh");
    let output = digger(&["/S:40", "/Q"], Some(&dir));
    assert_eq!(status(&output), 0, "{output:?}");
    assert!(stderr(&output).is_empty(), "{output:?}");
    // Starting does not create the configuration file.
    assert!(!dir.join("config.toml").exists());
    let _ = fs::remove_dir_all(&dir);
}
