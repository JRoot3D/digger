//! Starting without a display (`desktop-app` spec, supported desktop
//! platforms: no display available): one `digger: error:` line on standard
//! error and exit status 69. Linux only, where the display is named by
//! `WAYLAND_DISPLAY`, `WAYLAND_SOCKET` and `DISPLAY`.

#![cfg(target_os = "linux")]
#![allow(
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;
use std::process::Command;

#[test]
fn no_display_exits_69() {
    let dir = std::env::temp_dir().join(format!("digger-no-display-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let output = Command::new(env!("CARGO_BIN_EXE_digger"))
        .arg("--config")
        .arg(dir.join("config.toml"))
        .arg("--data-dir")
        .arg(&dir)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .output()
        .unwrap_or_else(|e| panic!("cannot run digger: {e}"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(69), "{output:?}");
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 1, "{stderr}");
    assert!(
        lines
            .first()
            .is_some_and(|line| line.starts_with("digger: error: ")),
        "{stderr}"
    );
    // Nothing was written: the program stopped before the game began.
    assert!(!dir.join("config.toml").exists());
    let _ = fs::remove_dir_all(&dir);
}
