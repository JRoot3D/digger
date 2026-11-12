//! The platform crates are used only inside `src/platform/` (design D1 of
//! `add-desktop-app`): the shell, input, pacing, presentation,
//! configuration and score code must build without a window, an audio
//! device or a gamepad, so that tests run headless and the browser build
//! can reuse them.

#![allow(
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;
use std::path::{Path, PathBuf};

/// The crates only `src/platform/` may name.
const PLATFORM_CRATES: &[&str] = &["winit", "softbuffer", "cpal", "gilrs"];

/// Every `.rs` file under `dir`, recursively, in a stable order.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()));
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The platform crates named as identifiers in `source`, outside `//`
/// comments, with their line numbers.
fn platform_names(source: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let code = line.split_once("//").map_or(line, |(code, _)| code);
        let words = code.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
        for word in words {
            if let Some(name) = PLATFORM_CRATES.iter().find(|name| **name == word) {
                found.push((index + 1, *name));
            }
        }
    }
    found
}

#[test]
fn detector_finds_platform_paths_and_skips_comments() {
    let source = "\
//! Uses winit through the platform layer.
use winit::event_loop::EventLoop;
let x = cpal::default_host(); // gilrs here is a comment
use std::fmt; // softbuffer
pub(crate) struct Winit; // a different identifier
";
    assert_eq!(platform_names(source), [(2, "winit"), (3, "cpal")]);
}

#[test]
fn platform_imports_are_confined() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let platform = src.join("platform");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    assert!(
        files.iter().any(|f| f.ends_with("main.rs")),
        "the source tree was not found under {}",
        src.display()
    );
    let mut offences = Vec::new();
    for file in files.iter().filter(|file| !file.starts_with(&platform)) {
        let source = fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
        for (line, name) in platform_names(&source) {
            offences.push(format!("{}:{line}: {name}", file.display()));
        }
    }
    assert!(
        offences.is_empty(),
        "platform crates named outside src/platform/:\n{}",
        offences.join("\n")
    );
}
