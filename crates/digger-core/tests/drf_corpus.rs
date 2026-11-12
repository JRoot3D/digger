//! Reads the 23 upstream `.drf` recordings of `tests/recordings/data/`
//! with the DRF reader alone (`add-recording-and-conformance`, task 3.5).
//!
//! Every header must parse, and every body must tokenise to its end with
//! the cursor: seed, runs up to the end marker, `EOL`, repeated while a
//! round follows. What remains after the last `EOL` is the trailer (`EOG`
//! and initials), which playback ignores. The per-file summary is the
//! table "DRF corpus" in `docs/conformance.md`; this test fails when the
//! table is stale.

use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use digger_core::recording::{DrfCursor, DrfRecording, DrfStep};

/// Number of `.drf` files in the corpus.
const DRF_FILES: usize = 23;
/// Upper bound on the records one body may give, so that a defect cannot
/// make the walk loop for ever (the longest game has 565330 ticks).
const MAX_RECORDS: u64 = 2_000_000;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One file's summary.
struct Summary {
    name: String,
    version: String,
    mode: String,
    rounds: u32,
    records: u64,
    trailer: String,
}

/// Walks one recording's body with the cursor alone.
fn walk(name: &str, bytes: &[u8]) -> Result<Summary, Box<dyn Error>> {
    let recording = DrfRecording::parse(bytes)?;
    let mut cursor = DrfCursor::new();
    let mut rounds = 0;
    let mut records: u64 = 0;
    while cursor.round_follows(&recording) {
        cursor.next_seed(&recording)?;
        rounds += 1;
        while let DrfStep::Record(_) = cursor.next_record(&recording)? {
            records += 1;
            if records > MAX_RECORDS {
                return Err(format!("{name}: more than {MAX_RECORDS} records").into());
            }
        }
        cursor
            .end_round(&recording)
            .map_err(|e| format!("{name}: round {rounds} out of step: {e:?}"))?;
    }
    if rounds == 0 {
        return Err(format!("{name}: no round").into());
    }
    let trailer = String::from_utf8(cursor.rest(&recording))?;
    Ok(Summary {
        name: name.to_owned(),
        version: recording.header().version_line().to_owned(),
        mode: recording.header().mode_line().to_owned(),
        rounds,
        records,
        trailer,
    })
}

/// The markdown table of all summaries.
fn table(summaries: &[Summary]) -> String {
    let mut out = String::from(
        "| File | Version line | Mode | Rounds | Records | Trailer |\n|---|---|---|---:|---:|---|\n",
    );
    for s in summaries {
        let trailer = if s.trailer.is_empty() {
            "(none)".to_owned()
        } else {
            format!("`{}`", s.trailer)
        };
        let _ = writeln!(
            out,
            "| `{}` | `{}` | `{}` | {} | {} | {} |",
            s.name, s.version, s.mode, s.rounds, s.records, trailer
        );
    }
    out
}

#[test]
fn corpus_drf_bodies_tokenise() -> Result<(), Box<dyn Error>> {
    let dir = repo_root().join("tests/recordings/data");
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    paths.retain(|p| p.extension().is_some_and(|e| e == "drf"));
    paths.sort();
    assert_eq!(paths.len(), DRF_FILES);

    let mut summaries = Vec::new();
    for path in &paths {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("file name")?
            .to_owned();
        let bytes = fs::read(path)?;
        summaries.push(walk(&name, &bytes)?);
    }

    let expected = table(&summaries);
    let doc = fs::read_to_string(repo_root().join("docs/conformance.md"))?;
    let doc = doc.replace("\r\n", "\n");
    assert!(
        doc.contains(&expected),
        "docs/conformance.md lacks the current DRF corpus table:\n{expected}"
    );
    Ok(())
}
