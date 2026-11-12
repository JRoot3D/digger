//! Guards the upstream recording corpus in `tests/recordings/` (spec
//! "Upstream recording corpus", design D11).
//!
//! Every recording and result file must have the SHA-256 recorded for it in
//! `MANIFEST.sha256`; a listed file must exist and no unlisted file may be
//! present. The corpus holds 23 `.drf`, 27 `.edrf` and 27 `.out` files, every
//! `.drf` has an `.edrf` twin, and every base name has exactly one `.out`
//! holding exactly one line. The manifest is parsed here independently of
//! `cargo xtask import-recordings`, which writes it.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use sha2::{Digest as _, Sha256};

/// The manifest's file name inside the corpus directory.
const MANIFEST: &str = "MANIFEST.sha256";
/// The corpus subdirectories the manifest describes.
const DIRS: [&str; 2] = ["data", "results"];

/// One problem found in a corpus directory, naming the file concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Problem {
    /// The file's SHA-256 differs from its manifest entry.
    Mismatch {
        path: String,
        expected: String,
        actual: String,
    },
    /// The manifest lists the file but it is not in the directory.
    Missing(String),
    /// The directory holds a file the manifest does not list.
    Unlisted(String),
    /// A manifest line that is not `<sha256>  <path>`.
    BadLine { line: usize, text: String },
    /// The manifest lists a path twice.
    Duplicate(String),
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "{path}: checksum mismatch: manifest {expected}, file {actual}"
            ),
            Self::Missing(path) => write!(f, "{path}: listed in {MANIFEST} but missing"),
            Self::Unlisted(path) => write!(f, "{path}: not listed in {MANIFEST}"),
            Self::BadLine { line, text } => {
                write!(f, "{MANIFEST}:{line}: malformed line {text:?}")
            }
            Self::Duplicate(path) => write!(f, "{path}: listed twice in {MANIFEST}"),
        }
    }
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/recordings")
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// Parses `<64 lowercase hex>  <path>` lines into path → digest.
fn parse_manifest(text: &str, problems: &mut Vec<Problem>) -> BTreeMap<String, String> {
    let mut entries = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        let parsed = line.split_once("  ").filter(|(digest, path)| {
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
                && !path.is_empty()
                && path.bytes().all(|b| b.is_ascii_graphic())
        });
        match parsed {
            Some((digest, path)) => {
                if entries.insert(path.to_owned(), digest.to_owned()).is_some() {
                    problems.push(Problem::Duplicate(path.to_owned()));
                }
            }
            None => problems.push(Problem::BadLine {
                line: index + 1,
                text: line.to_owned(),
            }),
        }
    }
    entries
}

/// The files of the corpus subdirectories, as `dir/name`, with contents.
fn corpus_files(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, Box<dyn Error>> {
    let mut files = BTreeMap::new();
    for dir in DIRS {
        for entry in std::fs::read_dir(root.join(dir))? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            files.insert(format!("{dir}/{name}"), std::fs::read(entry.path())?);
        }
    }
    Ok(files)
}

/// Every manifest problem of the corpus at `root`.
fn manifest_problems(root: &Path) -> Result<Vec<Problem>, Box<dyn Error>> {
    let mut problems = Vec::new();
    let manifest = std::fs::read_to_string(root.join(MANIFEST))?;
    let mut expected = parse_manifest(&manifest, &mut problems);
    for (path, bytes) in corpus_files(root)? {
        match expected.remove(&path) {
            Some(digest) => {
                let actual = sha256_hex(&bytes);
                if actual != digest {
                    problems.push(Problem::Mismatch {
                        path,
                        expected: digest,
                        actual,
                    });
                }
            }
            None => problems.push(Problem::Unlisted(path)),
        }
    }
    problems.extend(expected.into_keys().map(Problem::Missing));
    Ok(problems)
}

fn report(problems: &[Problem]) -> String {
    problems.iter().fold(String::new(), |mut text, problem| {
        let _ = writeln!(text, "  {problem}");
        text
    })
}

/// Base names of the files in `dir` ending in `.<suffix>`.
fn base_names(files: &BTreeMap<String, Vec<u8>>, dir: &str, suffix: &str) -> BTreeSet<String> {
    files
        .keys()
        .filter_map(|path| path.strip_prefix(dir)?.strip_prefix('/'))
        .filter_map(|name| name.strip_suffix(suffix)?.strip_suffix('.'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_file_matches_the_manifest() -> Result<(), Box<dyn Error>> {
    let problems = manifest_problems(&corpus_dir())?;
    assert!(
        problems.is_empty(),
        "tests/recordings/ differs from {MANIFEST} (re-run `cargo xtask import-recordings`, \
         never edit by hand):\n{}",
        report(&problems)
    );
    Ok(())
}

#[test]
fn corpus_has_23_drf_27_edrf_and_27_results() -> Result<(), Box<dyn Error>> {
    let files = corpus_files(&corpus_dir())?;
    let drf = base_names(&files, "data", "drf");
    let edrf = base_names(&files, "data", "edrf");
    let out = base_names(&files, "results", "out");
    assert_eq!(
        (drf.len(), edrf.len(), out.len()),
        (23, 27, 27),
        "counts of .drf, .edrf, .out"
    );
    assert_eq!(
        files.len(),
        23 + 27 + 27,
        "data/ and results/ hold only corpus files: {:?}",
        files.keys().collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn every_drf_has_an_edrf_twin() -> Result<(), Box<dyn Error>> {
    let files = corpus_files(&corpus_dir())?;
    let edrf = base_names(&files, "data", "edrf");
    let orphans: Vec<String> = base_names(&files, "data", "drf")
        .difference(&edrf)
        .cloned()
        .collect();
    assert!(
        orphans.is_empty(),
        ".drf files without .edrf twin: {orphans:?}"
    );
    Ok(())
}

#[test]
fn every_base_name_has_one_single_line_result() -> Result<(), Box<dyn Error>> {
    let files = corpus_files(&corpus_dir())?;
    let recordings: BTreeSet<String> = base_names(&files, "data", "drf")
        .union(&base_names(&files, "data", "edrf"))
        .cloned()
        .collect();
    let results = base_names(&files, "results", "out");
    assert_eq!(recordings, results, "base names of recordings and results");
    for name in &results {
        let path = format!("results/{name}.out");
        let bytes = files.get(&path).ok_or_else(|| format!("{path}: missing"))?;
        let text = std::str::from_utf8(bytes)?;
        let body = text
            .strip_suffix('\n')
            .ok_or_else(|| format!("{path}: does not end in a line feed"))?;
        assert!(
            !body.is_empty() && !body.contains(['\n', '\r']),
            "{path}: is not exactly one line: {text:?}"
        );
    }
    Ok(())
}

/// A copy of the corpus in a fresh temporary directory, removed on drop.
struct TempCorpus(PathBuf);

impl TempCorpus {
    fn copy_of(root: &Path) -> Result<Self, Box<dyn Error>> {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "digger-corpus-integrity-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let copy = Self(dir);
        for sub in DIRS {
            std::fs::create_dir_all(copy.0.join(sub))?;
        }
        std::fs::copy(root.join(MANIFEST), copy.0.join(MANIFEST))?;
        for (path, bytes) in corpus_files(root)? {
            std::fs::write(copy.0.join(path), bytes)?;
        }
        Ok(copy)
    }
}

impl Drop for TempCorpus {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn one_changed_byte_is_a_mismatch_naming_the_file() -> Result<(), Box<dyn Error>> {
    let copy = TempCorpus::copy_of(&corpus_dir())?;
    assert_eq!(manifest_problems(&copy.0)?, []);

    let victim = copy.0.join("data/muu27700.drf");
    let mut bytes = std::fs::read(&victim)?;
    let last = bytes.last_mut().ok_or("empty recording")?;
    *last ^= 0x01;
    std::fs::write(&victim, bytes)?;

    let problems = manifest_problems(&copy.0)?;
    assert!(
        matches!(problems.as_slice(), [Problem::Mismatch { path, .. }] if path == "data/muu27700.drf"),
        "{problems:?}"
    );
    let message = report(&problems);
    assert!(
        message.contains("data/muu27700.drf: checksum mismatch"),
        "{message}"
    );

    std::fs::write(copy.0.join("results/extra.out"), b"x\n")?;
    std::fs::remove_file(copy.0.join("results/dror0.out"))?;
    let problems = manifest_problems(&copy.0)?;
    assert!(problems.contains(&Problem::Unlisted("results/extra.out".to_owned())));
    assert!(problems.contains(&Problem::Missing("results/dror0.out".to_owned())));
    Ok(())
}

#[test]
fn malformed_manifest_lines_are_reported() {
    let mut problems = Vec::new();
    let digest = "a".repeat(64);
    let entries = parse_manifest(
        &format!("{digest}  data/a.drf\nnot a line\n{digest}  data/a.drf\n"),
        &mut problems,
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(
        problems,
        [
            Problem::BadLine {
                line: 2,
                text: "not a line".to_owned()
            },
            Problem::Duplicate("data/a.drf".to_owned()),
        ]
    );
}
