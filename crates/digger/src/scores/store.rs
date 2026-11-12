//! The `scores.toml` version 1 schema, loading with recovery and atomic
//! saving (design D10, `high-scores` spec: score file, safe saving,
//! missing, damaged or newer score file).
//!
//! ```toml
//! format = 1
//!
//! [[table]]
//! levels = "builtin"
//! kind = "normal-1"
//! entries = [
//!   { initials = "MSX", score = 27700 },
//!   …ten entries, highest first
//! ]
//! ```
//!
//! The score file never stops the program: a file that cannot be read,
//! parsed or understood gives fresh tables and one notice, and is renamed
//! to `scores.toml.bad` before the next save so that it is not lost.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the store is opened by main and written by the initials scene of later task groups"
    )
)]

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{Entry, LevelSet, ScoreTable, Scores, TABLE_LEN, TableKey, TableKind};
use crate::report;

/// The format version this program reads and writes.
pub(crate) const FORMAT: i64 = 1;

/// The `levels` value of the built-in level set's tables.
pub(crate) const BUILTIN_LEVELS: &str = "builtin";

/// The first line of every score file the program writes.
pub(crate) const HEADER: &str = "# Digger high scores, written by the game.";

/// What loading found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoadStatus {
    /// There is no score file: every table is fresh.
    Fresh,
    /// The file was read.
    Loaded,
    /// The file could not be used; every table is fresh and the file is
    /// renamed to `scores.toml.bad` before the next save.
    Recovered {
        /// Why the file was not used.
        reason: String,
    },
}

/// Reads the score file at `path`. Never fails: a missing file is
/// [`LoadStatus::Fresh`], an unusable one [`LoadStatus::Recovered`], and
/// both give fresh tables.
pub(crate) fn load(path: &Path) -> (Scores, LoadStatus) {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return (Scores::default(), LoadStatus::Fresh);
        }
        Err(error) => return recovered(format!("cannot be read: {error}")),
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return recovered("is not UTF-8 text".to_owned());
    };
    match parse(&text) {
        Ok(scores) => (scores, LoadStatus::Loaded),
        Err(reason) => recovered(reason),
    }
}

fn recovered(reason: String) -> (Scores, LoadStatus) {
    (Scores::default(), LoadStatus::Recovered { reason })
}

/// The file as written by format 1, without its `format` key (checked
/// first).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileV1 {
    #[serde(default)]
    table: Vec<TableV1>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TableV1 {
    levels: String,
    kind: String,
    entries: Vec<EntryV1>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryV1 {
    initials: String,
    score: u32,
}

/// Parses the text of a score file; the error says what is wrong.
fn parse(text: &str) -> Result<Scores, String> {
    let mut document: toml::Table = text
        .parse()
        .map_err(|error: toml::de::Error| format!("not valid TOML: {}", error.message()))?;
    match document.get("format").and_then(toml::Value::as_integer) {
        Some(FORMAT) => {}
        Some(newer) if newer > FORMAT => {
            return Err(format!(
                "written in format {newer} by a newer version of the game; this version reads format {FORMAT}"
            ));
        }
        Some(other) => return Err(format!("unknown format {other}")),
        None => return Err("does not declare its format".to_owned()),
    }
    document.remove("format");
    let file: FileV1 = toml::Value::Table(document)
        .try_into()
        .map_err(|error: toml::de::Error| format!("wrong structure: {}", error.message()))?;
    let mut scores = Scores::default();
    let mut seen = BTreeSet::new();
    for (n, table) in file.table.into_iter().enumerate() {
        let n = n + 1;
        let kind = TableKind::from_name(&table.kind)
            .ok_or_else(|| format!("table {n}: unknown kind {:?}", table.kind))?;
        let levels = if table.levels == BUILTIN_LEVELS {
            LevelSet::Builtin
        } else if table.levels.is_empty() {
            return Err(format!("table {n}: empty levels name"));
        } else {
            LevelSet::Custom(table.levels)
        };
        let key = TableKey { levels, kind };
        if !seen.insert(key.clone()) {
            return Err(format!(
                "table {n}: a second {kind} table for the same levels"
            ));
        }
        let count = table.entries.len();
        let entries: [EntryV1; TABLE_LEN] = table
            .entries
            .try_into()
            .map_err(|_| format!("table {n}: {count} entries, a table has {TABLE_LEN}"))?;
        let mut parsed = [Entry::EMPTY; TABLE_LEN];
        for (slot, entry) in parsed.iter_mut().zip(entries) {
            let initials: [u8; 3] = entry
                .initials
                .as_bytes()
                .try_into()
                .map_err(|_| format!("table {n}: bad initials {:?}", entry.initials))?;
            *slot = Entry {
                initials,
                score: entry.score,
            };
        }
        let table = ScoreTable::from_entries(parsed).ok_or_else(|| {
            format!(
                "table {n}: initials must be three letters A-Z or \"...\", scores 0 to 999999, \
                 highest first"
            )
        })?;
        scores.set_table(key, table);
    }
    Ok(scores)
}

/// The complete text of a score file holding `scores`.
pub(crate) fn render(scores: &Scores) -> String {
    let mut text = format!("{HEADER}\nformat = {FORMAT}\n");
    for (key, table) in scores.tables() {
        let levels = match &key.levels {
            LevelSet::Builtin => BUILTIN_LEVELS,
            LevelSet::Custom(name) => name,
        };
        let levels = toml::Value::String(levels.to_owned());
        let _ = write!(
            text,
            "\n[[table]]\nlevels = {levels}\nkind = \"{}\"\nentries = [\n",
            key.kind
        );
        for entry in table.entries() {
            let _ = writeln!(
                text,
                "  {{ initials = \"{}\", score = {} }},",
                entry.initials_text(),
                entry.score
            );
        }
        text.push_str("]\n");
    }
    text
}

/// A score file that could not be written.
#[derive(Debug)]
pub(crate) struct SaveError {
    /// The path that could not be written or renamed.
    pub(crate) path: PathBuf,
    /// What went wrong.
    pub(crate) source: io::Error,
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "high scores not saved: {}: {}; they are kept until the program ends",
            self.path.display(),
            self.source
        )
    }
}

impl std::error::Error for SaveError {}

/// The score tables together with the file they live in.
#[derive(Debug)]
pub(crate) struct ScoreStore {
    path: PathBuf,
    scores: Scores,
    /// The file on disk was not used and must be kept as
    /// `scores.toml.bad` before it is overwritten.
    keep_bad_file: bool,
}

impl ScoreStore {
    /// Loads the score file at `path` (see [`load`]).
    pub(crate) fn open(path: PathBuf) -> (Self, LoadStatus) {
        let (scores, status) = load(&path);
        let keep_bad_file = matches!(status, LoadStatus::Recovered { .. });
        (
            Self {
                path,
                scores,
                keep_bad_file,
            },
            status,
        )
    }

    /// Loads the score file at `path`, printing one notice naming the
    /// file if it could not be used.
    pub(crate) fn open_reporting(path: PathBuf) -> Self {
        let (store, status) = Self::open(path);
        if let LoadStatus::Recovered { reason } = &status {
            report::notice(format!(
                "{}: {reason}; using fresh high-score tables (the file is kept as {} at the next save)",
                store.path.display(),
                bad_path(&store.path).display()
            ));
        }
        store
    }

    /// The score file.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The tables.
    pub(crate) fn scores(&self) -> &Scores {
        &self.scores
    }

    /// Inserts `entry` into the table named by `key` and saves the file.
    /// The entry stays in memory whether or not the save succeeds.
    ///
    /// Returns the place taken (`None` when the score does not qualify,
    /// in which case nothing is written) and the outcome of the save.
    pub(crate) fn record(
        &mut self,
        key: &TableKey,
        entry: Entry,
    ) -> (Option<usize>, Result<(), SaveError>) {
        let place = self.scores.insert(key, entry);
        if place.is_none() {
            return (None, Ok(()));
        }
        (place, self.save())
    }

    /// [`ScoreStore::record`], printing a notice naming the file and the
    /// cause if the save fails.
    pub(crate) fn record_reporting(&mut self, key: &TableKey, entry: Entry) -> Option<usize> {
        let (place, saved) = self.record(key, entry);
        if let Err(error) = saved {
            report::notice(error);
        }
        place
    }

    /// Writes every table to the file: first, if the file on disk was not
    /// usable, it is renamed to `scores.toml.bad`; then the new text is
    /// written to `scores.toml.tmp` and renamed over `scores.toml`, so an
    /// interrupted write leaves the old or the new complete file. The
    /// directory is created if needed.
    ///
    /// # Errors
    ///
    /// [`SaveError`]; the caller reports it as a notice and carries on.
    pub(crate) fn save(&mut self) -> Result<(), SaveError> {
        let fail = |path: &Path| {
            let path = path.to_path_buf();
            move |source| SaveError { path, source }
        };
        if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
            fs::create_dir_all(dir).map_err(fail(dir))?;
        }
        if self.keep_bad_file {
            match fs::rename(&self.path, bad_path(&self.path)) {
                Ok(()) => {}
                // Gone since it was read: nothing left to keep.
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(SaveError {
                        path: bad_path(&self.path),
                        source,
                    });
                }
            }
            self.keep_bad_file = false;
        }
        let tmp = sibling(&self.path, ".tmp");
        fs::write(&tmp, render(&self.scores)).map_err(fail(&tmp))?;
        if let Err(source) = fs::rename(&tmp, &self.path) {
            let _ = fs::remove_file(&tmp);
            return Err(SaveError {
                path: self.path.clone(),
                source,
            });
        }
        Ok(())
    }
}

/// `scores.toml` → `scores.toml.bad`, in the same directory.
pub(crate) fn bad_path(path: &Path) -> PathBuf {
    sibling(path, ".bad")
}

/// `path` with `suffix` added to its file name.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map_or_else(
        || OsString::from(crate::config::paths::SCORES_FILE),
        OsString::from,
    );
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{LoadStatus, ScoreStore, bad_path, load, parse, render, sibling};
    use crate::config::GameMode;
    use crate::config::file::tests::TempDir;
    use crate::scores::{Entry, LevelSet, ScoreTable, Scores, TableKey, TableKind};

    fn key(mode: GameMode) -> TableKey {
        TableKey::from_mode(mode, LevelSet::Builtin)
    }

    fn scores_file(dir: &TempDir) -> PathBuf {
        dir.path().join("scores.toml")
    }

    fn sample() -> Scores {
        let mut scores = Scores::default();
        for (score, initials) in [(27700, *b"MSX"), (5000, *b"AAA"), (5000, *b"BBB")] {
            scores.insert(&key(GameMode::OnePlayer), Entry::new(initials, score));
        }
        scores.insert(
            &key(GameMode::TwoDiggerGauntlet),
            Entry::new(*b"ZZZ", 999_999),
        );
        scores.insert(
            &TableKey::from_mode(
                GameMode::Gauntlet,
                LevelSet::Custom("hard \"x\".dlf".into()),
            ),
            Entry::new(*b"HRD", 12),
        );
        scores
    }

    #[test]
    fn scores_round_trip() {
        let scores = sample();
        let text = render(&scores);
        assert!(text.contains("\nformat = 1\n"), "{text}");
        assert!(
            text.contains("{ initials = \"MSX\", score = 27700 },"),
            "{text}"
        );
        assert!(text.contains("kind = \"gauntlet-2\""), "{text}");
        let parsed = parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        assert_eq!(parsed, scores);
        assert_eq!(render(&parsed), text);
        // Ties keep their order across a save.
        let one = parsed.table(&key(GameMode::OnePlayer)).entries();
        assert_eq!(one[1], Entry::new(*b"BBB", 5000));
        assert_eq!(one[2], Entry::new(*b"AAA", 5000));
        // Tables absent from a valid file are fresh.
        assert_eq!(*parsed.table(&key(GameMode::Gauntlet)), ScoreTable::EMPTY);
        let empty = parse("format = 1\n").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(empty, Scores::default());
        // Through a file.
        let dir = TempDir::new("scores-round-trip");
        let (mut store, status) = ScoreStore::open(scores_file(&dir));
        assert_eq!(status, LoadStatus::Fresh);
        assert_eq!(
            store
                .record(&key(GameMode::OnePlayer), Entry::new(*b"MSX", 27700))
                .0,
            Some(0)
        );
        let (again, status) = load(&scores_file(&dir));
        assert_eq!(status, LoadStatus::Loaded);
        assert_eq!(again, *store.scores());
    }

    #[test]
    fn scores_custom_level_set_separate() {
        let dir = TempDir::new("scores-custom");
        let path = scores_file(&dir);
        let (mut store, _) = ScoreStore::open(path.clone());
        let hard = LevelSet::for_level_file(Some(Path::new("levels/hard.dlf")));
        let custom = TableKey::from_mode(GameMode::OnePlayer, hard.clone());
        let builtin = key(GameMode::OnePlayer);
        store
            .record(&builtin, Entry::new(*b"BLT", 100))
            .1
            .unwrap_or_else(|e| panic!("{e}"));
        store
            .record(&custom, Entry::new(*b"HRD", 900))
            .1
            .unwrap_or_else(|e| panic!("{e}"));
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(text.contains("levels = \"hard.dlf\""), "{text}");
        assert!(text.contains("levels = \"builtin\""), "{text}");
        let (scores, _) = load(&path);
        assert_eq!(
            scores.table(&builtin).entries()[0],
            Entry::new(*b"BLT", 100)
        );
        assert_eq!(scores.table(&builtin).entries()[1], Entry::EMPTY);
        assert_eq!(scores.table(&custom).entries()[0], Entry::new(*b"HRD", 900));
        assert_eq!(scores.table(&custom).entries()[1], Entry::EMPTY);
        let other = TableKey::from_mode(GameMode::OnePlayer, LevelSet::Custom("easy.dlf".into()));
        assert_eq!(*scores.table(&other), ScoreTable::EMPTY);
        assert_eq!(custom.kind, TableKind::Normal1);
    }

    #[test]
    fn scores_missing_is_fresh() {
        let dir = TempDir::new("scores-missing");
        let (scores, status) = load(&scores_file(&dir));
        assert_eq!(status, LoadStatus::Fresh);
        assert_eq!(scores, Scores::default());
        for mode in GameMode::ALL {
            assert_eq!(*scores.table(&key(mode)), ScoreTable::EMPTY);
        }
        // Opening does not create the file.
        let _ = ScoreStore::open_reporting(scores_file(&dir));
        assert!(!scores_file(&dir).exists());
    }

    fn assert_recovers(label: &str, content: &[u8]) -> String {
        let dir = TempDir::new(label);
        let path = scores_file(&dir);
        fs::write(&path, content).unwrap_or_else(|e| panic!("{e}"));
        let (scores, status) = load(&path);
        assert_eq!(scores, Scores::default(), "{label}");
        let LoadStatus::Recovered { reason } = status else {
            panic!("{label}: {status:?}");
        };
        // Loading leaves the file alone.
        assert_eq!(fs::read(&path).ok().as_deref(), Some(content));
        reason
    }

    #[test]
    fn scores_garbage_recovers() {
        let garbage: Vec<u8> = (0u32..512)
            .map(|i| u8::try_from(i.wrapping_mul(2_654_435_761) >> 24 & 0xff).unwrap_or(0))
            .collect();
        assert_recovers("garbage", &garbage);
        assert_recovers("not-toml", b"this is [not toml");
        assert_recovers("no-format", b"[[table]]\nlevels = \"builtin\"\n");
        let entries = |n: usize| {
            let mut t = String::from(
                "format = 1\n[[table]]\nlevels = \"builtin\"\nkind = \"normal-1\"\nentries = [\n",
            );
            for _ in 0..n {
                t.push_str("{ initials = \"...\", score = 0 },\n");
            }
            t.push_str("]\n");
            t
        };
        assert!(parse(&entries(10)).is_ok());
        assert!(assert_recovers("nine", entries(9).as_bytes()).contains("9 entries"));
        assert_recovers("eleven", entries(11).as_bytes());
        let ten = entries(10);
        assert_recovers("kind", ten.replace("normal-1", "normal-3").as_bytes());
        assert_recovers("initials", ten.replacen("\"...\"", "\"ab1\"", 1).as_bytes());
        assert_recovers(
            "short-initials",
            ten.replacen("\"...\"", "\"AB\"", 1).as_bytes(),
        );
        assert_recovers(
            "score-too-big",
            ten.replacen("score = 0", "score = 1000000", 1).as_bytes(),
        );
        assert_recovers(
            "negative",
            ten.replacen("score = 0", "score = -1", 1).as_bytes(),
        );
        assert_recovers(
            "unsorted",
            ten.replacen("score = 0", "score = 1", 2)
                .replacen("score = 1", "score = 0", 1)
                .as_bytes(),
        );
        assert_recovers(
            "unknown-field",
            ten.replace("format = 1\n", "format = 1\nextra = 2\n")
                .as_bytes(),
        );
        assert_recovers(
            "old-format",
            ten.replace("format = 1", "format = 0").as_bytes(),
        );
        let twice = format!("{ten}{}", ten.replace("format = 1\n", ""));
        assert!(assert_recovers("duplicate", twice.as_bytes()).contains("second"));
        // The notice path prints without panicking and keeps fresh tables.
        let dir = TempDir::new("garbage-notice");
        fs::write(scores_file(&dir), &garbage).unwrap_or_else(|e| panic!("{e}"));
        let store = ScoreStore::open_reporting(scores_file(&dir));
        assert_eq!(*store.scores(), Scores::default());
    }

    #[test]
    fn scores_newer_format_recovers() {
        let dir = TempDir::new("scores-newer");
        let path = scores_file(&dir);
        let newer = "format = 2\nsomething = \"new\"\n";
        fs::write(&path, newer).unwrap_or_else(|e| panic!("{e}"));
        let (mut store, status) = ScoreStore::open(path.clone());
        let LoadStatus::Recovered { reason } = status else {
            panic!("{status:?}");
        };
        assert!(reason.contains("format 2"), "{reason}");
        assert_eq!(*store.scores(), Scores::default());
        // Not overwritten without a new score.
        drop(ScoreStore::open(path.clone()));
        assert_eq!(fs::read_to_string(&path).ok().as_deref(), Some(newer));
        assert!(!bad_path(&path).exists());
        // A score that does not qualify writes nothing either.
        assert_eq!(
            store
                .record(&key(GameMode::OnePlayer), Entry::new(*b"ZER", 0))
                .0,
            None
        );
        assert_eq!(fs::read_to_string(&path).ok().as_deref(), Some(newer));
        // A new score renames it first.
        store
            .record(&key(GameMode::OnePlayer), Entry::new(*b"NEW", 10))
            .1
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            fs::read_to_string(bad_path(&path)).ok().as_deref(),
            Some(newer)
        );
        assert_eq!(load(&path).1, LoadStatus::Loaded);
    }

    #[test]
    fn scores_save_atomic() {
        let dir = TempDir::new("scores-atomic");
        let path = dir.path().join("nested").join("scores.toml");
        let tmp = sibling(&path, ".tmp");
        let (mut store, _) = ScoreStore::open(path.clone());
        store
            .record(&key(GameMode::OnePlayer), Entry::new(*b"ONE", 1))
            .1
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(path.exists(), "the directory is created");
        assert!(!tmp.exists(), "no temporary file is left behind");
        // A temporary file left by an interrupted write does not disturb
        // loading and is replaced by the next save.
        fs::write(&tmp, "format = 1\n[[table]]\nhalf a").unwrap_or_else(|e| panic!("{e}"));
        let (reloaded, status) = load(&path);
        assert_eq!(status, LoadStatus::Loaded);
        assert_eq!(reloaded, *store.scores());
        store
            .record(&key(GameMode::OnePlayer), Entry::new(*b"TWO", 2))
            .1
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(!tmp.exists());
        let (reloaded, _) = load(&path);
        assert_eq!(
            reloaded.table(&key(GameMode::OnePlayer)).entries()[0],
            Entry::new(*b"TWO", 2)
        );
        assert_eq!(
            reloaded.table(&key(GameMode::OnePlayer)).entries()[1],
            Entry::new(*b"ONE", 1)
        );
        // A write that cannot finish leaves the old file whole: here the
        // temporary file cannot be created because a directory has its
        // name.
        let before = fs::read(&path).unwrap_or_else(|e| panic!("{e}"));
        fs::create_dir_all(tmp.join("inside")).unwrap_or_else(|e| panic!("{e}"));
        let (place, saved) = store.record(&key(GameMode::OnePlayer), Entry::new(*b"BLK", 3));
        assert_eq!(place, Some(0));
        assert!(saved.is_err());
        assert_eq!(fs::read(&path).ok(), Some(before));
        assert_eq!(load(&path).1, LoadStatus::Loaded);
    }

    #[test]
    fn scores_bad_file_preserved() {
        let dir = TempDir::new("scores-bad");
        let path = scores_file(&dir);
        let garbage = b"\x00\xffnot a score file\x13";
        fs::write(&path, garbage).unwrap_or_else(|e| panic!("{e}"));
        let mut store = ScoreStore::open_reporting(path.clone());
        assert!(!bad_path(&path).exists(), "kept in place until a save");
        let place = store.record_reporting(&key(GameMode::OnePlayer), Entry::new(*b"NEW", 4200));
        assert_eq!(place, Some(0));
        assert_eq!(
            fs::read(bad_path(&path)).ok().as_deref(),
            Some(&garbage[..])
        );
        let (scores, status) = load(&path);
        assert_eq!(status, LoadStatus::Loaded);
        assert_eq!(
            scores.table(&key(GameMode::OnePlayer)).entries()[0],
            Entry::new(*b"NEW", 4200)
        );
        // Later saves leave the kept file alone.
        store
            .record(&key(GameMode::OnePlayer), Entry::new(*b"TWO", 5))
            .1
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            fs::read(bad_path(&path)).ok().as_deref(),
            Some(&garbage[..])
        );
    }

    #[test]
    fn scores_unwritable_keeps_memory() {
        let dir = TempDir::new("scores-unwritable");
        // The data directory is a regular file, so nothing can be written
        // in it (a permission bit would not stop a test run as root).
        let not_a_dir = dir.path().join("data");
        fs::write(&not_a_dir, b"").unwrap_or_else(|e| panic!("{e}"));
        let path = not_a_dir.join("scores.toml");
        let mut store = ScoreStore::open_reporting(path.clone());
        let entry = Entry::new(*b"MEM", 777);
        let (place, saved) = store.record(&key(GameMode::OnePlayer), entry);
        assert_eq!(place, Some(0));
        let error = saved
            .err()
            .unwrap_or_else(|| panic!("the save should fail"));
        let message = error.to_string();
        assert!(message.contains(&*not_a_dir.to_string_lossy()), "{message}");
        assert_eq!(
            store.scores().table(&key(GameMode::OnePlayer)).entries()[0],
            entry
        );
        // The reporting form carries on too.
        assert_eq!(
            store.record_reporting(&key(GameMode::OnePlayer), Entry::new(*b"TWO", 778)),
            Some(0)
        );
        assert_eq!(
            store.scores().table(&key(GameMode::OnePlayer)).entries()[1],
            entry
        );
        assert_eq!(store.path(), path);
    }
}
