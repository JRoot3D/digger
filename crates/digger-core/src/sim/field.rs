//! The earth of the 15x10 cell grid: slices, the dig step, redraw
//! normalisation, emeralds and the per-player saves (design D1, D5).
//!
//! Each cell's earth is five horizontal slices 4 pixels wide and six
//! vertical slices 3 pixels high, plus an "untouched" mark. A [`Cell`] keeps
//! the original's 16-bit layout because the upstream state hash covers it
//! (design D5): horizontal slices in bits 0–4, vertical slices in bits 6–11,
//! the untouched mark in bit 13 and every other bit always set. No mask
//! literal appears outside this module (research 01 §5.3, research 02 §0).
//!
//! [`EmeraldMap`] holds one bit per cell and player, so each player of an
//! alternating game keeps their own emeralds (research 01 §5.4).

use super::geom::{CellPos, Direction, locate};
use super::ids::PlayerId;
use super::level::{LevelLayout, LevelNumber, LevelSet};
use super::penalty::{Penalty, PenaltySite};
use crate::geom::Px;

/// Number of horizontal slices of a cell: five, 4 pixels wide each.
pub(crate) const H_SLICES: u8 = 5;
/// Number of vertical slices of a cell: six, 3 pixels high each.
pub(crate) const V_SLICES: u8 = 6;
/// Width of a horizontal slice in pixels.
const H_SLICE_WIDTH: u8 = 4;
/// Height of a vertical slice in pixels.
const V_SLICE_HEIGHT: u8 = 3;

/// Bits of the five horizontal slices.
const H_MASK: u16 = 0x001f;
/// Bit of the first vertical slice; slice `i` is bit `V_SHIFT + i`.
const V_SHIFT: u8 = 6;
/// Bits of the six vertical slices.
const V_MASK: u16 = 0x0fc0;
/// The untouched mark.
const UNTOUCHED: u16 = 0x2000;
/// Every slice: what a cell without any tunnel holds.
const ALL_SLICES: u16 = H_MASK | V_MASK;

/// Number of cells in the grid.
const CELLS: usize = 150;
/// Number of grid columns, as an index stride.
const STRIDE: usize = 15;

/// The earth of one cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cell(u16);

impl Cell {
    /// Solid untouched earth: every slice present.
    pub const EARTH: Self = Self(u16::MAX);

    /// The cell a level starts with for the layout character `c`: `S` has
    /// no slices, `H` no horizontal slices, `V` no vertical slices, and all
    /// three are not untouched; any other character is solid untouched
    /// earth (playfield spec, "Initial earth and tunnel state").
    pub const fn from_layout_char(c: u8) -> Self {
        match c {
            b'S' => Self(Self::EARTH.0 & !(H_MASK | V_MASK | UNTOUCHED)),
            b'H' => Self(Self::EARTH.0 & !(H_MASK | UNTOUCHED)),
            b'V' => Self(Self::EARTH.0 & !(V_MASK | UNTOUCHED)),
            _ => Self::EARTH,
        }
    }

    /// The cell's 16-bit value in the original's layout, as the upstream
    /// state hash reads it.
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Whether horizontal slice `i` (0..=4, left to right) is still earth.
    /// A slice number out of range reads as earth.
    pub const fn has_h_slice(self, i: u8) -> bool {
        i >= H_SLICES || self.0 & (1 << i) != 0
    }

    /// Whether vertical slice `i` (0..=5, top to bottom) is still earth.
    /// A slice number out of range reads as earth.
    pub const fn has_v_slice(self, i: u8) -> bool {
        i >= V_SLICES || self.0 & (1 << (V_SHIFT + i)) != 0
    }

    /// Whether all five horizontal slices are still earth.
    pub const fn has_all_h_slices(self) -> bool {
        self.0 & H_MASK == H_MASK
    }

    /// Whether all six vertical slices are still earth.
    pub const fn has_all_v_slices(self) -> bool {
        self.0 & V_MASK == V_MASK
    }

    /// Whether every horizontal slice is gone.
    pub const fn has_no_h_slices(self) -> bool {
        self.0 & H_MASK == 0
    }

    /// Whether every vertical slice is gone.
    pub const fn has_no_v_slices(self) -> bool {
        self.0 & V_MASK == 0
    }

    /// Whether the cell is untouched: never fully crossed by a digging
    /// move and not opened by its layout character.
    pub const fn is_untouched(self) -> bool {
        self.0 & UNTOUCHED != 0
    }

    /// Whether the cell has a tunnel in it: at least one of its eleven
    /// slices is gone (research 02 §0).
    pub const fn has_tunnel(self) -> bool {
        self.0 & ALL_SLICES != ALL_SLICES
    }

    /// Removes horizontal slice `i`; out-of-range numbers change nothing.
    pub(crate) const fn clear_h_slice(&mut self, i: u8) {
        if i < H_SLICES {
            self.0 &= !(1 << i);
        }
    }

    /// Removes vertical slice `i`; out-of-range numbers change nothing.
    pub(crate) const fn clear_v_slice(&mut self, i: u8) {
        if i < V_SLICES {
            self.0 &= !(1 << (V_SHIFT + i));
        }
    }

    /// Removes the untouched mark.
    const fn clear_untouched(&mut self) {
        self.0 &= !UNTOUCHED;
    }

    /// Removes every horizontal slice and the untouched mark.
    const fn open_horizontal(&mut self) {
        self.0 &= !(H_MASK | UNTOUCHED);
    }

    /// Removes every vertical slice and the untouched mark.
    const fn open_vertical(&mut self) {
        self.0 &= !(V_MASK | UNTOUCHED);
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::EARTH
    }
}

/// The earth of the whole 15×10 grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Field {
    /// Cells row by row: cell (h, v) at index v×15 + h, the original's
    /// order, which the state hash follows.
    cells: [Cell; CELLS],
}

impl Field {
    /// A field of solid untouched earth, as a level number below 1 gives
    /// (playfield spec, "Initial earth and tunnel state").
    pub const EARTH: Self = Self {
        cells: [Cell::EARTH; CELLS],
    };

    /// The field a level starts with for `layout`, read cell by cell
    /// through [`Cell::from_layout_char`].
    pub fn from_layout(layout: &LevelLayout) -> Self {
        let mut field = Self::EARTH;
        for cell in CellPos::all_column_major() {
            *field.cell_mut(cell) = Cell::from_layout_char(layout.cell_char(cell));
        }
        field
    }

    /// The field `level` of `set` starts with, as played with one or two
    /// diggers (the Q07 substitution applies through
    /// [`LevelSet::layout_as_played`]). `None` stands for a level number
    /// below 1 and gives solid earth.
    pub fn for_level(set: &LevelSet, level: Option<LevelNumber>, two_diggers: bool) -> Self {
        match level {
            Some(level) => Self::from_layout(&set.layout_as_played(level.plan(), two_diggers)),
            None => Self::EARTH,
        }
    }

    /// The earth of `cell`.
    pub fn cell(&self, cell: CellPos) -> Cell {
        match self.cells.get(index(cell)) {
            Some(cell) => *cell,
            None => unreachable!("a cell position is inside the 15x10 grid"),
        }
    }

    /// Every cell row by row, cell (h, v) at index v×15 + h.
    pub const fn cells(&self) -> &[Cell; CELLS] {
        &self.cells
    }

    /// The earth of `cell`, mutably.
    fn cell_mut(&mut self, cell: CellPos) -> &mut Cell {
        match self.cells.get_mut(index(cell)) {
            Some(cell) => cell,
            None => unreachable!("a cell position is inside the 15x10 grid"),
        }
    }

    /// One dig step of a mover at `at` heading `dir`: removes exactly one
    /// slice of the cell being entered and charges [`PenaltySite::DigStep`].
    ///
    /// With h, v the mover's cell, xs = ((x−12) mod 20) div 4 and
    /// ys = ((y−18) mod 18) div 3: right removes horizontal slice xs of
    /// (h+1, v); down vertical slice ys of (h, v+1); left horizontal slice
    /// xs−1 of (h, v), or slice 4 of (h−1, v) when xs is 0; up vertical
    /// slice ys−1 of (h, v), or slice 5 of (h, v−1) when ys is 0. A
    /// horizontal step that leaves the cell with no horizontal slices, or a
    /// vertical one that leaves it with no vertical slices, clears its
    /// untouched mark (playfield spec, "Digging removes one slice per
    /// step"; research 01 §5.3, research 02 §0).
    ///
    /// The penalty is charged even with no direction (compat-quirks Q38).
    /// A step that would address a cell outside the grid changes nothing
    /// (compat-quirks U05).
    #[allow(dead_code, reason = "called by the movers of later task groups")]
    pub(crate) fn dig(&mut self, at: Px, dir: Option<Direction>, penalty: &mut Penalty) {
        penalty.charge(PenaltySite::DigStep);
        let (Some(dir), Some(located)) = (dir, locate(at)) else {
            return;
        };
        let xs = located.rem_x / H_SLICE_WIDTH;
        let ys = located.rem_y / V_SLICE_HEIGHT;
        let own = located.cell;
        let (target, slice) = match dir {
            Direction::Right => (own.neighbour(Direction::Right), xs),
            Direction::Down => (own.neighbour(Direction::Down), ys),
            Direction::Left => match xs.checked_sub(1) {
                Some(slice) => (Some(own), slice),
                None => (own.neighbour(Direction::Left), H_SLICES - 1),
            },
            Direction::Up => match ys.checked_sub(1) {
                Some(slice) => (Some(own), slice),
                None => (own.neighbour(Direction::Up), V_SLICES - 1),
            },
        };
        let Some(target) = target else {
            return;
        };
        let cell = self.cell_mut(target);
        if dir.is_horizontal() {
            cell.clear_h_slice(slice);
            if cell.has_no_h_slices() {
                cell.clear_untouched();
            }
        } else {
            cell.clear_v_slice(slice);
            if cell.has_no_v_slices() {
                cell.clear_untouched();
            }
        }
    }
}

impl Field {
    /// Applies the full-redraw normalisation to every cell: a cell that is
    /// not untouched loses all six vertical slices when any of them is
    /// gone, and all five horizontal slices when any of them is gone.
    /// Untouched cells are left as they are.
    ///
    /// **Quirk Q05:** a full draw (a new level, or the turn passing to the
    /// other player) changes the game state this way, not only the picture,
    /// so it runs whether or not anything is displayed (research 01 §5.3,
    /// §10 item 8, research 03 §8.2).
    #[allow(dead_code, reason = "called by the full level draw (task group 4)")]
    pub(crate) fn normalise_for_redraw(&mut self) {
        for cell in &mut self.cells {
            if cell.is_untouched() {
                continue;
            }
            if !cell.has_all_v_slices() {
                cell.open_vertical();
            }
            if !cell.has_all_h_slices() {
                cell.open_horizontal();
            }
        }
    }

    /// Whether a nobbin in cell `from` can move one cell in `dir`: the
    /// neighbouring cell exists, is not untouched, and is connected to
    /// `from`, meaning the neighbour's slice nearest `from` is gone or
    /// `from`'s slice nearest the neighbour is gone (monsters spec,
    /// "Nobbin passability and choice"; research 02 §0, §4.4).
    pub fn is_passable(&self, from: CellPos, dir: Direction) -> bool {
        let Some(to) = from.neighbour(dir) else {
            return false;
        };
        let (own, target) = (self.cell(from), self.cell(to));
        if target.is_untouched() {
            return false;
        }
        let (last_h, last_v) = (H_SLICES - 1, V_SLICES - 1);
        match dir {
            Direction::Right => !target.has_h_slice(0) || !own.has_h_slice(last_h),
            Direction::Left => !target.has_h_slice(last_h) || !own.has_h_slice(0),
            Direction::Up => !target.has_v_slice(last_v) || !own.has_v_slice(0),
            Direction::Down => !target.has_v_slice(0) || !own.has_v_slice(last_v),
        }
    }
}

impl Default for Field {
    fn default() -> Self {
        Self::EARTH
    }
}

/// The field of each player of an alternating two-player game, saved at the
/// end of every round and restored when that player's field is next drawn
/// in full. Two simultaneous diggers share one field and use the first
/// player's save only (playfield spec, "Per-player field memory"; research
/// 01 §2.1, §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct PlayerFields {
    saved: [Field; PlayerId::COUNT],
}

#[allow(dead_code, reason = "used by the round flow (task groups 7 and 14)")]
impl PlayerFields {
    /// Keeps `field` as `player`'s field: at level start with the level's
    /// initial field, and at the end of every round with the field in play.
    pub(crate) fn save(&mut self, player: PlayerId, field: &Field) {
        *player.of_mut(&mut self.saved) = *field;
    }

    /// `player`'s saved field, to be put in play before a full draw.
    pub(crate) fn restore(&self, player: PlayerId) -> Field {
        *player.of(&self.saved)
    }
}

/// The emeralds of the grid: one bit per cell for each player of an
/// alternating game, so each player keeps their own emeralds (research 01
/// §5.4; playfield spec, "Emerald placement and counting").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmeraldMap {
    /// Per cell, row by row like [`Field`], bit `p` for player `p`.
    cells: [u8; CELLS],
}

impl EmeraldMap {
    /// A map without any emerald.
    pub const EMPTY: Self = Self { cells: [0; CELLS] };

    /// Sets `player`'s emeralds to the `C` cells of `layout` and clears
    /// them everywhere else; the other player's emeralds are kept.
    pub fn place(&mut self, player: PlayerId, layout: &LevelLayout) {
        let mask = player_mask(player);
        for cell in CellPos::all_column_major() {
            let bits = self.bits_mut(cell);
            if layout.cell_char(cell) == EMERALD_CHAR {
                *bits |= mask;
            } else {
                *bits &= !mask;
            }
        }
    }

    /// Places `player`'s emeralds for `level` of `set` as played with one or
    /// two diggers; `None`, a level number below 1, places none.
    pub fn place_for_level(
        &mut self,
        player: PlayerId,
        set: &LevelSet,
        level: Option<LevelNumber>,
        two_diggers: bool,
    ) {
        let layout = match level {
            Some(level) => set.layout_as_played(level.plan(), two_diggers),
            None => LevelLayout::EARTH,
        };
        self.place(player, &layout);
    }

    /// Whether `cell` holds one of `player`'s emeralds.
    pub fn has(&self, player: PlayerId, cell: CellPos) -> bool {
        self.bits(cell) & player_mask(player) != 0
    }

    /// Removes `player`'s emerald from `cell`; returns whether there was
    /// one. An emerald is removed once only.
    #[allow(
        dead_code,
        reason = "called by diggers, hobbins and bags (later task groups)"
    )]
    pub(crate) fn remove(&mut self, player: PlayerId, cell: CellPos) -> bool {
        let mask = player_mask(player);
        let bits = self.bits_mut(cell);
        let had = *bits & mask != 0;
        *bits &= !mask;
        had
    }

    /// The number of cells still holding one of `player`'s emeralds.
    pub fn count(&self, player: PlayerId) -> u8 {
        let mask = player_mask(player);
        let count = self.cells.iter().filter(|&&bits| bits & mask != 0).count();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "at most 150 cells, so the count fits"
        )]
        let count = count as u8;
        count
    }

    /// The bits of `cell`: bit `p` set when player `p`'s emerald is there.
    /// This is the original's per-cell value, as the state hash reads it.
    pub fn bits(&self, cell: CellPos) -> u8 {
        match self.cells.get(index(cell)) {
            Some(bits) => *bits,
            None => unreachable!("a cell position is inside the 15x10 grid"),
        }
    }

    /// Every cell's bits, row by row, cell (h, v) at index v×15 + h.
    pub const fn cells(&self) -> &[u8; CELLS] {
        &self.cells
    }

    /// The bits of `cell`, mutably.
    fn bits_mut(&mut self, cell: CellPos) -> &mut u8 {
        match self.cells.get_mut(index(cell)) {
            Some(bits) => bits,
            None => unreachable!("a cell position is inside the 15x10 grid"),
        }
    }
}

impl Default for EmeraldMap {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// The layout character of a cell holding an emerald.
const EMERALD_CHAR: u8 = b'C';

/// The emerald bit of `player`: 1 for the first, 2 for the second.
const fn player_mask(player: PlayerId) -> u8 {
    1 << player.number()
}

/// The row-major index of `cell`: v×15 + h.
fn index(cell: CellPos) -> usize {
    usize::from(cell.v()) * STRIDE + usize::from(cell.h())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(h: u8, v: u8) -> CellPos {
        CellPos::new(h, v).unwrap_or_else(|| unreachable!("test cell inside the grid"))
    }

    fn level(n: u16) -> Option<LevelNumber> {
        LevelNumber::new(n)
    }

    /// A layout of solid earth with the given characters at the given cells.
    fn layout_with(marks: &[(u8, u8, u8)]) -> LevelLayout {
        let mut rows = [[b' '; 15]; 10];
        for &(h, v, c) in marks {
            if let Some(slot) = rows
                .get_mut(usize::from(v))
                .and_then(|row| row.get_mut(usize::from(h)))
            {
                *slot = c;
            }
        }
        LevelLayout::from_rows(rows)
    }

    fn h_slices(c: Cell) -> [bool; 5] {
        [0, 1, 2, 3, 4].map(|i| c.has_h_slice(i))
    }

    fn v_slices(c: Cell) -> [bool; 6] {
        [0, 1, 2, 3, 4, 5].map(|i| c.has_v_slice(i))
    }

    /// The cells of `after` that differ from `before`.
    fn changed(before: &Field, after: &Field) -> Vec<CellPos> {
        CellPos::all_column_major()
            .filter(|&c| before.cell(c) != after.cell(c))
            .collect()
    }

    #[test]
    fn initial_cells_from_layout_chars() {
        let field = Field::from_layout(&layout_with(&[
            (0, 0, b'S'),
            (1, 0, b'H'),
            (2, 0, b'V'),
            (3, 0, b' '),
            (4, 0, b'B'),
            (5, 0, b'C'),
        ]));
        let junction = field.cell(cell(0, 0));
        assert!(junction.has_no_h_slices() && junction.has_no_v_slices());
        assert!(!junction.is_untouched() && junction.has_tunnel());

        let horizontal = field.cell(cell(1, 0));
        assert_eq!(h_slices(horizontal), [false; 5]);
        assert_eq!(v_slices(horizontal), [true; 6]);
        assert!(!horizontal.is_untouched() && horizontal.has_tunnel());

        let vertical = field.cell(cell(2, 0));
        assert_eq!(h_slices(vertical), [true; 5]);
        assert_eq!(v_slices(vertical), [false; 6]);
        assert!(!vertical.is_untouched() && vertical.has_tunnel());

        for h in 3..=5 {
            let earth = field.cell(cell(h, 0));
            assert_eq!(earth, Cell::EARTH);
            assert!(earth.has_all_h_slices() && earth.has_all_v_slices());
            assert!(earth.is_untouched() && !earth.has_tunnel());
        }
        // The original's bit values (research 01 §5.3), which the state
        // hash will read.
        assert_eq!(junction.bits(), 0xd020);
        assert_eq!(horizontal.bits(), 0xdfe0);
        assert_eq!(vertical.bits(), 0xd03f);
        assert_eq!(Cell::EARTH.bits(), 0xffff);
    }

    #[test]
    fn unknown_char_is_solid_earth() {
        for c in [b'x', b'h', b's', b'0', b'.', 0, 0xff] {
            assert_eq!(Cell::from_layout_char(c), Cell::EARTH, "char {c:#04x}");
        }
        let field = Field::from_layout(&layout_with(&[(7, 3, b'x')]));
        assert_eq!(field, Field::EARTH);
    }

    #[test]
    fn level_below_1_is_all_earth() {
        let set = LevelSet::builtin();
        assert_eq!(level(0), None);
        assert_eq!(Field::for_level(&set, None, false), Field::EARTH);
        assert_eq!(Field::for_level(&set, None, true), Field::EARTH);
        assert!(Field::EARTH.cells().iter().all(|c| c.is_untouched()));

        // Level 1 is not: its first cell is an `S`.
        let level_1 = Field::for_level(&set, level(1), false);
        assert!(level_1.cell(cell(0, 0)).has_no_h_slices());
        assert_eq!(
            level_1,
            Field::from_layout(set.layout(LevelNumber::FIRST.plan()))
        );
    }

    #[test]
    fn dig_right_from_corner_removes_slice_0_of_next_cell() {
        let mut field = Field::EARTH;
        let mut penalty = Penalty::default();
        field.dig(Px::new(12, 18), Some(Direction::Right), &mut penalty);
        assert_eq!(changed(&Field::EARTH, &field), [cell(1, 0)]);
        let entered = field.cell(cell(1, 0));
        assert_eq!(h_slices(entered), [false, true, true, true, true]);
        assert_eq!(v_slices(entered), [true; 6]);
        assert!(entered.is_untouched());
        assert_eq!(penalty.value(), 1);
        assert_eq!(penalty.count_of(PenaltySite::DigStep), 1);
    }

    #[test]
    fn dig_left_from_corner_removes_slice_4_of_previous_cell() {
        let mut field = Field::EARTH;
        let mut penalty = Penalty::default();
        field.dig(Px::new(52, 18), Some(Direction::Left), &mut penalty);
        assert_eq!(changed(&Field::EARTH, &field), [cell(1, 0)]);
        assert_eq!(
            h_slices(field.cell(cell(1, 0))),
            [true, true, true, true, false]
        );

        // Inside a cell the step removes slice xs − 1 of the own cell.
        let mut field = Field::EARTH;
        field.dig(Px::new(60, 18), Some(Direction::Left), &mut penalty);
        assert_eq!(changed(&Field::EARTH, &field), [cell(2, 0)]);
        assert_eq!(
            h_slices(field.cell(cell(2, 0))),
            [true, false, true, true, true]
        );
        assert_eq!(penalty.count_of(PenaltySite::DigStep), 2);
    }

    #[test]
    fn dig_up_inside_cell() {
        let mut field = Field::EARTH;
        let mut penalty = Penalty::default();
        field.dig(Px::new(12, 42), Some(Direction::Up), &mut penalty);
        assert_eq!(changed(&Field::EARTH, &field), [cell(0, 1)]);
        assert_eq!(
            v_slices(field.cell(cell(0, 1))),
            [true, false, true, true, true, true]
        );

        // From a cell corner the step removes slice 5 of the cell above.
        let mut field = Field::EARTH;
        field.dig(Px::new(12, 36), Some(Direction::Up), &mut penalty);
        assert_eq!(changed(&Field::EARTH, &field), [cell(0, 0)]);
        assert_eq!(
            v_slices(field.cell(cell(0, 0))),
            [true, true, true, true, true, false]
        );

        // Down removes slice ys of the cell below.
        let mut field = Field::EARTH;
        field.dig(Px::new(12, 24), Some(Direction::Down), &mut penalty);
        assert_eq!(changed(&Field::EARTH, &field), [cell(0, 1)]);
        assert_eq!(
            v_slices(field.cell(cell(0, 1))),
            [true, true, false, true, true, true]
        );
        assert_eq!(penalty.count_of(PenaltySite::DigStep), 3);
    }

    #[test]
    fn five_steps_clear_untouched_mark() {
        let mut field = Field::EARTH;
        let mut penalty = Penalty::default();
        for step in 0..5 {
            assert!(field.cell(cell(1, 0)).is_untouched(), "before step {step}");
            field.dig(
                Px::new(12 + 4 * step, 18),
                Some(Direction::Right),
                &mut penalty,
            );
        }
        let crossed = field.cell(cell(1, 0));
        assert!(crossed.has_no_h_slices());
        assert!(crossed.has_all_v_slices());
        assert!(!crossed.is_untouched());
        assert_eq!(changed(&Field::EARTH, &field), [cell(1, 0)]);

        // Six steps down clear the mark of a vertically crossed cell.
        let mut field = Field::EARTH;
        for step in 0..6 {
            field.dig(
                Px::new(12, 18 + 3 * step),
                Some(Direction::Down),
                &mut penalty,
            );
        }
        let crossed = field.cell(cell(0, 1));
        assert!(crossed.has_no_v_slices() && crossed.has_all_h_slices());
        assert!(!crossed.is_untouched());
        assert_eq!(penalty.count_of(PenaltySite::DigStep), 11);
    }

    #[test]
    fn partly_dug_cell_stays_untouched() {
        let mut field = Field::EARTH;
        let mut penalty = Penalty::default();
        field.dig(Px::new(12, 18), Some(Direction::Right), &mut penalty);
        field.dig(Px::new(16, 18), Some(Direction::Right), &mut penalty);
        let dug = field.cell(cell(1, 0));
        assert_eq!(h_slices(dug), [false, false, true, true, true]);
        assert!(dug.has_tunnel());
        assert!(dug.is_untouched());

        // A vertical step into the same cell leaves it untouched as well: the
        // mark goes only when one kind of slice is gone completely.
        field.dig(Px::new(32, 36), Some(Direction::Up), &mut penalty);
        let dug = field.cell(cell(1, 0));
        assert!(!dug.has_v_slice(5) && dug.is_untouched());
    }

    #[test]
    fn dig_beyond_grid_changes_nothing() {
        let mut penalty = Penalty::default();
        let cases = [
            (Px::new(292, 18), Direction::Right),
            (Px::new(292, 180), Direction::Right),
            (Px::new(12, 180), Direction::Down),
            (Px::new(12, 18), Direction::Left),
            (Px::new(12, 18), Direction::Up),
            (Px::new(8, 18), Direction::Right),
            (Px::new(12, 10), Direction::Down),
            (Px::new(400, 300), Direction::Left),
        ];
        for (at, dir) in cases {
            for start in [
                Field::EARTH,
                Field::for_level(&LevelSet::builtin(), level(1), false),
            ] {
                let mut field = start;
                field.dig(at, Some(dir), &mut penalty);
                assert_eq!(field, start, "dig {dir:?} from {at:?}");
            }
        }
        // The penalty is charged for every request all the same.
        assert_eq!(penalty.count_of(PenaltySite::DigStep), 16);
    }

    #[test]
    fn dig_without_direction_charges_but_changes_nothing() {
        let mut field = Field::EARTH;
        let mut penalty = Penalty::default();
        field.dig(Px::new(100, 100), None, &mut penalty);
        assert_eq!(field, Field::EARTH);
        assert_eq!(penalty.count_of(PenaltySite::DigStep), 1);
    }

    #[test]
    fn quirk_q05_redraw_opens_partly_dug_cells() {
        // (1,0) is `H`, (2,0) is `V`: neither is untouched.
        let start = Field::from_layout(&layout_with(&[(1, 0, b'H'), (2, 0, b'V')]));
        let mut field = start;
        let mut penalty = Penalty::default();
        // One vertical slice of the `H` cell: a step down from (32,18) into
        // (1,1) would leave (1,0) alone, so dig up from the cell below.
        field.dig(Px::new(32, 36), Some(Direction::Up), &mut penalty);
        // One horizontal slice of the `V` cell: a step right from (32,18).
        field.dig(Px::new(32, 18), Some(Direction::Right), &mut penalty);
        let h_cell = field.cell(cell(1, 0));
        let v_cell = field.cell(cell(2, 0));
        assert_eq!(v_slices(h_cell), [true, true, true, true, true, false]);
        assert_eq!(h_slices(v_cell), [false, true, true, true, true]);

        field.normalise_for_redraw();
        let h_cell = field.cell(cell(1, 0));
        let v_cell = field.cell(cell(2, 0));
        assert!(h_cell.has_no_v_slices() && h_cell.has_no_h_slices());
        assert!(v_cell.has_no_h_slices() && v_cell.has_no_v_slices());
        assert_eq!(changed(&start, &field), [cell(1, 0), cell(2, 0)]);

        // A cell opened by its layout character alone is already normal.
        let mut level_1 = Field::for_level(&LevelSet::builtin(), level(1), false);
        let before = level_1;
        level_1.normalise_for_redraw();
        assert_eq!(level_1, before);
    }

    #[test]
    fn untouched_cells_survive_redraw() {
        let mut field = Field::EARTH;
        let mut penalty = Penalty::default();
        // Two horizontal and one vertical slice of the untouched (1,0).
        field.dig(Px::new(12, 18), Some(Direction::Right), &mut penalty);
        field.dig(Px::new(16, 18), Some(Direction::Right), &mut penalty);
        field.dig(Px::new(32, 36), Some(Direction::Up), &mut penalty);
        assert!(field.cell(cell(1, 0)).is_untouched());
        let before = field;
        field.normalise_for_redraw();
        assert_eq!(field, before);

        let mut earth = Field::EARTH;
        earth.normalise_for_redraw();
        assert_eq!(earth, Field::EARTH);
    }

    #[test]
    fn redraw_normalisation_is_idempotent() {
        let mut field = Field::for_level(&LevelSet::builtin(), level(2), false);
        let mut penalty = Penalty::default();
        for step in 0..3 {
            field.dig(
                Px::new(12, 18 + 3 * step),
                Some(Direction::Down),
                &mut penalty,
            );
        }
        field.normalise_for_redraw();
        let once = field;
        field.normalise_for_redraw();
        assert_eq!(field, once);
    }

    #[test]
    fn players_have_separate_fields() {
        let set = LevelSet::builtin();
        let initial = Field::for_level(&set, level(1), false);
        let mut saves = PlayerFields::default();
        let [p1, p2] = PlayerId::ALL;
        saves.save(p1, &initial);
        saves.save(p2, &initial);

        // Player 1 digs a tunnel and the round ends.
        let mut in_play = saves.restore(p1);
        let mut penalty = Penalty::default();
        for step in 0..5 {
            in_play.dig(
                Px::new(112 + 4 * step, 90),
                Some(Direction::Right),
                &mut penalty,
            );
        }
        assert_ne!(in_play, initial);
        saves.save(p1, &in_play);

        // Player 2's turn starts from the level's initial layout.
        assert_eq!(saves.restore(p2), initial);
        // The turn returns to player 1 with the saved tunnels.
        assert_eq!(saves.restore(p1), in_play);
        assert!(!saves.restore(p1).cell(cell(6, 4)).is_untouched());
        assert!(initial.cell(cell(6, 4)).is_untouched());
    }

    #[test]
    fn level_1_has_30_emeralds() {
        let set = LevelSet::builtin();
        let [p1, p2] = PlayerId::ALL;
        let mut emeralds = EmeraldMap::default();
        emeralds.place_for_level(p1, &set, level(1), false);
        assert_eq!(emeralds.count(p1), 30);
        assert_eq!(emeralds.count(p2), 0);
        // Exactly the `C` cells hold one.
        let layout = set.layout(LevelNumber::FIRST.plan());
        for c in CellPos::all_column_major() {
            assert_eq!(emeralds.has(p1, c), layout.cell_char(c) == b'C', "{c:?}");
        }
        // The first emerald of row 1 is at (3,1).
        assert!(emeralds.has(p1, cell(3, 1)) && !emeralds.has(p1, cell(2, 1)));

        let mut none = EmeraldMap::default();
        none.place_for_level(p1, &set, None, false);
        assert_eq!(none, EmeraldMap::EMPTY);
    }

    #[test]
    fn emerald_removed_once() {
        let set = LevelSet::builtin();
        let [p1, _] = PlayerId::ALL;
        let mut emeralds = EmeraldMap::default();
        emeralds.place_for_level(p1, &set, level(1), false);
        assert!(emeralds.remove(p1, cell(3, 1)));
        assert_eq!(emeralds.count(p1), 29);
        assert!(!emeralds.has(p1, cell(3, 1)));
        assert!(!emeralds.remove(p1, cell(3, 1)));
        assert_eq!(emeralds.count(p1), 29);
        // A cell without an emerald has nothing to remove.
        assert!(!emeralds.remove(p1, cell(0, 0)));
        assert_eq!(emeralds.count(p1), 29);
    }

    #[test]
    fn emeralds_are_per_player() {
        let set = LevelSet::builtin();
        let [p1, p2] = PlayerId::ALL;
        let mut emeralds = EmeraldMap::default();
        emeralds.place_for_level(p1, &set, level(1), false);
        emeralds.place_for_level(p2, &set, level(2), false);
        let level_2_count = emeralds.count(p2);
        assert_eq!(emeralds.count(p1), 30);
        assert!(level_2_count > 0);

        // Player 1 eats an emerald that both players have.
        let shared = CellPos::all_column_major()
            .find(|&c| emeralds.has(p1, c) && emeralds.has(p2, c))
            .unwrap_or_else(|| unreachable!("levels 1 and 2 share an emerald cell"));
        assert_eq!(emeralds.bits(shared), 0b11);
        assert!(emeralds.remove(p1, shared));
        assert_eq!(emeralds.bits(shared), 0b10);
        assert!(emeralds.has(p2, shared));
        assert_eq!(emeralds.count(p1), 29);
        assert_eq!(emeralds.count(p2), level_2_count);

        // Placing player 1's next level leaves player 2's emeralds alone.
        let before_p2: Vec<bool> = CellPos::all_column_major()
            .map(|c| emeralds.has(p2, c))
            .collect();
        emeralds.place_for_level(p1, &set, level(3), false);
        let after_p2: Vec<bool> = CellPos::all_column_major()
            .map(|c| emeralds.has(p2, c))
            .collect();
        assert_eq!(before_p2, after_p2);
    }

    #[test]
    fn q07_start_cells_hold_no_emerald() {
        let set = LevelSet::builtin();
        let [p1, _] = PlayerId::ALL;
        for n in [3, 4] {
            let mut emeralds = EmeraldMap::default();
            emeralds.place_for_level(p1, &set, level(n), true);
            assert!(!emeralds.has(p1, cell(6, 9)) && !emeralds.has(p1, cell(8, 9)));
            let field = Field::for_level(&set, level(n), true);
            for h in [6, 8] {
                let c = field.cell(cell(h, 9));
                assert!(c.has_no_h_slices() && c.has_all_v_slices() && !c.is_untouched());
            }
        }
    }

    /// A field with every cell `S`.
    fn all_open() -> Field {
        Field::from_layout(&LevelLayout::from_rows([[b'S'; 15]; 10]))
    }

    #[test]
    fn passable_through_joined_cells() {
        let field = Field::for_level(&LevelSet::builtin(), level(1), false);
        // Row 9 of layout 1 holds `H` from column 4 to 10.
        assert!(field.is_passable(cell(5, 9), Direction::Right));
        assert!(field.is_passable(cell(6, 9), Direction::Left));
        assert!(field.is_passable(cell(4, 9), Direction::Right));
        // The `S` at (0,0) joins the `V` below it.
        assert!(field.is_passable(cell(0, 0), Direction::Down));
        assert!(field.is_passable(cell(0, 1), Direction::Up));

        // A digger's tunnel joins two cells once the slice ahead is gone.
        let mut field = Field::EARTH;
        let mut penalty = Penalty::default();
        for step in 0..5 {
            field.dig(
                Px::new(12 + 4 * step, 18),
                Some(Direction::Right),
                &mut penalty,
            );
        }
        assert!(!field.cell(cell(1, 0)).is_untouched());
        assert!(field.is_passable(cell(0, 0), Direction::Right));
        assert!(!field.is_passable(cell(1, 0), Direction::Left));
    }

    #[test]
    fn not_passable_into_untouched_cell() {
        let field = Field::for_level(&LevelSet::builtin(), level(1), false);
        // (3,9) is earth, left of the `H` at (4,9).
        assert!(field.cell(cell(3, 9)).is_untouched());
        assert!(!field.is_passable(cell(4, 9), Direction::Left));

        // A partly dug neighbour is still untouched and not passable.
        let mut field = Field::from_layout(&layout_with(&[(0, 0, b'S')]));
        let mut penalty = Penalty::default();
        field.dig(Px::new(12, 18), Some(Direction::Right), &mut penalty);
        field.dig(Px::new(16, 18), Some(Direction::Right), &mut penalty);
        assert!(field.cell(cell(1, 0)).has_tunnel() && field.cell(cell(1, 0)).is_untouched());
        assert!(!field.is_passable(cell(0, 0), Direction::Right));
    }

    #[test]
    fn adjacent_unconnected_tunnel_not_passable() {
        // Two vertical shafts side by side: neither cell is untouched, but
        // both keep the horizontal slices that face each other.
        let field = Field::from_layout(&layout_with(&[(0, 0, b'V'), (1, 0, b'V')]));
        assert!(!field.cell(cell(1, 0)).is_untouched());
        assert!(!field.is_passable(cell(0, 0), Direction::Right));
        assert!(!field.is_passable(cell(1, 0), Direction::Left));

        // Two horizontal corridors one above the other are not joined
        // vertically either.
        let field = Field::from_layout(&layout_with(&[(0, 0, b'H'), (0, 1, b'H')]));
        assert!(!field.is_passable(cell(0, 0), Direction::Down));
        assert!(!field.is_passable(cell(0, 1), Direction::Up));

        // One facing slice gone on either side is enough.
        let field = Field::from_layout(&layout_with(&[(0, 0, b'H'), (1, 0, b'V')]));
        assert!(field.is_passable(cell(0, 0), Direction::Right));
        assert!(field.is_passable(cell(1, 0), Direction::Left));
    }

    #[test]
    fn not_passable_off_grid() {
        let field = all_open();
        for v in 0..10 {
            assert!(!field.is_passable(cell(14, v), Direction::Right));
            assert!(!field.is_passable(cell(0, v), Direction::Left));
            assert!(field.is_passable(cell(13, v), Direction::Right));
        }
        for h in 0..15 {
            assert!(!field.is_passable(cell(h, 0), Direction::Up));
            assert!(!field.is_passable(cell(h, 9), Direction::Down));
            assert!(field.is_passable(cell(h, 1), Direction::Up));
        }
    }
}
