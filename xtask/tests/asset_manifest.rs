//! Guards the committed asset blobs against accidental change (spec
//! "Committed asset integrity", design D8).
//!
//! Every file in `assets/` must have the SHA-256 digest and length recorded
//! for it in `assets/manifest.txt`; a listed file must exist and no unlisted
//! file may be present. The only files not described by the manifest are
//! those in [`EXEMPT`]. The manifest is parsed here independently of the
//! importer that writes it, so a bug in the writer cannot hide itself.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

/// Files in `assets/` that the manifest does not describe: the hand-written
/// README and the manifest itself.
const EXEMPT: [&str; 2] = ["README.md", "manifest.txt"];

/// The manifest's file name inside the asset directory.
const MANIFEST: &str = "manifest.txt";

/// One problem found in an asset directory, naming the file concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Problem {
    /// The file's length or digest differs from its manifest entry.
    Mismatch { name: String, detail: String },
    /// The manifest lists the file but it is not in the directory.
    Missing(String),
    /// The directory holds a file the manifest does not list.
    Unlisted(String),
    /// A manifest line that is not `<sha256>  <length>  <name>`.
    BadLine { line: usize, text: String },
}

/// A manifest entry: expected digest and length.
struct Expected {
    sha256: String,
    len: u64,
}

fn asset_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// Parses the manifest text; comment and blank lines are skipped.
fn parse(text: &str, problems: &mut Vec<Problem>) -> BTreeMap<String, Expected> {
    let mut entries = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let parsed = match fields.as_slice() {
            [sha256, len, name]
                if sha256.len() == 64
                    && sha256
                        .bytes()
                        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) =>
            {
                len.parse().ok().map(|len| {
                    (
                        (*name).to_owned(),
                        Expected {
                            sha256: (*sha256).to_owned(),
                            len,
                        },
                    )
                })
            }
            _ => None,
        };
        match parsed {
            Some((name, expected)) if !entries.contains_key(&name) => {
                entries.insert(name, expected);
            }
            _ => problems.push(Problem::BadLine {
                line: index + 1,
                text: line.to_owned(),
            }),
        }
    }
    entries
}

/// Compares every file in `dir` with `dir/manifest.txt`.
fn verify(dir: &Path) -> Result<Vec<Problem>, Box<dyn Error>> {
    let mut problems = Vec::new();
    let manifest = std::fs::read_to_string(dir.join(MANIFEST))?;
    let mut expected = parse(&manifest, &mut problems);

    let mut names: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    names.sort();

    for name in names {
        if EXEMPT.contains(&name.as_str()) {
            continue;
        }
        let Some(want) = expected.remove(&name) else {
            problems.push(Problem::Unlisted(name));
            continue;
        };
        let path = dir.join(&name);
        if !path.is_file() {
            problems.push(Problem::Mismatch {
                name,
                detail: "not a regular file".to_owned(),
            });
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let len = u64::try_from(bytes.len())?;
        let sha256 = sha256_hex(&bytes);
        if len != want.len {
            problems.push(Problem::Mismatch {
                name,
                detail: format!("length {len}, manifest says {}", want.len),
            });
        } else if sha256 != want.sha256 {
            problems.push(Problem::Mismatch {
                name,
                detail: format!("SHA-256 {sha256}, manifest says {}", want.sha256),
            });
        }
    }
    problems.extend(expected.into_keys().map(Problem::Missing));
    Ok(problems)
}

/// Formats the problems as one line each, for the assertion message.
fn report(problems: &[Problem]) -> String {
    problems.iter().fold(String::new(), |mut text, problem| {
        let _ = match problem {
            Problem::Mismatch { name, detail } => writeln!(text, "  {name}: {detail}"),
            Problem::Missing(name) => writeln!(text, "  {name}: listed in manifest but missing"),
            Problem::Unlisted(name) => writeln!(text, "  {name}: not listed in manifest"),
            Problem::BadLine { line, text: bad } => {
                writeln!(text, "  {MANIFEST} line {line}: malformed: {bad}")
            }
        };
        text
    })
}

#[test]
fn committed_assets_match_manifest() -> Result<(), Box<dyn Error>> {
    let dir = asset_dir();
    let problems = verify(&dir)?;
    assert!(
        problems.is_empty(),
        "assets in {} differ from {MANIFEST}; re-run `cargo xtask import-assets` \
         or restore the files:\n{}",
        dir.display(),
        report(&problems)
    );
    Ok(())
}

#[test]
fn manifest_lists_the_imported_files() -> Result<(), Box<dyn Error>> {
    let mut problems = Vec::new();
    let manifest = std::fs::read_to_string(asset_dir().join(MANIFEST))?;
    let entries = parse(&manifest, &mut problems);
    assert!(problems.is_empty(), "{}", report(&problems));
    let names: Vec<&str> = entries.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            "font.bin",
            "icon.rgba",
            "index.rs",
            "levels.bin",
            "music.bin",
            "sprites.bin",
            "title.zz"
        ]
    );
    assert!(manifest.contains("638125eecc2a36629b886f58d7b75253894cf3d3"));
    Ok(())
}

/// A scratch copy of an asset directory, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Result<Self, Box<dyn Error>> {
        let dir = std::env::temp_dir().join(format!(
            "digger-asset-manifest-{tag}-{}",
            std::process::id()
        ));
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("a.bin"), b"abc")?;
        std::fs::write(dir.join("b.bin"), b"")?;
        std::fs::write(dir.join("README.md"), b"hand written")?;
        std::fs::write(
            dir.join(MANIFEST),
            "# test manifest\n\
             ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  3  a.bin\n\
             e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  0  b.bin\n",
        )?;
        Ok(Self(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn verify_accepts_matching_directory() -> Result<(), Box<dyn Error>> {
    let scratch = Scratch::new("ok")?;
    assert_eq!(verify(&scratch.0)?, []);
    Ok(())
}

#[test]
fn verify_names_changed_file() -> Result<(), Box<dyn Error>> {
    let scratch = Scratch::new("changed")?;
    std::fs::write(scratch.0.join("a.bin"), b"abd")?;
    let problems = verify(&scratch.0)?;
    assert!(
        matches!(problems.as_slice(), [Problem::Mismatch { name, .. }] if name == "a.bin"),
        "{problems:?}"
    );
    assert!(report(&problems).contains("a.bin: SHA-256"));
    Ok(())
}

#[test]
fn verify_names_resized_file() -> Result<(), Box<dyn Error>> {
    let scratch = Scratch::new("resized")?;
    std::fs::write(scratch.0.join("b.bin"), b"x")?;
    let problems = verify(&scratch.0)?;
    assert!(
        matches!(problems.as_slice(), [Problem::Mismatch { name, detail }]
            if name == "b.bin" && detail.starts_with("length 1")),
        "{problems:?}"
    );
    Ok(())
}

#[test]
fn verify_names_missing_and_unlisted_files() -> Result<(), Box<dyn Error>> {
    let scratch = Scratch::new("missing")?;
    std::fs::remove_file(scratch.0.join("b.bin"))?;
    std::fs::write(scratch.0.join("stray.bin"), b"?")?;
    assert_eq!(
        verify(&scratch.0)?,
        [
            Problem::Unlisted("stray.bin".to_owned()),
            Problem::Missing("b.bin".to_owned()),
        ]
    );
    Ok(())
}

#[test]
fn verify_rejects_malformed_manifest_line() -> Result<(), Box<dyn Error>> {
    let scratch = Scratch::new("malformed")?;
    let path = scratch.0.join(MANIFEST);
    let mut text = std::fs::read_to_string(&path)?;
    text.push_str("abc 3 c.bin\n");
    std::fs::write(&path, text)?;
    assert_eq!(
        verify(&scratch.0)?,
        [Problem::BadLine {
            line: 4,
            text: "abc 3 c.bin".to_owned()
        }]
    );
    Ok(())
}
