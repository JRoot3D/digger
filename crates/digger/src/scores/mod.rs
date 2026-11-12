//! The high-score tables: `ScoreTable`, `TableKey`, qualification and
//! insertion (design D10, `high-scores` spec).
//!
//! There are four tables per level set (normal and gauntlet, one and two
//! Diggers); the built-in levels and every custom level file have a set of
//! their own. A table holds exactly ten entries, highest first. A table
//! that was never written is ten `...` entries with score 0.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the tables are used by the title, initials and game-over scenes of later task groups"
    )
)]

pub(crate) mod store;

use std::collections::BTreeMap;
use std::fmt;

use crate::config::GameMode;

/// Entries in every table.
pub(crate) const TABLE_LEN: usize = 10;

/// The highest score a table can hold: the six-digit score shown on
/// screen.
pub(crate) const MAX_SCORE: u32 = 999_999;

/// The initials of an entry that was never filled.
pub(crate) const EMPTY_INITIALS: [u8; 3] = *b"...";

/// One line of a table: three initials and a score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Entry {
    /// Three letters `A`–`Z`, or `...` for an entry never filled.
    pub(crate) initials: [u8; 3],
    /// The six-digit score, 0 to 999999.
    pub(crate) score: u32,
}

impl Entry {
    /// The entry of a table that was never written: `...` with 0.
    pub(crate) const EMPTY: Self = Self {
        initials: EMPTY_INITIALS,
        score: 0,
    };

    /// An entry with `initials` and `score`; a score above 999999 is held
    /// as 999999.
    pub(crate) fn new(initials: [u8; 3], score: u32) -> Self {
        Self {
            initials,
            score: score.min(MAX_SCORE),
        }
    }

    /// The initials as text.
    pub(crate) fn initials_text(self) -> String {
        self.initials.iter().copied().map(char::from).collect()
    }

    /// The row shown on the title screen: the initials, two spaces and
    /// the score right-aligned in six characters padded with spaces
    /// (`ABC   12345`, `...       0`; research 01 §6.3).
    pub(crate) fn row_text(self) -> String {
        format!("{}  {:>6}", self.initials_text(), self.score)
    }
}

/// Whether `initials` are valid in a table: three letters `A`–`Z`, or
/// `...`.
pub(crate) fn valid_initials(initials: [u8; 3]) -> bool {
    initials == EMPTY_INITIALS || initials.iter().all(u8::is_ascii_uppercase)
}

/// Ten entries ordered from highest to lowest score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ScoreTable {
    entries: [Entry; TABLE_LEN],
}

impl Default for ScoreTable {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl ScoreTable {
    /// A table that was never written: ten `...` entries with 0.
    pub(crate) const EMPTY: Self = Self {
        entries: [Entry::EMPTY; TABLE_LEN],
    };

    /// A table holding `entries`, or `None` unless they are ten valid
    /// entries ordered from highest to lowest (equal scores allowed).
    pub(crate) fn from_entries(entries: [Entry; TABLE_LEN]) -> Option<Self> {
        let valid = entries
            .iter()
            .all(|e| e.score <= MAX_SCORE && valid_initials(e.initials));
        let sorted = entries.windows(2).all(|w| match w {
            [a, b] => a.score >= b.score,
            _ => true,
        });
        (valid && sorted).then_some(Self { entries })
    }

    /// The entries, first place first.
    pub(crate) fn entries(&self) -> &[Entry; TABLE_LEN] {
        &self.entries
    }

    /// The score of tenth place.
    pub(crate) fn lowest(&self) -> u32 {
        self.entries.last().map_or(0, |e| e.score)
    }

    /// Whether `score` earns a place: only a score strictly greater than
    /// tenth place does, so 0 never qualifies against a fresh table
    /// (research 01 §6.3). `score` is the six-digit score shown on
    /// screen.
    pub(crate) fn qualifies(&self, score: u32) -> bool {
        score > self.lowest()
    }

    /// Puts `entry` above every entry with a lower or equal score and
    /// below every higher one; the entries below move down and the former
    /// tenth entry is dropped (research 01 §6.3: ties go above). Returns
    /// the place taken, 0 for first; `None`, leaving the table unchanged,
    /// when the score does not qualify or the initials are not three
    /// letters `A`–`Z` (a table only ever holds entries it can store).
    pub(crate) fn insert(&mut self, entry: Entry) -> Option<usize> {
        if !self.qualifies(entry.score) || !valid_initials(entry.initials) {
            return None;
        }
        let place = self.entries.iter().position(|e| e.score <= entry.score)?;
        let tail = self.entries.get_mut(place..)?;
        tail.rotate_right(1);
        *tail.first_mut()? = entry;
        Some(place)
    }
}

/// Which levels a table belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum LevelSet {
    /// The eight built-in layouts.
    Builtin,
    /// A level file, by its name without directory (`hard.dlf`).
    Custom(String),
}

impl LevelSet {
    /// The level set of a game played with the level file at `path`
    /// (`None`: the built-in levels).
    pub(crate) fn for_level_file(path: Option<&std::path::Path>) -> Self {
        match path.and_then(std::path::Path::file_name) {
            Some(name) => Self::Custom(name.to_string_lossy().into_owned()),
            None => Self::Builtin,
        }
    }
}

/// Which of the four tables of a level set: the game's kind and its
/// number of Diggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum TableKind {
    /// Normal game, one Digger (one player and two players alternating).
    Normal1,
    /// Gauntlet, one Digger.
    Gauntlet1,
    /// Normal game, two Diggers.
    Normal2,
    /// Gauntlet, two Diggers.
    Gauntlet2,
}

impl TableKind {
    /// The four kinds, in the order they are written to the file.
    pub(crate) const ALL: [Self; 4] = [
        Self::Normal1,
        Self::Gauntlet1,
        Self::Normal2,
        Self::Gauntlet2,
    ];

    /// The kind's name in the score file.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Normal1 => "normal-1",
            Self::Gauntlet1 => "gauntlet-1",
            Self::Normal2 => "normal-2",
            Self::Gauntlet2 => "gauntlet-2",
        }
    }

    /// The kind named `name` in the score file.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// The table a game in `mode` uses: alternating two-player games
    /// share the one-Digger normal table (research 01 §6.4).
    pub(crate) const fn from_mode(mode: GameMode) -> Self {
        match mode {
            GameMode::OnePlayer | GameMode::TwoPlayers => Self::Normal1,
            GameMode::Gauntlet => Self::Gauntlet1,
            GameMode::TwoDiggers => Self::Normal2,
            GameMode::TwoDiggerGauntlet => Self::Gauntlet2,
        }
    }

    /// Whether the table is for gauntlet games.
    pub(crate) const fn gauntlet(self) -> bool {
        matches!(self, Self::Gauntlet1 | Self::Gauntlet2)
    }

    /// Whether the table is for games with two Diggers.
    pub(crate) const fn two_diggers(self) -> bool {
        matches!(self, Self::Normal2 | Self::Gauntlet2)
    }
}

impl fmt::Display for TableKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Names one table: its level set and its kind.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct TableKey {
    /// The levels the games were played on.
    pub(crate) levels: LevelSet,
    /// Normal or gauntlet, one or two Diggers.
    pub(crate) kind: TableKind,
}

impl TableKey {
    /// The table for a game in `mode` on `levels`.
    pub(crate) fn from_mode(mode: GameMode, levels: LevelSet) -> Self {
        Self {
            levels,
            kind: TableKind::from_mode(mode),
        }
    }
}

/// Every table the program knows: those read from the score file and
/// those written since. A table not held is a fresh one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Scores {
    tables: BTreeMap<TableKey, ScoreTable>,
}

impl Scores {
    /// The table named by `key`; a fresh table if it was never written.
    pub(crate) fn table(&self, key: &TableKey) -> &ScoreTable {
        self.tables.get(key).unwrap_or(&ScoreTable::EMPTY)
    }

    /// Whether `score` earns a place in the table named by `key`.
    pub(crate) fn qualifies(&self, key: &TableKey, score: u32) -> bool {
        self.table(key).qualifies(score)
    }

    /// Inserts `entry` into the table named by `key` (see
    /// [`ScoreTable::insert`]) and returns the place taken.
    pub(crate) fn insert(&mut self, key: &TableKey, entry: Entry) -> Option<usize> {
        if !self.qualifies(key, entry.score) {
            return None;
        }
        self.tables.entry(key.clone()).or_default().insert(entry)
    }

    /// Replaces the table named by `key`.
    pub(crate) fn set_table(&mut self, key: TableKey, table: ScoreTable) {
        self.tables.insert(key, table);
    }

    /// The tables held, built-in levels first, then each level file by
    /// name, each set in [`TableKind::ALL`] order.
    pub(crate) fn tables(&self) -> impl Iterator<Item = (&TableKey, &ScoreTable)> {
        self.tables.iter()
    }
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::{Entry, LevelSet, MAX_SCORE, ScoreTable, Scores, TABLE_LEN, TableKey, TableKind};
    use crate::config::GameMode;

    /// A table with the given scores (highest first) and initials `AAA`,
    /// `BBB`, … in order.
    fn table(scores: [u32; TABLE_LEN]) -> ScoreTable {
        let mut entries = [Entry::EMPTY; TABLE_LEN];
        for (i, (entry, score)) in entries.iter_mut().zip(scores).enumerate() {
            let letter = b'A' + u8::try_from(i).unwrap_or(0);
            *entry = Entry::new([letter; 3], score);
        }
        ScoreTable::from_entries(entries).unwrap_or_else(|| panic!("bad table {scores:?}"))
    }

    #[test]
    fn qualify_strict() {
        let t = table([9000, 8000, 7000, 6000, 5000, 4000, 3000, 2000, 1500, 1000]);
        assert!(!t.qualifies(1000), "equal to tenth place does not qualify");
        assert!(!t.qualifies(999));
        assert!(t.qualifies(1001));
    }

    #[test]
    fn qualify_zero_against_empty() {
        let t = ScoreTable::EMPTY;
        assert!(t.entries().iter().all(|e| *e == Entry::EMPTY));
        assert!(!t.qualifies(0));
        assert!(t.qualifies(1));
        let mut t = t;
        assert_eq!(t.insert(Entry::new(*b"ABC", 0)), None);
        assert_eq!(t, ScoreTable::EMPTY);
    }

    #[test]
    fn insert_tie_above() {
        let mut t = table([9000, 5000, 4000, 3000, 2000, 1000, 900, 800, 700, 600]);
        assert_eq!(t.entries()[1], Entry::new(*b"BBB", 5000));
        assert_eq!(t.insert(Entry::new(*b"XYZ", 5000)), Some(1));
        assert_eq!(t.entries()[0], Entry::new(*b"AAA", 9000));
        assert_eq!(t.entries()[1], Entry::new(*b"XYZ", 5000));
        assert_eq!(t.entries()[2], Entry::new(*b"BBB", 5000));
        assert_eq!(t.entries()[3], Entry::new(*b"CCC", 4000));
        // A tie with tenth place but above it via an equal run: the new
        // entry goes above all the equal scores.
        let mut t = table([10, 10, 10, 10, 10, 10, 10, 10, 10, 0]);
        assert_eq!(t.insert(Entry::new(*b"NEW", 10)), Some(0));
        assert_eq!(t.entries()[0].initials, *b"NEW");
        assert_eq!(t.entries()[9], Entry::new(*b"III", 10));
    }

    #[test]
    fn insert_drops_tenth() {
        let mut t = table([9000, 8000, 7000, 6000, 5000, 4000, 3000, 2000, 1500, 1000]);
        assert_eq!(t.insert(Entry::new(*b"TOP", 20000)), Some(0));
        assert_eq!(t.entries()[0], Entry::new(*b"TOP", 20000));
        assert_eq!(t.entries()[1], Entry::new(*b"AAA", 9000));
        assert_eq!(t.entries()[9], Entry::new(*b"III", 1500));
        assert!(t.entries().iter().all(|e| e.initials != *b"JJJ"));
        // Into last place.
        assert_eq!(t.insert(Entry::new(*b"LOW", 1600)), Some(9));
        assert_eq!(t.entries()[8], Entry::new(*b"HHH", 2000));
        assert_eq!(t.entries()[9], Entry::new(*b"LOW", 1600));
        // A fresh table: the first entry takes first place.
        let mut t = ScoreTable::EMPTY;
        assert_eq!(t.insert(Entry::new(*b"MSX", 27700)), Some(0));
        assert_eq!(t.entries()[1], Entry::EMPTY);
        // The score is held to six digits.
        assert_eq!(Entry::new(*b"BIG", 5_000_000).score, MAX_SCORE);
    }

    #[test]
    fn table_key_mapping() {
        let key = |mode| TableKey::from_mode(mode, LevelSet::Builtin).kind;
        assert_eq!(key(GameMode::OnePlayer), TableKind::Normal1);
        assert_eq!(key(GameMode::TwoPlayers), TableKind::Normal1);
        assert_eq!(key(GameMode::Gauntlet), TableKind::Gauntlet1);
        assert_eq!(key(GameMode::TwoDiggers), TableKind::Normal2);
        assert_eq!(key(GameMode::TwoDiggerGauntlet), TableKind::Gauntlet2);
        for kind in TableKind::ALL {
            assert_eq!(TableKind::from_name(kind.name()), Some(kind));
        }
        assert!(TableKind::Gauntlet2.gauntlet() && TableKind::Gauntlet2.two_diggers());
        assert!(!TableKind::Normal1.gauntlet() && !TableKind::Normal1.two_diggers());
        assert_eq!(
            LevelSet::for_level_file(Some(std::path::Path::new("/x/y/hard.dlf"))),
            LevelSet::Custom("hard.dlf".to_owned())
        );
        assert_eq!(LevelSet::for_level_file(None), LevelSet::Builtin);
        // Gauntlet scores stay apart; alternating players share the
        // one-player table.
        let mut scores = Scores::default();
        let gauntlet = TableKey::from_mode(GameMode::Gauntlet, LevelSet::Builtin);
        let one = TableKey::from_mode(GameMode::OnePlayer, LevelSet::Builtin);
        let two = TableKey::from_mode(GameMode::TwoPlayers, LevelSet::Builtin);
        assert_eq!(
            scores.insert(&gauntlet, Entry::new(*b"GGG", 50000)),
            Some(0)
        );
        assert_eq!(*scores.table(&one), ScoreTable::EMPTY);
        assert_eq!(scores.insert(&two, Entry::new(*b"PTW", 700)), Some(0));
        assert_eq!(scores.table(&one).entries()[0], Entry::new(*b"PTW", 700));
        assert_eq!(scores.table(&gauntlet).entries()[0].score, 50000);
    }

    #[test]
    fn score_row_text() {
        assert_eq!(Entry::new(*b"ABC", 12345).row_text(), "ABC   12345");
        assert_eq!(Entry::EMPTY.row_text(), "...       0");
        assert_eq!(Entry::new(*b"MAX", 999_999).row_text(), "MAX  999999");
        assert_eq!(Entry::new(*b"ABC", 12345).row_text().len(), 11);
    }

    #[test]
    fn from_entries_rejects_bad_tables() {
        let mut entries = [Entry::EMPTY; TABLE_LEN];
        entries[3].score = 5;
        assert_eq!(ScoreTable::from_entries(entries), None, "unsorted");
        let mut entries = [Entry::EMPTY; TABLE_LEN];
        entries[0].initials = *b"ab1";
        assert_eq!(ScoreTable::from_entries(entries), None, "not letters");
        let mut entries = [Entry::EMPTY; TABLE_LEN];
        entries[0].score = MAX_SCORE + 1;
        assert_eq!(ScoreTable::from_entries(entries), None, "too large");
        // Insertion keeps the same rules: initials that could not be
        // stored are refused and the table is unchanged.
        let mut t = ScoreTable::EMPTY;
        assert_eq!(t.insert(Entry::new(*b"a\"b", 100)), None);
        assert_eq!(t, ScoreTable::EMPTY);
    }
}
