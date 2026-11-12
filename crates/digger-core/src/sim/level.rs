//! Level layouts, the level-number-to-layout sequence, the difficulty index,
//! custom level data and the two-digger start-cell substitution (design D1).
//!
//! A [`LevelLayout`] is the 10×15 character map of one level; the layout
//! number, 1 to 8, is phase 2's [`LevelPlan`], which also selects the
//! background tile. A [`LevelSet`] holds the eight layouts of a game and its
//! bonus-life score step (research 01 §5.5, playfield spec).

use std::fmt;

use super::geom::{COLUMNS, CellPos, ROWS};
use super::rng::Modulus;
use crate::video::LevelPlan;

/// Number of layouts in a level set.
const BUILTIN_LAYOUTS: usize = 8;
/// Rows of a layout.
const LAYOUT_ROWS: usize = ROWS as usize;
/// Characters in a layout row.
const LAYOUT_COLUMNS: usize = COLUMNS as usize;
/// Bytes of one layout.
const LAYOUT_BYTES: usize = LAYOUT_ROWS * LAYOUT_COLUMNS;
/// Bytes of the eight layouts.
const LEVELS_BYTES: usize = BUILTIN_LAYOUTS * LAYOUT_BYTES;
/// Bytes of the bonus-life step that precedes the layouts in custom data.
const BONUS_STEP_BYTES: usize = 2;
/// Minimum length of custom level data: the bonus step and eight layouts.
pub const CUSTOM_LEVELS_LEN: usize = BONUS_STEP_BYTES + LEVELS_BYTES;

/// The bonus-life score step of the built-in levels (research 01 §5.2).
const BUILTIN_BONUS_STEP: u16 = 20_000;

/// The highest level number; completing it plays it again (research 01
/// §5.5, §10 item 4; compat-quirks Q08).
const MAX_LEVEL: u16 = 1000;
/// The level from which difficulty stops rising (research 01 §5.5; Q08).
const MAX_DIFFICULTY: u8 = 10;
/// Levels up to this number use the layout of the same number (Q08).
const DIRECT_LAYOUTS: u16 = 8;

/// The layouts whose start cells are opened for two diggers (Q07).
const Q07_LAYOUTS: [u8; 2] = [3, 4];
/// The row of the opened start cells (Q07).
const Q07_ROW: u8 = 9;
/// The columns of the opened start cells: where digger 2 and digger 1 of a
/// two-digger game start (Q07).
const Q07_COLUMNS: [u8; 2] = [6, 8];
/// The character the opened start cells read as: a horizontal tunnel (Q07).
const Q07_CHAR: u8 = b'H';

/// The eight built-in 10×15 layouts imported from upstream
/// (`assets/levels.bin`, see `docs/assets.md`): layout by layout, each row
/// by row, one character per cell from the alphabet ` SHVBC`. The array
/// type checks the length at compile time (research 01 §5.5).
static BUILTIN_LEVELS: &[u8; LEVELS_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/levels.bin"
));

/// The 10×15 character map of one level.
///
/// A space is solid earth, `S` a cell tunnelled both ways, `H` a horizontal
/// tunnel, `V` a vertical tunnel, `B` earth holding a gold bag and `C` earth
/// holding an emerald. Any other byte is kept as given and reads as solid
/// earth (playfield spec, "Level layout alphabet").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LevelLayout {
    rows: [[u8; LAYOUT_COLUMNS]; LAYOUT_ROWS],
}

impl LevelLayout {
    /// A layout of solid earth only.
    pub const EARTH: Self = Self {
        rows: [[b' '; LAYOUT_COLUMNS]; LAYOUT_ROWS],
    };

    /// The layout with these rows, top row first.
    pub const fn from_rows(rows: [[u8; LAYOUT_COLUMNS]; LAYOUT_ROWS]) -> Self {
        Self { rows }
    }

    /// The layout written as ten text rows of 15 bytes each, top row first.
    ///
    /// # Errors
    ///
    /// [`LevelError::RowLength`] for the first row that is not 15 bytes
    /// long.
    pub fn parse_rows(rows: [&str; LAYOUT_ROWS]) -> Result<Self, LevelError> {
        let mut layout = Self::EARTH;
        for (row, (target, text)) in layout.rows.iter_mut().zip(rows).enumerate() {
            *target = text
                .as_bytes()
                .try_into()
                .map_err(|_| LevelError::RowLength {
                    row,
                    length: text.len(),
                })?;
        }
        Ok(layout)
    }

    /// The rows, top row first.
    pub const fn rows(&self) -> &[[u8; LAYOUT_COLUMNS]; LAYOUT_ROWS] {
        &self.rows
    }

    /// The character written at `cell`.
    pub fn cell_char(&self, cell: CellPos) -> u8 {
        match self
            .rows
            .get(usize::from(cell.v()))
            .and_then(|row| row.get(usize::from(cell.h())))
        {
            Some(byte) => *byte,
            None => unreachable!("a cell position is inside the 15x10 grid"),
        }
    }

    /// Sets the character at `cell`.
    fn set_cell_char(&mut self, cell: CellPos, byte: u8) {
        if let Some(slot) = self
            .rows
            .get_mut(usize::from(cell.v()))
            .and_then(|row| row.get_mut(usize::from(cell.h())))
        {
            *slot = byte;
        }
    }

    /// The layout read from 150 bytes, row by row.
    fn from_bytes(bytes: &[u8; LAYOUT_BYTES]) -> Self {
        let mut layout = Self::EARTH;
        let (rows, _) = bytes.as_chunks::<LAYOUT_COLUMNS>();
        for (target, row) in layout.rows.iter_mut().zip(rows) {
            *target = *row;
        }
        layout
    }
}

impl Default for LevelLayout {
    fn default() -> Self {
        Self::EARTH
    }
}

/// A level number, 1 to 1000 (playfield spec, "Level number to layout
/// sequence").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LevelNumber(u16);

impl LevelNumber {
    /// Level 1.
    pub const FIRST: Self = Self(1);
    /// Level 1000, the highest.
    pub const MAX: Self = Self(MAX_LEVEL);

    /// Level `level`, or `None` outside 1..=1000.
    pub const fn new(level: u16) -> Option<Self> {
        if level >= 1 && level <= MAX_LEVEL {
            Some(Self(level))
        } else {
            None
        }
    }

    /// The level's number.
    pub const fn get(self) -> u16 {
        self.0
    }

    /// The level after this one; level 1000 is followed by level 1000
    /// again (compat-quirks Q08).
    #[must_use]
    pub const fn next(self) -> Self {
        if self.0 < MAX_LEVEL {
            Self(self.0 + 1)
        } else {
            Self::MAX
        }
    }

    /// The layout the level is played on: levels 1–8 use layouts 1–8,
    /// every later level layout (level mod 4) + 5, giving 1–8, 6–8, then
    /// 5–8 repeating (research 01 §5.5; compat-quirks Q08).
    pub const fn plan(self) -> LevelPlan {
        let plan = if self.0 <= DIRECT_LAYOUTS {
            self.0
        } else {
            self.0 % 4 + 5
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "`plan` is 1..=8 by the two branches above"
        )]
        let plan = plan as u8;
        match LevelPlan::new(plan) {
            Some(plan) => plan,
            None => unreachable!(),
        }
    }

    /// The difficulty index of the level, min(level, 10) (research 01
    /// §5.5, research 02 §7; compat-quirks Q08).
    pub const fn difficulty(self) -> Difficulty {
        if self.0 < MAX_DIFFICULTY as u16 {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "below 10 here, so the narrowing is exact"
            )]
            let difficulty = self.0 as u8;
            Difficulty(difficulty)
        } else {
            Difficulty(MAX_DIFFICULTY)
        }
    }
}

impl TryFrom<u16> for LevelNumber {
    type Error = LevelError;

    fn try_from(level: u16) -> Result<Self, LevelError> {
        Self::new(level).ok_or(LevelError::LevelOutOfRange { value: level })
    }
}

/// The difficulty index, 1 to 10, from which every level-dependent game
/// parameter is computed (playfield spec, "Difficulty index").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Difficulty(u8);

impl Difficulty {
    /// The index, 1..=10.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The modulus of a nobbin's double-move draw: 15 − difficulty, 5..=14
    /// (research 01 §7.1, research 02 §4.3).
    pub const fn double_move_modulus(self) -> Modulus {
        Modulus::from_difficulty(15 - self.0)
    }

    /// The modulus of the decision noise draw: difficulty + 5, 6..=15
    /// (research 01 §7.1, research 02 §4.4).
    pub const fn noise_modulus(self) -> Modulus {
        Modulus::from_difficulty(self.0 + 5)
    }
}

/// The eight layouts of a game, its bonus-life score step and whether the
/// layouts are used verbatim (design D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LevelSet {
    plans: [LevelLayout; BUILTIN_LAYOUTS],
    bonus_step: u16,
    verbatim: bool,
}

impl LevelSet {
    /// The game's own eight layouts with the bonus-life step of 20,000.
    pub fn builtin() -> Self {
        let mut plans = [LevelLayout::EARTH; BUILTIN_LAYOUTS];
        let (chunks, _) = BUILTIN_LEVELS.as_chunks::<LAYOUT_BYTES>();
        for (plan, chunk) in plans.iter_mut().zip(chunks) {
            *plan = LevelLayout::from_bytes(chunk);
        }
        Self {
            plans,
            bonus_step: BUILTIN_BONUS_STEP,
            verbatim: false,
        }
    }

    /// Custom level data: a 16-bit little-endian bonus-life step followed by
    /// eight layouts of 10 rows of 15 bytes, with no separators. Bytes after
    /// the first 1202 are ignored. Custom layouts are always used verbatim
    /// (playfield spec, "Custom level data"; compat-quirks U10).
    ///
    /// # Errors
    ///
    /// [`LevelError::TooShort`] when `data` holds fewer than 1202 bytes.
    pub fn from_custom_bytes(data: &[u8]) -> Result<Self, LevelError> {
        let too_short = LevelError::TooShort {
            expected: CUSTOM_LEVELS_LEN,
            actual: data.len(),
        };
        let (step, rest) = data
            .split_first_chunk::<BONUS_STEP_BYTES>()
            .ok_or(too_short)?;
        let levels = rest.first_chunk::<LEVELS_BYTES>().ok_or(too_short)?;
        let mut plans = [LevelLayout::EARTH; BUILTIN_LAYOUTS];
        let (chunks, _) = levels.as_chunks::<LAYOUT_BYTES>();
        for (plan, chunk) in plans.iter_mut().zip(chunks) {
            *plan = LevelLayout::from_bytes(chunk);
        }
        Ok(Self {
            plans,
            bonus_step: u16::from_le_bytes(*step),
            verbatim: true,
        })
    }

    /// A set from layouts given directly, as a recording header holds them.
    /// `verbatim` disables the two-digger start-cell substitution (Q07).
    pub const fn from_plans(
        plans: [LevelLayout; BUILTIN_LAYOUTS],
        bonus_step: u16,
        verbatim: bool,
    ) -> Self {
        Self {
            plans,
            bonus_step,
            verbatim,
        }
    }

    /// The layout numbered `plan`, as stored.
    pub fn layout(&self, plan: LevelPlan) -> &LevelLayout {
        match self.plans.get(usize::from(plan.number()) - 1) {
            Some(layout) => layout,
            None => unreachable!("a level plan is 1..=8"),
        }
    }

    /// All eight layouts, layout 1 first.
    pub const fn layouts(&self) -> &[LevelLayout; BUILTIN_LAYOUTS] {
        &self.plans
    }

    /// The bonus-life score step.
    pub const fn bonus_step(&self) -> u16 {
        self.bonus_step
    }

    /// Whether the layouts are used exactly as stored.
    pub const fn is_verbatim(&self) -> bool {
        self.verbatim
    }

    /// The character of `cell` in layout `plan` as the game reads it.
    ///
    /// **Quirk Q07:** with two diggers on a non-verbatim set, layouts 3 and
    /// 4 read as `H` at cells (6,9) and (8,9), the start cells of the two
    /// diggers, so both start in a tunnel (research 01 §5.5, §10 item 7).
    pub fn cell_char(&self, plan: LevelPlan, cell: CellPos, two_diggers: bool) -> u8 {
        if two_diggers && !self.verbatim && is_q07_cell(plan, cell) {
            Q07_CHAR
        } else {
            self.layout(plan).cell_char(cell)
        }
    }

    /// Layout `plan` with every cell read through [`Self::cell_char`]: the
    /// map a level is set up from.
    pub fn layout_as_played(&self, plan: LevelPlan, two_diggers: bool) -> LevelLayout {
        let mut layout = *self.layout(plan);
        for cell in CellPos::all_column_major() {
            layout.set_cell_char(cell, self.cell_char(plan, cell, two_diggers));
        }
        layout
    }
}

impl Default for LevelSet {
    fn default() -> Self {
        Self::builtin()
    }
}

/// Whether `cell` of layout `plan` is one of the start cells opened for two
/// diggers (Q07).
fn is_q07_cell(plan: LevelPlan, cell: CellPos) -> bool {
    Q07_LAYOUTS.contains(&plan.number()) && cell.v() == Q07_ROW && Q07_COLUMNS.contains(&cell.h())
}

/// Level data that cannot be used (design D10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LevelError {
    /// Custom level data is shorter than the bonus step and eight layouts.
    TooShort {
        /// The minimum length, 1202 bytes.
        expected: usize,
        /// The length supplied.
        actual: usize,
    },
    /// A layout row is not 15 bytes long.
    RowLength {
        /// The row, 0 for the top.
        row: usize,
        /// Its length in bytes.
        length: usize,
    },
    /// A level number outside 1..=1000.
    LevelOutOfRange {
        /// The rejected number.
        value: u16,
    },
}

impl fmt::Display for LevelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { expected, actual } => write!(
                f,
                "custom level data is too short: expected at least {expected} bytes, got {actual}"
            ),
            Self::RowLength { row, length } => write!(
                f,
                "layout row {row} is {length} bytes long instead of {LAYOUT_COLUMNS}"
            ),
            Self::LevelOutOfRange { value } => {
                write!(f, "level {value} is outside the valid range 1..1000")
            }
        }
    }
}

impl std::error::Error for LevelError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(n: u8) -> LevelPlan {
        LevelPlan::new(n).unwrap_or_else(|| unreachable!("plan {n}"))
    }

    fn level(n: u16) -> LevelNumber {
        LevelNumber::new(n).unwrap_or_else(|| unreachable!("level {n}"))
    }

    fn cell(h: u8, v: u8) -> CellPos {
        CellPos::new(h, v).unwrap_or_else(|| unreachable!("cell ({h},{v})"))
    }

    fn row_text(layout: &LevelLayout, row: usize) -> String {
        layout
            .rows()
            .get(row)
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .unwrap_or_default()
    }

    /// The set read from custom data that is known to be long enough.
    fn custom(data: &[u8]) -> LevelSet {
        LevelSet::from_custom_bytes(data).unwrap_or_else(|error| unreachable!("{error}"))
    }

    /// The layout from rows that are known to be 15 bytes long.
    fn parsed(rows: [&str; 10]) -> LevelLayout {
        LevelLayout::parse_rows(rows).unwrap_or_else(|error| unreachable!("{error}"))
    }

    /// Custom data whose layout `p` is filled with the digit `p` and whose
    /// row `r` starts with the letter `a + r`, so every byte's origin is
    /// visible.
    fn marked_custom_data() -> Vec<u8> {
        let mut data = vec![0x10, 0x27];
        for p in 0..8u8 {
            for r in 0..10u8 {
                for c in 0..15u8 {
                    data.push(if c == 0 { b'a' + r } else { b'0' + p });
                }
            }
        }
        data
    }

    #[test]
    fn embedded_levels_hold_eight_layouts_over_the_alphabet() {
        assert_eq!(BUILTIN_LEVELS.len(), 1200);
        assert!(BUILTIN_LEVELS.iter().all(|byte| b" SHVBC".contains(byte)));
        let set = LevelSet::builtin();
        assert_eq!(set.bonus_step(), 20_000);
        assert!(!set.is_verbatim());
        for (p, layout) in set.layouts().iter().enumerate() {
            let flat: Vec<u8> = layout.rows().iter().flatten().copied().collect();
            assert_eq!(
                Some(flat.as_slice()),
                BUILTIN_LEVELS.get(p * 150..(p + 1) * 150)
            );
        }
    }

    #[test]
    fn builtin_layout_1_rows_match_spec() {
        let set = LevelSet::builtin();
        let layout = set.layout(plan(1));
        let expected = [
            "S   B     HHHHS",
            "V  CC  C  V B  ",
            "VB CC  C  V    ",
            "V  CCB CB V CCC",
            "V  CC  C  V CCC",
            "HH CC  C  V CCC",
            " V    B B V    ",
            " HHHH     V    ",
            "C   V     V   C",
            "CC  HHHHHHH  CC",
        ];
        for (row, text) in expected.iter().enumerate() {
            assert_eq!(row_text(layout, row), *text, "row {row}");
        }
        assert_eq!(*layout, parsed(expected));
        assert_eq!(layout.cell_char(cell(4, 0)), b'B');
        assert_eq!(layout.cell_char(cell(14, 9)), b'C');
    }

    #[test]
    fn layout_sequence_1_to_13() {
        let plans: Vec<u8> = (1..=13).map(|n| level(n).plan().number()).collect();
        assert_eq!(plans, [1, 2, 3, 4, 5, 6, 7, 8, 6, 7, 8, 5, 6]);
        // The 5–8 cycle continues.
        let later: Vec<u8> = (14..=21).map(|n| level(n).plan().number()).collect();
        assert_eq!(later, [7, 8, 5, 6, 7, 8, 5, 6]);
    }

    #[test]
    fn level_1000_stays_and_uses_layout_5() {
        assert_eq!(LevelNumber::MAX.get(), 1000);
        assert_eq!(LevelNumber::MAX.next(), LevelNumber::MAX);
        assert_eq!(LevelNumber::MAX.plan().number(), 5);
        assert_eq!(level(999).next(), LevelNumber::MAX);
        assert_eq!(level(1).next(), level(2));
        assert_eq!(LevelNumber::new(0), None);
        assert_eq!(LevelNumber::new(1001), None);
        assert_eq!(
            LevelNumber::try_from(1001),
            Err(LevelError::LevelOutOfRange { value: 1001 })
        );
    }

    #[test]
    fn difficulty_is_min_level_10() {
        for (n, expected) in [
            (1, 1),
            (3, 3),
            (9, 9),
            (10, 10),
            (11, 10),
            (250, 10),
            (1000, 10),
        ] {
            assert_eq!(level(n).difficulty().get(), expected, "level {n}");
        }
        let easiest = level(1).difficulty();
        let hardest = level(10).difficulty();
        assert_eq!(easiest.double_move_modulus().get(), 14);
        assert_eq!(hardest.double_move_modulus().get(), 5);
        assert_eq!(easiest.noise_modulus().get(), 6);
        assert_eq!(hardest.noise_modulus().get(), 15);
    }

    #[test]
    fn quirk_q07_two_digger_start_cells_open_on_layouts_3_and_4() {
        let set = LevelSet::builtin();
        for p in [3, 4] {
            let stored = set.layout(plan(p));
            // As stored, neither start cell is a tunnel.
            assert_ne!(stored.cell_char(cell(6, 9)), b'H', "layout {p}");
            assert_ne!(stored.cell_char(cell(8, 9)), b'H', "layout {p}");
            for h in [6, 8] {
                assert_eq!(set.cell_char(plan(p), cell(h, 9), true), b'H');
            }
            // Every other cell reads as stored.
            let played = set.layout_as_played(plan(p), true);
            for c in CellPos::all_column_major() {
                let expected = if c.v() == 9 && (c.h() == 6 || c.h() == 8) {
                    b'H'
                } else {
                    stored.cell_char(c)
                };
                assert_eq!(played.cell_char(c), expected, "layout {p} cell {c:?}");
            }
        }
        // Layout 4 holds an emerald at (6,9) and (8,9): it is gone.
        assert_eq!(set.layout(plan(4)).cell_char(cell(6, 9)), b'C');
        // Other layouts are never changed, and one digger sees the stored map.
        for p in 1..=8 {
            let stored = *set.layout(plan(p));
            assert_eq!(set.layout_as_played(plan(p), false), stored, "layout {p}");
            if p != 3 && p != 4 {
                assert_eq!(set.layout_as_played(plan(p), true), stored, "layout {p}");
            }
        }
    }

    #[test]
    fn q07_not_applied_to_custom_or_verbatim_levels() {
        let builtin = LevelSet::builtin();
        let verbatim = LevelSet::from_plans(*builtin.layouts(), 20_000, true);
        let mut data = vec![0x20, 0x4E];
        for layout in builtin.layouts() {
            data.extend(layout.rows().iter().flatten());
        }
        let custom = custom(&data);
        assert!(custom.is_verbatim());
        for set in [verbatim, custom] {
            for p in [3, 4] {
                let stored = *builtin.layout(plan(p));
                assert_eq!(set.layout_as_played(plan(p), true), stored);
                for h in [6, 8] {
                    assert_eq!(
                        set.cell_char(plan(p), cell(h, 9), true),
                        stored.cell_char(cell(h, 9))
                    );
                }
            }
        }
        // A non-verbatim set from plans gets the substitution.
        let patched = LevelSet::from_plans(*builtin.layouts(), 20_000, false);
        assert_eq!(patched.cell_char(plan(3), cell(6, 9), true), b'H');
    }

    #[test]
    fn custom_levels_parse_bonus_step_and_rows() {
        let data = marked_custom_data();
        assert_eq!(data.len(), CUSTOM_LEVELS_LEN);
        let set = custom(&data);
        assert_eq!(set.bonus_step(), 10_000);
        assert!(set.is_verbatim());
        // Byte 2 + p×150 + r×15 + c is column c of row r of layout p+1.
        for p in 0..8u8 {
            let layout = set.layout(plan(p + 1));
            for r in 0..10u8 {
                for c in 0..15u8 {
                    let index = 2 + usize::from(p) * 150 + usize::from(r) * 15 + usize::from(c);
                    assert_eq!(Some(&layout.cell_char(cell(c, r))), data.get(index));
                }
            }
        }
        // Layout 1 differs from the built-in one and is used for level 1.
        assert_ne!(
            set.layout(level(1).plan()),
            LevelSet::builtin().layout(plan(1))
        );
        assert_eq!(row_text(set.layout(level(1).plan()), 3), "d00000000000000");
    }

    #[test]
    fn custom_levels_reject_short_data() {
        for len in [0, 1, 2, 900, 1201] {
            let data = vec![b' '; len];
            assert_eq!(
                LevelSet::from_custom_bytes(&data),
                Err(LevelError::TooShort {
                    expected: 1202,
                    actual: len
                })
            );
        }
        let message = LevelError::TooShort {
            expected: 1202,
            actual: 900,
        }
        .to_string();
        assert!(
            message.contains("1202") && message.contains("900"),
            "{message}"
        );
    }

    #[test]
    fn custom_levels_ignore_trailing_bytes() {
        let data = marked_custom_data();
        let mut longer = data.clone();
        // A level file stores high scores after the layouts.
        longer.extend([0xFF; 512]);
        assert_eq!(
            LevelSet::from_custom_bytes(&longer),
            LevelSet::from_custom_bytes(&data)
        );
    }

    #[test]
    fn parse_rows_rejects_wrong_row_length() {
        let mut rows = ["               "; 10];
        rows[4] = "too short";
        assert_eq!(
            LevelLayout::parse_rows(rows),
            Err(LevelError::RowLength { row: 4, length: 9 })
        );
        // Characters outside the alphabet are kept as given.
        rows[4] = "x              ";
        let layout = parsed(rows);
        assert_eq!(layout.cell_char(cell(0, 4)), b'x');
    }

    #[test]
    fn from_plans_keeps_layouts_and_step() {
        let mut plans = [LevelLayout::EARTH; 8];
        plans[7] = *LevelSet::builtin().layout(plan(1));
        let set = LevelSet::from_plans(plans, 1234, false);
        assert_eq!(set.bonus_step(), 1234);
        assert!(!set.is_verbatim());
        assert_eq!(set.layout(plan(8)), LevelSet::builtin().layout(plan(1)));
        assert_eq!(*set.layout(plan(1)), LevelLayout::EARTH);
    }
}
