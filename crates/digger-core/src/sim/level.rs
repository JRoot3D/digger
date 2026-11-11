//! Level layouts, the level-number-to-layout sequence, the difficulty index,
//! custom level data and the two-digger start-cell substitution (design D1).

/// Number of built-in layouts.
const BUILTIN_LAYOUTS: usize = 8;
/// Rows of a layout.
const LAYOUT_ROWS: usize = 10;
/// Characters in a layout row.
const LAYOUT_COLUMNS: usize = 15;

/// The eight built-in 10×15 layouts imported from upstream
/// (`assets/levels.bin`, see `docs/assets.md`): layout by layout, each row
/// by row, one character per cell from the alphabet ` SHVBC`. The array
/// type checks the length at compile time (research 01 §5.5).
#[allow(dead_code, reason = "read by LevelSet::builtin (task 2.2)")]
pub(super) static BUILTIN_LEVELS: &[u8; BUILTIN_LAYOUTS * LAYOUT_ROWS * LAYOUT_COLUMNS] = include_bytes!(
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/levels.bin")
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_levels_hold_eight_layouts_over_the_alphabet() {
        assert_eq!(BUILTIN_LEVELS.len(), 1200);
        assert!(BUILTIN_LEVELS.iter().all(|byte| b" SHVBC".contains(byte)));
        let (rows, rest) = BUILTIN_LEVELS.as_chunks::<LAYOUT_COLUMNS>();
        assert_eq!(rest, &[] as &[u8]);
        assert_eq!(rows.len(), BUILTIN_LAYOUTS * LAYOUT_ROWS);
        // Layout 1, rows 0 and 9 (playfield spec, "Built-in layout 1").
        assert_eq!(rows.first(), Some(b"S   B     HHHHS"));
        assert_eq!(rows.get(9), Some(b"CC  HHHHHHH  CC"));
    }
}
