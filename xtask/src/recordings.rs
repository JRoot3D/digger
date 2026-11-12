//! `cargo xtask import-recordings`: copy upstream's recording corpus into
//! `tests/recordings/` byte-for-byte (design D10, D11; spec "Upstream
//! recording corpus").
//!
//! The recordings are copied as data: `tests/data/*.drf` and `*.edrf` go to
//! `data/`, `tests/results/*.out` to `results/`, and `MANIFEST.sha256` lists
//! the SHA-256 of every copied file in the format `sha256sum -c` reads. The
//! checkout's `HEAD` is read from its `.git` directory (no `git` process) and
//! must be the pinned commit unless `--allow-other-commit` is given. Every
//! check and every read happens before the first write, so a refused import
//! changes nothing.

use std::fmt::{self, Write as _};
use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;

use crate::import::manifest::{UPSTREAM_COMMIT, sha256_hex};

/// Arguments of `cargo xtask import-recordings`.
#[derive(Debug, Args)]
pub(crate) struct ImportRecordingsArgs {
    /// Checkout of upstream sobomax/digger at the pinned commit.
    #[arg(long, value_name = "DIR")]
    pub(crate) from: PathBuf,

    /// Import even if the checkout's HEAD is not the pinned commit.
    #[arg(long)]
    pub(crate) allow_other_commit: bool,
}

/// Files read for the import: name and contents, sorted by name.
type Files = Vec<(String, Vec<u8>)>;

/// The manifest's file name inside the corpus directory.
pub(crate) const MANIFEST: &str = "MANIFEST.sha256";
/// The corpus subdirectory holding the recordings.
pub(crate) const DATA_DIR: &str = "data";
/// The corpus subdirectory holding the expected results.
pub(crate) const RESULTS_DIR: &str = "results";

/// Why `cargo xtask import-recordings` failed.
#[derive(Debug)]
pub(crate) enum RecordingsError {
    /// The checkout has no readable `.git` metadata or `HEAD`.
    NoRepository { path: PathBuf, detail: String },
    /// `HEAD` is not the pinned commit and `--allow-other-commit` was not given.
    WrongCommit { found: String },
    /// The checkout holds no recording or no expected-result file.
    Empty { dir: PathBuf },
    /// A file name that cannot be stored safely in the manifest.
    BadName { path: PathBuf },
    /// Reading or writing a file failed.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for RecordingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRepository { path, detail } => write!(
                f,
                "{}: cannot read the checkout's HEAD ({detail}); is --from a git checkout of sobomax/digger?",
                path.display()
            ),
            Self::WrongCommit { found } => write!(
                f,
                "upstream checkout is at {found}, expected {UPSTREAM_COMMIT}; \
                 nothing was changed (pass --allow-other-commit to import anyway)"
            ),
            Self::Empty { dir } => write!(f, "{}: no files to import", dir.display()),
            Self::BadName { path } => write!(
                f,
                "{}: file name is not plain printable ASCII without spaces",
                path.display()
            ),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for RecordingsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> RecordingsError + '_ {
    move |source| RecordingsError::Io {
        path: path.to_owned(),
        source,
    }
}

/// What an import copied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Summary {
    /// The commit the checkout's `HEAD` resolved to.
    pub(crate) commit: String,
    pub(crate) drf: usize,
    pub(crate) edrf: usize,
    pub(crate) results: usize,
}

/// Runs `cargo xtask import-recordings` into `tests/recordings/`.
pub(crate) fn run(args: &ImportRecordingsArgs) -> Result<(), RecordingsError> {
    let out = corpus_dir();
    let summary = import(&args.from, &out, args.allow_other_commit)?;
    println!(
        "imported {} .drf, {} .edrf and {} .out files from {} into {}",
        summary.drf,
        summary.edrf,
        summary.results,
        summary.commit,
        out.display()
    );
    Ok(())
}

/// The corpus directory: `tests/recordings/` in the workspace root.
pub(crate) fn corpus_dir() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap_or(manifest_dir)
        .join("tests")
        .join("recordings")
}

/// Copies the corpus of the checkout `from` into `out`.
///
/// Files in `out/data` and `out/results` with a corpus suffix that the
/// checkout no longer has are removed; other files (the README) are kept.
pub(crate) fn import(
    from: &Path,
    out: &Path,
    allow_other_commit: bool,
) -> Result<Summary, RecordingsError> {
    let commit = head_commit(from)?;
    if commit != UPSTREAM_COMMIT && !allow_other_commit {
        return Err(RecordingsError::WrongCommit { found: commit });
    }

    let data = read_files(&from.join("tests").join("data"), &["drf", "edrf"])?;
    let results = read_files(&from.join("tests").join("results"), &["out"])?;
    let count = |suffix: &str| {
        data.iter()
            .filter(|(name, _)| name.ends_with(&format!(".{suffix}")))
            .count()
    };
    let summary = Summary {
        drf: count("drf"),
        edrf: count("edrf"),
        results: results.len(),
        commit,
    };

    let mut manifest = String::new();
    for (dir, files) in [(DATA_DIR, &data), (RESULTS_DIR, &results)] {
        for (name, bytes) in files {
            let _ = writeln!(manifest, "{}  {dir}/{name}", sha256_hex(bytes));
        }
    }

    // Every read and check is done; from here on the import writes.
    write_dir(&out.join(DATA_DIR), &data, &["drf", "edrf"])?;
    write_dir(&out.join(RESULTS_DIR), &results, &["out"])?;
    let manifest_path = out.join(MANIFEST);
    fs::write(&manifest_path, manifest).map_err(io(&manifest_path))?;
    Ok(summary)
}

/// Whether `name` ends in `.<suffix>` for one of `suffixes`.
fn has_suffix(name: &str, suffixes: &[&str]) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(stem, ext)| !stem.is_empty() && suffixes.contains(&ext))
}

/// The regular files of `dir` with one of `suffixes`, sorted by name.
fn read_files(dir: &Path, suffixes: &[&str]) -> Result<Files, RecordingsError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).map_err(io(dir))? {
        let entry = entry.map_err(io(dir))?;
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !has_suffix(&name, suffixes) || !path.is_file() {
            continue;
        }
        if !name.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(RecordingsError::BadName { path });
        }
        let bytes = fs::read(&path).map_err(io(&path))?;
        files.push((name, bytes));
    }
    if files.is_empty() {
        return Err(RecordingsError::Empty {
            dir: dir.to_owned(),
        });
    }
    files.sort();
    Ok(files)
}

/// Writes `files` into `dir` and removes the stale files with `suffixes`.
fn write_dir(
    dir: &Path,
    files: &[(String, Vec<u8>)],
    suffixes: &[&str],
) -> Result<(), RecordingsError> {
    fs::create_dir_all(dir).map_err(io(dir))?;
    for entry in fs::read_dir(dir).map_err(io(dir))? {
        let entry = entry.map_err(io(dir))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if has_suffix(name, suffixes) && !files.iter().any(|(kept, _)| kept == name) {
            let path = entry.path();
            fs::remove_file(&path).map_err(io(&path))?;
        }
    }
    for (name, bytes) in files {
        let path = dir.join(name);
        fs::write(&path, bytes).map_err(io(&path))?;
    }
    Ok(())
}

/// The commit `HEAD` of the checkout at `checkout` names, read from its git
/// metadata: a `.git` directory or a `.git` file pointing at one (worktrees
/// and submodules), loose refs, then `packed-refs`, also in the common
/// directory of a worktree.
pub(crate) fn head_commit(checkout: &Path) -> Result<String, RecordingsError> {
    let no_repo = |detail: String| RecordingsError::NoRepository {
        path: checkout.to_owned(),
        detail,
    };
    let dot_git = checkout.join(".git");
    let git_dir = if dot_git.is_file() {
        let text = fs::read_to_string(&dot_git).map_err(|e| no_repo(e.to_string()))?;
        let target = text
            .trim()
            .strip_prefix("gitdir:")
            .ok_or_else(|| no_repo(".git file has no `gitdir:` line".to_owned()))?
            .trim();
        checkout.join(target)
    } else if dot_git.is_dir() {
        dot_git
    } else {
        return Err(no_repo("no .git".to_owned()));
    };
    let common_dir = match fs::read_to_string(git_dir.join("commondir")) {
        Ok(text) => git_dir.join(text.trim()),
        Err(_) => git_dir.clone(),
    };

    let head = fs::read_to_string(git_dir.join("HEAD")).map_err(|e| no_repo(e.to_string()))?;
    let head = head.trim();
    let Some(reference) = head.strip_prefix("ref:").map(str::trim) else {
        return object_id(head).ok_or_else(|| no_repo(format!("HEAD holds `{head}`")));
    };
    for dir in [&git_dir, &common_dir] {
        if let Ok(text) = fs::read_to_string(dir.join(reference)) {
            return object_id(text.trim())
                .ok_or_else(|| no_repo(format!("{reference} holds `{}`", text.trim())));
        }
    }
    for dir in [&git_dir, &common_dir] {
        let Ok(text) = fs::read_to_string(dir.join("packed-refs")) else {
            continue;
        };
        let found = text
            .lines()
            .filter(|line| !line.starts_with('#') && !line.starts_with('^'))
            .filter_map(|line| line.split_once(' '))
            .find(|(_, name)| name.trim() == reference);
        if let Some((id, _)) = found {
            return object_id(id).ok_or_else(|| no_repo(format!("packed {reference} is `{id}`")));
        }
    }
    Err(no_repo(format!("{reference} does not resolve")))
}

/// `text` as a lowercase 40-digit object id, if it is one.
fn object_id(text: &str) -> Option<String> {
    (text.len() == 40 && text.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| text.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;
    use std::path::Path;

    use super::{Files, MANIFEST, RecordingsError, head_commit, import};
    use crate::import::fixture::TempDir;
    use crate::import::manifest::{UPSTREAM_COMMIT, sha256_hex};

    const OTHER_COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    /// A fake upstream checkout at `commit` with a CR LF `.drf`, its `.edrf`
    /// twin, an `.edrf` without twin, two results and files that must be
    /// left behind.
    fn fake_checkout(tag: &str, commit: &str) -> Result<TempDir, Box<dyn Error>> {
        let dir = TempDir::new(tag)?;
        let root = dir.path();
        fs::create_dir_all(root.join(".git/refs/heads"))?;
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/master\n")?;
        fs::write(root.join(".git/refs/heads/master"), format!("{commit}\n"))?;
        fs::create_dir_all(root.join("tests/data"))?;
        fs::create_dir_all(root.join("tests/results"))?;
        fs::write(root.join("tests/data/a.drf"), b"DRF\r\nx\r\n")?;
        fs::write(root.join("tests/data/a.edrf"), b"eDRF 2\n\x00\xff")?;
        fs::write(root.join("tests/data/b.edrf"), b"eDRF 2\nb\n")?;
        fs::write(root.join("tests/data/notes.txt"), b"not a recording")?;
        fs::write(
            root.join("tests/results/a.out"),
            b"score=1 level=1 ticks=2\n",
        )?;
        fs::write(
            root.join("tests/results/b.out"),
            b"score=3 level=1 ticks=4\n",
        )?;
        Ok(dir)
    }

    /// Every file below `root` with its contents, by `/`-separated path.
    fn file_tree(root: &Path) -> Result<Files, Box<dyn Error>> {
        let mut files = Vec::new();
        if root.exists() {
            for entry in fs::read_dir(root)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = entry.path();
                if path.is_dir() {
                    for (inner, bytes) in file_tree(&path)? {
                        files.push((format!("{name}/{inner}"), bytes));
                    }
                } else {
                    files.push((name, fs::read(&path)?));
                }
            }
        }
        files.sort();
        Ok(files)
    }

    #[test]
    fn right_commit_copies_byte_for_byte_and_writes_manifest() -> Result<(), Box<dyn Error>> {
        let upstream = fake_checkout("rec-ok", UPSTREAM_COMMIT)?;
        let out = TempDir::new("rec-ok-out")?;
        fs::write(out.path().join("README.md"), b"kept")?;
        fs::create_dir_all(out.path().join("data"))?;
        fs::write(out.path().join("data/stale.drf"), b"old")?;

        let summary = import(upstream.path(), out.path(), false)?;
        assert_eq!(summary.commit, UPSTREAM_COMMIT);
        assert_eq!((summary.drf, summary.edrf, summary.results), (1, 2, 2));

        let tree = file_tree(out.path())?;
        let names: Vec<&str> = tree.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                MANIFEST,
                "README.md",
                "data/a.drf",
                "data/a.edrf",
                "data/b.edrf",
                "results/a.out",
                "results/b.out",
            ]
        );
        for (name, bytes) in &tree {
            if let Some(rel) = name.strip_prefix("data/") {
                assert_eq!(
                    bytes,
                    &fs::read(upstream.path().join("tests/data").join(rel))?
                );
            }
            if let Some(rel) = name.strip_prefix("results/") {
                assert_eq!(
                    bytes,
                    &fs::read(upstream.path().join("tests/results").join(rel))?
                );
            }
        }
        assert_eq!(fs::read(out.path().join("data/a.drf"))?, b"DRF\r\nx\r\n");

        let manifest = fs::read_to_string(out.path().join(MANIFEST))?;
        let expected: String = tree
            .iter()
            .filter(|(name, _)| name.starts_with("data/") || name.starts_with("results/"))
            .fold(String::new(), |mut text, (name, bytes)| {
                text.push_str(&sha256_hex(bytes));
                text.push_str("  ");
                text.push_str(name);
                text.push('\n');
                text
            });
        assert_eq!(manifest, expected);
        Ok(())
    }

    #[test]
    fn wrong_commit_refuses_and_changes_nothing() -> Result<(), Box<dyn Error>> {
        let upstream = fake_checkout("rec-wrong", OTHER_COMMIT)?;
        let out = TempDir::new("rec-wrong-out")?;
        fs::write(out.path().join("README.md"), b"kept")?;
        fs::create_dir_all(out.path().join("data"))?;
        fs::write(out.path().join("data/old.drf"), b"old")?;
        fs::write(out.path().join(MANIFEST), b"old manifest")?;
        let before = file_tree(out.path())?;

        let error = import(upstream.path(), out.path(), false)
            .err()
            .ok_or("import from the wrong commit succeeded")?;
        assert!(
            matches!(&error, RecordingsError::WrongCommit { found } if found == OTHER_COMMIT),
            "{error}"
        );
        assert!(error.to_string().contains(UPSTREAM_COMMIT));
        assert_eq!(file_tree(out.path())?, before);

        let summary = import(upstream.path(), out.path(), true)?;
        assert_eq!(summary.commit, OTHER_COMMIT);
        assert!(!out.path().join("data/old.drf").exists());
        Ok(())
    }

    #[test]
    fn head_resolves_detached_packed_and_worktree() -> Result<(), Box<dyn Error>> {
        let detached = TempDir::new("rec-head-detached")?;
        fs::create_dir_all(detached.path().join(".git"))?;
        fs::write(
            detached.path().join(".git/HEAD"),
            format!("{}\n", UPSTREAM_COMMIT.to_ascii_uppercase()),
        )?;
        assert_eq!(head_commit(detached.path())?, UPSTREAM_COMMIT);

        let packed = TempDir::new("rec-head-packed")?;
        fs::create_dir_all(packed.path().join(".git"))?;
        fs::write(packed.path().join(".git/HEAD"), "ref: refs/heads/master\n")?;
        fs::write(
            packed.path().join(".git/packed-refs"),
            format!(
                "# pack-refs with: peeled fully-peeled sorted\n\
                 {OTHER_COMMIT} refs/heads/other\n\
                 {UPSTREAM_COMMIT} refs/heads/master\n^{OTHER_COMMIT}\n"
            ),
        )?;
        assert_eq!(head_commit(packed.path())?, UPSTREAM_COMMIT);

        let worktree = TempDir::new("rec-head-worktree")?;
        let main_git = worktree.path().join("main/.git");
        let wt_git = main_git.join("worktrees/wt");
        fs::create_dir_all(&wt_git)?;
        fs::create_dir_all(worktree.path().join("wt"))?;
        fs::write(
            worktree.path().join("wt/.git"),
            "gitdir: ../main/.git/worktrees/wt\n",
        )?;
        fs::write(wt_git.join("HEAD"), "ref: refs/heads/feature\n")?;
        fs::write(wt_git.join("commondir"), "../..\n")?;
        fs::create_dir_all(main_git.join("refs/heads"))?;
        fs::write(
            main_git.join("refs/heads/feature"),
            format!("{OTHER_COMMIT}\n"),
        )?;
        assert_eq!(head_commit(&worktree.path().join("wt"))?, OTHER_COMMIT);

        let none = TempDir::new("rec-head-none")?;
        assert!(matches!(
            head_commit(none.path()),
            Err(RecordingsError::NoRepository { .. })
        ));
        let out = TempDir::new("rec-head-none-out")?;
        assert!(import(none.path(), out.path(), true).is_err());
        assert_eq!(file_tree(out.path())?, Files::new());
        Ok(())
    }
}
