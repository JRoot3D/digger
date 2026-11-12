//! Golden sample hashes (design D12, item 4).
//!
//! Every script `tests/golden/*.txt` is rendered at 44100 Hz with the
//! filter off and volume 100, and the SHA-256 of its samples as
//! little-endian bytes is compared with `tests/golden/hashes.toml`.
//! The renders were reviewed once (see `tests/golden/REVIEW.md`); the
//! hashes then guard against unintended change.
//!
//! To regenerate the hash file after an intended change, run
//! `UPDATE_GOLDEN=1 cargo test -p digger-sound --test golden`, and review
//! the changed renders with `cargo xtask render-sound` before committing.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use digger_sound::{Script, SoundConfig, TuneTables};
use sha2::{Digest, Sha256};

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// The golden settings: 44100 Hz, sound and music on, volume 100, filter
/// off.
fn config() -> SoundConfig {
    SoundConfig {
        filter: false,
        ..SoundConfig::default()
    }
}

/// The scripts, by name (file stem), sorted.
fn scripts() -> BTreeMap<String, Script> {
    let mut scripts = BTreeMap::new();
    for entry in fs::read_dir(golden_dir()).expect("tests/golden") {
        let path = entry.expect("directory entry").path();
        if path.extension().is_none_or(|e| e != "txt") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("UTF-8 name")
            .to_owned();
        let text = fs::read_to_string(&path).expect("script text");
        let script = text
            .parse()
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        scripts.insert(name, script);
    }
    scripts
}

fn render(script: &Script) -> Vec<i16> {
    let tables = TuneTables::embedded().expect("embedded tables decode");
    script.render(config(), tables)
}

fn sha256_hex(samples: &[i16]) -> String {
    let mut hasher = Sha256::new();
    for sample in samples {
        hasher.update(sample.to_le_bytes());
    }
    hasher.finalize().iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Reads the `name = "hex"` lines of the hash file.
fn read_hashes(text: &str) -> BTreeMap<String, String> {
    let mut hashes = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("hashes.toml line {}: no `=`", index + 1));
        let key = key.trim().trim_matches('"').to_owned();
        let value = value
            .trim()
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or_else(|| panic!("hashes.toml line {}: value not quoted", index + 1));
        assert!(
            hashes.insert(key.clone(), value.to_owned()).is_none(),
            "hashes.toml: {key} appears twice"
        );
    }
    hashes
}

fn write_hashes(hashes: &BTreeMap<String, String>) -> String {
    let mut text = String::from(
        "# SHA-256 of the little-endian i16 samples of each tests/golden/*.txt\n\
         # script, rendered at 44100 Hz, filter off, volume 100.\n\
         # Regenerate: UPDATE_GOLDEN=1 cargo test -p digger-sound --test golden\n\n",
    );
    for (name, hash) in hashes {
        let _ = writeln!(text, "{name} = \"{hash}\"");
    }
    text
}

#[test]
fn golden_renders_match_hashes() {
    let path = golden_dir().join("hashes.toml");
    let actual: BTreeMap<String, String> = scripts()
        .iter()
        .map(|(name, script)| (name.clone(), sha256_hex(&render(script))))
        .collect();
    assert!(
        actual.len() >= 10,
        "expected the golden scripts, found {actual:?}"
    );

    if std::env::var_os("UPDATE_GOLDEN").is_some_and(|v| v == "1") {
        fs::write(&path, write_hashes(&actual)).expect("write hashes.toml");
        return;
    }

    let text = fs::read_to_string(&path).expect("tests/golden/hashes.toml");
    let expected = read_hashes(&text);
    let mut problems = Vec::new();
    for (name, hash) in &actual {
        match expected.get(name) {
            None => problems.push(format!("{name}: no hash recorded")),
            Some(want) if want != hash => {
                problems.push(format!("{name}: expected {want}, rendered {hash}"));
            }
            Some(_) => {}
        }
    }
    for name in expected.keys().filter(|n| !actual.contains_key(*n)) {
        problems.push(format!("{name}: hash recorded but no script"));
    }
    assert!(
        problems.is_empty(),
        "golden renders differ (run with UPDATE_GOLDEN=1 after reviewing an intended change):\n{}",
        problems.join("\n")
    );
}

#[test]
fn same_script_same_samples() {
    // Spec "Reproducible output": the same script rendered twice with
    // filtering off gives identical sample streams.
    let scripts = scripts();
    let mixed = scripts.get("mixed").expect("mixed script");
    assert_eq!(render(mixed), render(mixed));
}

#[test]
fn every_script_is_audible() {
    for (name, script) in scripts() {
        assert!(
            render(&script).iter().any(|&s| s != 0),
            "{name} renders only silence"
        );
    }
}
