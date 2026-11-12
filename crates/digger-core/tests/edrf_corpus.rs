//! Reads the 27 upstream `.edrf` recordings of `tests/recordings/data/`
//! with the eDRF reader (`add-recording-and-conformance`, task 4.4), and
//! checks that `docs/recording-formats.md` carries both grammars of the
//! game-recording spec.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use digger_core::SlotId;
use digger_core::recording::{EdrfRecording, EndRecord};

/// Number of `.edrf` files in the corpus.
const EDRF_FILES: usize = 27;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `ticks=` value of an expected-result line.
fn expected_ticks(result: &str) -> Option<u32> {
    result
        .split_whitespace()
        .find_map(|field| field.strip_prefix("ticks="))
        .and_then(|ticks| ticks.parse().ok())
}

#[test]
fn corpus_edrf_parse() -> Result<(), Box<dyn Error>> {
    let root = repo_root().join("tests/recordings");
    let mut names: Vec<PathBuf> = fs::read_dir(root.join("data"))?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    names.retain(|path| path.extension().is_some_and(|ext| ext == "edrf"));
    names.sort();
    assert_eq!(names.len(), EDRF_FILES);
    for path in &names {
        let base = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("file name")?;
        let recording =
            EdrfRecording::parse(&fs::read(path)?).map_err(|e| format!("{base}.edrf: {e}"))?;
        let result = fs::read_to_string(root.join("results").join(format!("{base}.out")))?;
        let end = recording.end().ok_or(format!("{base}: no Z record"))?;
        assert_eq!(Some(end.ticks), expected_ticks(&result), "{base}");
        assert!(!recording.truncated(), "{base}");
        assert!(!recording.seeds().is_empty(), "{base}: no seed");
        assert!(!recording.levels().is_empty(), "{base}: no level");
        // Every slot holds one value per tick of the game.
        for slot in SlotId::ALL {
            assert_eq!(
                recording.controls(slot).remaining(),
                u64::from(end.ticks),
                "{base}: slot {slot:?}"
            );
        }
        // One tick checkpoint per 256 ticks.
        assert_eq!(
            recording.checkpoints().len(),
            usize::try_from(end.ticks / 256)?,
            "{base}"
        );
        // One round-end hash per round.
        assert_eq!(
            recording.round_ends().len(),
            recording.seeds().len(),
            "{base}"
        );
        // The two games that were quit (`2riptest01` at tick 1024).
        if base == "2riptest01" || base == "2test03" {
            assert_eq!(recording.quit(), SlotId::new(0), "{base}");
        } else {
            assert_eq!(recording.quit(), None, "{base}");
        }
    }
    let riptest = EdrfRecording::parse(&fs::read(root.join("data/2riptest01.edrf"))?)?;
    assert!(matches!(riptest.end(), Some(EndRecord { ticks: 1024, .. })));
    Ok(())
}

/// The lines of the fenced code block that follows `marker` in `text`.
fn grammar_after<'a>(text: &'a str, marker: &str) -> Option<Vec<&'a str>> {
    let start = text.find(marker)?;
    let rest = text.get(start..)?;
    let open = rest.find("```\n")?;
    let body = rest.get(open + 4..)?;
    let close = body.find("```")?;
    Some(
        body.get(..close)?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect(),
    )
}

/// Collapses runs of white space, so alignment does not matter.
fn normalise(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn recording_formats_doc_has_the_spec_grammars() -> Result<(), Box<dyn Error>> {
    let root = repo_root();
    // The spec lives in the change until it is archived.
    let spec = [
        "openspec/changes/add-recording-and-conformance/specs/game-recording/spec.md",
        "openspec/specs/game-recording/spec.md",
    ]
    .iter()
    .find_map(|path| fs::read_to_string(root.join(path)).ok())
    .ok_or("game-recording spec not found")?;
    let doc = fs::read_to_string(root.join("docs/recording-formats.md"))?;
    let doc_lines: Vec<String> = doc.lines().map(normalise).collect();
    for requirement in [
        "### Requirement: DRF body grammar",
        "### Requirement: eDRF record grammar",
    ] {
        let section = spec
            .find(requirement)
            .and_then(|start| spec.get(start..))
            .ok_or(requirement)?;
        let productions = grammar_after(section, "#### Scenario: Grammar").ok_or(requirement)?;
        assert!(productions.len() >= 6, "{requirement}");
        for production in productions {
            let wanted = normalise(production);
            assert!(
                doc_lines.contains(&wanted),
                "docs/recording-formats.md lacks the production `{production}`"
            );
        }
    }
    Ok(())
}
