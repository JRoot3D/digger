//! The conformance suite: every upstream recording of the corpus, verified
//! and compared with its expected result (task 7.4 of
//! `add-recording-and-conformance`; replay-verification spec, "Conformance
//! suite" and "Suite tiers and run time"; design D11).
//!
//! One test per recording, generated from the `recording!` list below.
//! Each entry names the test, the file, the tier and the state:
//!
//! - `quick` recordings run with the ordinary `cargo test`; `full` ones
//!   carry `#[ignore = "full tier"]` and run with
//!   `cargo test --release -p digger-core --test conformance -- --ignored`;
//! - `pending` marks a recording that does not pass yet (divergence
//!   triage, task group 12, tracked in `docs/conformance.md`): a pending
//!   quick recording is ignored as well, so that the ordinary test run
//!   stays green, and is run with `-- --ignored <name>` while it is
//!   being fixed. Fixing it flips the entry to `passing`.
//!
//! A recording passes when its verify run exits with status 0 and its
//! result line equals `results/<base>.out`.
//! [`conformance_list_matches_corpus`] keeps the list and the corpus
//! directory in step.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use digger_core::replay::{ReplayOptions, verify};

/// Which test run verifies a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tier {
    /// The ordinary `cargo test`.
    Quick,
    /// Only the full tier, `-- --ignored`.
    Full,
}

/// Whether a recording passes yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Not yet: divergence triage is under way.
    Pending,
    /// It passes, and must go on passing.
    Passing,
}

/// One entry of the list.
#[derive(Debug, Clone, Copy)]
struct Entry {
    file: &'static str,
    tier: Tier,
    state: State,
}

/// The base names of the quick tier: those whose DRF is at most 15 KB
/// (replay-verification spec, "Suite tiers and run time").
const QUICK_BASE_NAMES: [&str; 11] = [
    "2riptest01",
    "sobodig",
    "dror0",
    "gauntlet01",
    "2test03",
    "thevoid",
    "ramon",
    "muu27700",
    "2test01",
    "2test02",
    "thevoids",
];

/// The corpus directory, `tests/recordings/`.
fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/recordings")
}

/// Verifies `file` and compares its result line with the expected one.
fn check(file: &str) {
    let corpus = corpus();
    let path = corpus.join("data").join(file);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let base = file.rsplit_once('.').map_or(file, |(base, _)| base);
    let expected_path = corpus.join("results").join(format!("{base}.out"));
    let expected = fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| panic!("{}: {e}", expected_path.display()));
    let report = match verify(&bytes, ReplayOptions::default()) {
        Ok(report) => report,
        Err(error) => panic!(
            "{file}: not playable (status {}): {error}",
            error.exit_status()
        ),
    };
    assert_eq!(
        report.exit_status(),
        0,
        "{file}: status {}\n{}",
        report.exit_status(),
        report.diagnostics(file)
    );
    assert_eq!(report.result_line(), expected, "{file}: result line");
}

macro_rules! tier {
    (quick) => {
        Tier::Quick
    };
    (full) => {
        Tier::Full
    };
}

macro_rules! state {
    (pending) => {
        State::Pending
    };
    (passing) => {
        State::Passing
    };
}

/// One test: ignored in the full tier and while pending.
macro_rules! case {
    ($test:ident, $file:literal, quick, passing) => {
        #[test]
        fn $test() {
            check($file);
        }
    };
    ($test:ident, $file:literal, quick, pending) => {
        #[test]
        #[ignore = "pending: does not pass yet (docs/conformance.md)"]
        fn $test() {
            check($file);
        }
    };
    ($test:ident, $file:literal, full, $state:ident) => {
        #[test]
        #[ignore = "full tier"]
        fn $test() {
            check($file);
        }
    };
}

/// The list: one test per entry, and [`RECORDINGS`].
macro_rules! recordings {
    ($( recording!($test:ident, $file:literal, $tier:ident, $state:ident); )*) => {
        /// Every recording of the corpus, as listed.
        const RECORDINGS: &[Entry] = &[
            $( Entry { file: $file, tier: tier!($tier), state: state!($state) }, )*
        ];
        $( case!($test, $file, $tier, $state); )*
    };
}

recordings! {
    recording!(edrf_2riptest01, "2riptest01.edrf", quick, passing);
    recording!(drf_2test01, "2test01.drf", quick, passing);
    recording!(edrf_2test01, "2test01.edrf", quick, passing);
    recording!(edrf_2test02, "2test02.edrf", quick, passing);
    recording!(edrf_2test03, "2test03.edrf", quick, passing);
    recording!(drf_dror0, "dror0.drf", quick, passing);
    recording!(edrf_dror0, "dror0.edrf", quick, passing);
    recording!(edrf_gauntlet01, "gauntlet01.edrf", quick, passing);
    recording!(drf_muu27700, "muu27700.drf", quick, passing);
    recording!(edrf_muu27700, "muu27700.edrf", quick, passing);
    recording!(drf_ramon, "ramon.drf", quick, passing);
    recording!(edrf_ramon, "ramon.edrf", quick, passing);
    recording!(drf_sobodig, "sobodig.drf", quick, passing);
    recording!(edrf_sobodig, "sobodig.edrf", quick, passing);
    recording!(drf_thevoid, "thevoid.drf", quick, passing);
    recording!(edrf_thevoid, "thevoid.edrf", quick, passing);
    recording!(drf_thevoids, "thevoids.drf", quick, passing);
    recording!(edrf_thevoids, "thevoids.edrf", quick, passing);
    recording!(drf_a1104675, "a1104675.drf", full, passing);
    recording!(edrf_a1104675, "a1104675.edrf", full, passing);
    recording!(drf_a1195000, "a1195000.drf", full, passing);
    recording!(edrf_a1195000, "a1195000.edrf", full, passing);
    recording!(drf_a1262500, "a1262500.drf", full, passing);
    recording!(edrf_a1262500, "a1262500.edrf", full, passing);
    recording!(drf_b1072175, "b1072175.drf", full, passing);
    recording!(edrf_b1072175, "b1072175.edrf", full, passing);
    recording!(drf_f1172525, "f1172525.drf", full, passing);
    recording!(edrf_f1172525, "f1172525.edrf", full, passing);
    recording!(drf_g1322150, "g1322150.drf", full, passing);
    recording!(edrf_g1322150, "g1322150.edrf", full, passing);
    recording!(drf_h1266775, "h1266775.drf", full, passing);
    recording!(edrf_h1266775, "h1266775.edrf", full, passing);
    recording!(drf_k1859075, "k1859075.drf", full, passing);
    recording!(edrf_k1859075, "k1859075.edrf", full, passing);
    recording!(drf_k2223700, "k2223700.drf", full, passing);
    recording!(edrf_k2223700, "k2223700.edrf", full, passing);
    recording!(drf_m1143925, "m1143925.drf", full, passing);
    recording!(edrf_m1143925, "m1143925.edrf", full, passing);
    recording!(drf_p1092525, "p1092525.drf", full, passing);
    recording!(edrf_p1092525, "p1092525.edrf", full, passing);
    recording!(drf_r2222400, "r2222400.drf", full, passing);
    recording!(edrf_r2222400, "r2222400.edrf", full, passing);
    recording!(drf_r3333300, "r3333300.drf", full, passing);
    recording!(edrf_r3333300, "r3333300.edrf", full, passing);
    recording!(drf_s1058800, "s1058800.drf", full, passing);
    recording!(edrf_s1058800, "s1058800.edrf", full, passing);
    recording!(drf_s1177650, "s1177650.drf", full, passing);
    recording!(edrf_s1177650, "s1177650.edrf", full, passing);
    recording!(drf_s1302025, "s1302025.drf", full, passing);
    recording!(edrf_s1302025, "s1302025.edrf", full, passing);
}

#[test]
fn conformance_list_matches_corpus() {
    let data = corpus().join("data");
    let on_disk: BTreeSet<String> = fs::read_dir(&data)
        .unwrap_or_else(|e| panic!("{}: {e}", data.display()))
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| {
            Path::new(name)
                .extension()
                .is_some_and(|ext| ext == "drf" || ext == "edrf")
        })
        .collect();
    let listed: BTreeSet<String> = RECORDINGS
        .iter()
        .map(|entry| entry.file.to_owned())
        .collect();
    assert_eq!(listed.len(), RECORDINGS.len(), "a file is listed twice");
    assert_eq!(listed, on_disk, "the list and the corpus differ");
    assert_eq!(RECORDINGS.len(), 50);

    // The tier follows from the base name.
    for entry in RECORDINGS {
        let base = entry.file.rsplit_once('.').map_or(entry.file, |(b, _)| b);
        let quick = QUICK_BASE_NAMES.contains(&base);
        assert_eq!(entry.tier == Tier::Quick, quick, "{}: tier", entry.file);
        let expected = corpus().join("results").join(format!("{base}.out"));
        assert!(expected.is_file(), "{}: no expected result", entry.file);
    }
    let quick = RECORDINGS.iter().filter(|e| e.tier == Tier::Quick).count();
    assert_eq!((quick, RECORDINGS.len() - quick), (18, 32));
    let pending: Vec<&str> = RECORDINGS
        .iter()
        .filter(|e| e.state == State::Pending)
        .map(|e| e.file)
        .collect();
    println!("pending: {pending:?}");
}

/// The definition of done of `add-recording-and-conformance` (task 13.1):
/// all 50 upstream recordings pass, 18 in the quick tier and 32 in the
/// full tier, and none is left pending.
#[test]
fn no_pending_recordings() {
    let pending: Vec<&str> = RECORDINGS
        .iter()
        .filter(|e| e.state == State::Pending)
        .map(|e| e.file)
        .collect();
    assert!(pending.is_empty(), "pending recordings: {pending:?}");
    let quick = RECORDINGS.iter().filter(|e| e.tier == Tier::Quick).count();
    let full = RECORDINGS.iter().filter(|e| e.tier == Tier::Full).count();
    assert_eq!((quick, full), (18, 32), "quick and full-tier entries");
}

#[test]
fn tracking_table_lists_every_recording() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/conformance.md");
    let doc = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let start = doc
        .find("## Tracking table")
        .expect("docs/conformance.md has a tracking table");
    let table = doc.get(start..).unwrap_or_default();
    let mut rows = Vec::new();
    for line in table.lines().filter(|l| l.starts_with("| `")) {
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        let file = cells
            .first()
            .and_then(|c| c.strip_prefix('`'))
            .and_then(|c| c.strip_suffix('`'))
            .unwrap_or_else(|| panic!("a recording name: {line}"));
        let status = cells.get(1).copied().unwrap_or_default();
        rows.push((file.to_owned(), status.to_owned()));
    }
    assert_eq!(rows.len(), 50, "one row per recording");
    let listed: BTreeSet<&str> = rows.iter().map(|(file, _)| file.as_str()).collect();
    let expected: BTreeSet<&str> = RECORDINGS.iter().map(|e| e.file).collect();
    assert_eq!(listed, expected, "the table and the list differ");
    for (file, status) in &rows {
        let entry = RECORDINGS
            .iter()
            .find(|e| e.file == file)
            .expect("a listed recording");
        let expected = match entry.state {
            State::Passing => "passing",
            State::Pending => "failing",
        };
        assert_eq!(status, expected, "{file}: status in docs/conformance.md");
    }
}
