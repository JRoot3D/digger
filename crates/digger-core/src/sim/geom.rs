//! Playfield geometry: directions, cell positions, playfield limits and
//! cell/pixel conversion (design D1).
//!
//! The playfield is a grid of 15 columns by 10 rows. Cell (h, v) has its
//! top-left corner at logical pixel (h×20+12, v×18+18), and a moving entity
//! stays within x 12..=292 and y 18..=180. Pixel positions are the
//! crate-wide [`Px`] (research 01 §5.1, research 02 §0).

use crate::geom::Px;

/// One of the four directions of movement.
///
/// "No direction" is `Option<Direction>::None`, never a sentinel value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Towards larger x.
    Right,
    /// Towards smaller y.
    Up,
    /// Towards smaller x.
    Left,
    /// Towards larger y.
    Down,
}

impl Direction {
    /// The four directions in the original's numbering order: right, up,
    /// left, down.
    pub const ALL: [Self; 4] = [Self::Right, Self::Up, Self::Left, Self::Down];

    /// The opposite direction.
    #[must_use]
    pub const fn reverse(self) -> Self {
        match self {
            Self::Right => Self::Left,
            Self::Up => Self::Down,
            Self::Left => Self::Right,
            Self::Down => Self::Up,
        }
    }

    /// Whether the direction is left or right.
    pub const fn is_horizontal(self) -> bool {
        matches!(self, Self::Right | Self::Left)
    }

    /// Whether the direction is up or down.
    pub const fn is_vertical(self) -> bool {
        !self.is_horizontal()
    }
}

/// The opposite of an optional direction: no direction stays no direction.
pub const fn reverse_of(direction: Option<Direction>) -> Option<Direction> {
    match direction {
        Some(direction) => Some(direction.reverse()),
        None => None,
    }
}

/// Number of cell columns: 15.
pub(crate) const COLUMNS: u8 = 15;
/// Number of cell rows: 10.
pub(crate) const ROWS: u8 = 10;
/// Width of a cell in logical pixels: 20.
pub(crate) const CELL_WIDTH: i32 = 20;
/// Height of a cell in logical pixels: 18.
pub(crate) const CELL_HEIGHT: i32 = 18;
/// Logical x of the left edge of column 0, and the leftmost position of a
/// moving entity: 12.
pub(crate) const MIN_X: i32 = 12;
/// Rightmost position of a moving entity, the corner of column 14: 292.
pub(crate) const MAX_X: i32 = MIN_X + (COLUMNS as i32 - 1) * CELL_WIDTH;
/// Logical y of the top edge of row 0, and the topmost position of a moving
/// entity: 18.
pub(crate) const MIN_Y: i32 = 18;
/// Lowest position of a moving entity, the corner of row 9: 180.
pub(crate) const MAX_Y: i32 = MIN_Y + (ROWS as i32 - 1) * CELL_HEIGHT;

/// A cell of the 15×10 grid: column `h` (0..=14) and row `v` (0..=9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellPos {
    h: u8,
    v: u8,
}

impl CellPos {
    /// Cell (`h`, `v`), or `None` outside the grid.
    pub const fn new(h: u8, v: u8) -> Option<Self> {
        if h < COLUMNS && v < ROWS {
            Some(Self { h, v })
        } else {
            None
        }
    }

    /// Cell (`h`, `v`) for signed coordinates, or `None` outside the grid.
    pub fn from_signed(h: i32, v: i32) -> Option<Self> {
        Self::new(u8::try_from(h).ok()?, u8::try_from(v).ok()?)
    }

    /// The column, 0..=14.
    pub const fn h(self) -> u8 {
        self.h
    }

    /// The row, 0..=9.
    pub const fn v(self) -> u8 {
        self.v
    }

    /// The logical pixel of the cell's top-left corner: (h×20+12, v×18+18).
    pub const fn to_px(self) -> Px {
        Px::new(
            self.h as i32 * CELL_WIDTH + MIN_X,
            self.v as i32 * CELL_HEIGHT + MIN_Y,
        )
    }

    /// The neighbouring cell in `direction`, or `None` at the grid edge.
    pub fn neighbour(self, direction: Direction) -> Option<Self> {
        let (h, v) = (i32::from(self.h), i32::from(self.v));
        match direction {
            Direction::Right => Self::from_signed(h + 1, v),
            Direction::Up => Self::from_signed(h, v - 1),
            Direction::Left => Self::from_signed(h - 1, v),
            Direction::Down => Self::from_signed(h, v + 1),
        }
    }

    /// Every cell, column by column: (0,0), (0,1), …, (0,9), (1,0), …
    pub fn all_column_major() -> impl Iterator<Item = Self> {
        (0..COLUMNS).flat_map(|h| (0..ROWS).map(move |v| Self { h, v }))
    }
}

/// A pixel position split into its cell and the remainders within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Located {
    /// The cell holding the position.
    pub cell: CellPos,
    /// (x − 12) mod 20: pixels right of the cell's left edge, 0..=19.
    pub rem_x: u8,
    /// (y − 18) mod 18: pixels below the cell's top edge, 0..=17.
    pub rem_y: u8,
}

/// Splits `at` into its cell, h = (x−12) div 20 and v = (y−18) div 18, and
/// the remainders (x−12) mod 20 and (y−18) mod 18 (research 01 §5.1).
///
/// Returns `None` for a position left of or above the grid or beyond its
/// right or bottom edge, where the original's division would name no cell of
/// the grid. Every position within the playfield limits is inside the grid.
pub fn locate(at: Px) -> Option<Located> {
    let (dx, dy) = (at.x - MIN_X, at.y - MIN_Y);
    if dx < 0 || dy < 0 {
        return None;
    }
    let cell = CellPos::from_signed(dx / CELL_WIDTH, dy / CELL_HEIGHT)?;
    Some(Located {
        cell,
        rem_x: u8::try_from(dx % CELL_WIDTH).ok()?,
        rem_y: u8::try_from(dy % CELL_HEIGHT).ok()?,
    })
}

/// Whether `at` lies within the playfield limits: x 12..=292, y 18..=180.
pub const fn within_limits(at: Px) -> bool {
    at.x >= MIN_X && at.x <= MAX_X && at.y >= MIN_Y && at.y <= MAX_Y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_cells_map_to_pixels() {
        assert_eq!(
            CellPos::new(0, 0).map(CellPos::to_px),
            Some(Px::new(12, 18))
        );
        assert_eq!(
            CellPos::new(14, 9).map(CellPos::to_px),
            Some(Px::new(292, 180))
        );
        assert_eq!((MIN_X, MAX_X, MIN_Y, MAX_Y), (12, 292, 18, 180));
        assert!(within_limits(Px::new(12, 18)) && within_limits(Px::new(292, 180)));
        assert!(!within_limits(Px::new(296, 180)) && !within_limits(Px::new(12, 184)));
    }

    #[test]
    fn pixel_60_27_is_cell_2_0_rem_8_9() {
        assert_eq!(
            locate(Px::new(60, 27)),
            Some(Located {
                cell: CellPos { h: 2, v: 0 },
                rem_x: 8,
                rem_y: 9
            })
        );
    }

    #[test]
    fn locate_round_trips_every_corner() {
        for cell in CellPos::all_column_major() {
            assert_eq!(
                locate(cell.to_px()),
                Some(Located {
                    cell,
                    rem_x: 0,
                    rem_y: 0
                })
            );
        }
        assert_eq!(CellPos::all_column_major().count(), 150);
        assert_eq!(locate(Px::new(11, 18)), None);
        assert_eq!(locate(Px::new(12, 17)), None);
        assert_eq!(locate(Px::new(312, 18)), None);
        assert_eq!(locate(Px::new(12, 198)), None);
        assert_eq!(
            locate(Px::new(311, 197)).map(|l| (l.rem_x, l.rem_y)),
            Some((19, 17))
        );
    }

    #[test]
    fn cell_constructor_rejects_outside_grid() {
        assert!(CellPos::new(15, 0).is_none());
        assert!(CellPos::new(0, 10).is_none());
        assert!(CellPos::from_signed(-1, 0).is_none());
        let corner = CellPos::new(0, 0);
        assert_eq!(corner.and_then(|c| c.neighbour(Direction::Left)), None);
        assert_eq!(
            corner.and_then(|c| c.neighbour(Direction::Down)),
            CellPos::new(0, 1)
        );
    }

    #[test]
    fn reverse_of_none_is_none() {
        assert_eq!(reverse_of(None), None);
        for direction in Direction::ALL {
            assert_eq!(reverse_of(Some(direction)), Some(direction.reverse()));
            assert_eq!(direction.reverse().reverse(), direction);
            assert_ne!(direction.reverse(), direction);
            assert_eq!(
                direction.is_horizontal(),
                direction.reverse().is_horizontal()
            );
        }
        assert_eq!(Direction::Up.reverse(), Direction::Down);
        assert_eq!(Direction::Right.reverse(), Direction::Left);
    }
}
