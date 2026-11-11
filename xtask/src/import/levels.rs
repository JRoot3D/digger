//! Level import: the eight built-in 10×15 layouts of `game.c` (research 01
//! §5.5; add-simulation-core task 1.7).
//!
//! Upstream keeps the layouts as the designated member `.leveldat` of the
//! `dgstate` struct initialiser: eight brace lists of ten 15-character
//! string literals. `levels.bin` stores the 1200 characters layout by
//! layout, each layout row by row, with no separators or terminators.

use super::ImportError;
use super::ctable::TableFile;

/// The string-group table holding the layouts.
pub(crate) const TABLE: &str = "dgstate.leveldat";

/// Number of built-in layouts.
pub(crate) const LAYOUTS: usize = 8;
/// Rows per layout.
pub(crate) const ROWS: usize = 10;
/// Characters per row.
pub(crate) const COLUMNS: usize = 15;
/// Length of `levels.bin`: 8 × 10 × 15 = 1200 bytes.
pub(crate) const BLOB_LEN: usize = LAYOUTS * ROWS * COLUMNS;

/// The characters a layout may hold: solid earth, both tunnels, horizontal,
/// vertical, bag and emerald (research 01 §5.5).
pub(crate) const ALPHABET: &[u8] = b" SHVBC";

/// Plans `levels.bin` from `source` (upstream `game.c`).
pub(crate) fn import(source: &TableFile) -> Result<Vec<u8>, ImportError> {
    let file = source.file();
    let layouts = source.string_groups(TABLE)?;
    if layouts.len() != LAYOUTS {
        return Err(ImportError::Count {
            file: file.to_owned(),
            table: TABLE.to_owned(),
            expected: LAYOUTS,
            got: layouts.len(),
        });
    }
    let mut blob = Vec::with_capacity(BLOB_LEN);
    for (layout_index, rows) in layouts.iter().enumerate() {
        let layout = layout_index + 1;
        if rows.len() != ROWS {
            return Err(ImportError::Count {
                file: file.to_owned(),
                table: format!("{TABLE} (layout {layout})"),
                expected: ROWS,
                got: rows.len(),
            });
        }
        for (row, text) in rows.iter().enumerate() {
            if text.len() != COLUMNS {
                return Err(ImportError::Length {
                    file: file.to_owned(),
                    table: format!("{TABLE} (layout {layout}, row {row})"),
                    expected: COLUMNS,
                    got: text.len(),
                });
            }
            if let Some((column, &byte)) = text
                .iter()
                .enumerate()
                .find(|(_, byte)| !ALPHABET.contains(byte))
            {
                return Err(ImportError::Character {
                    file: file.to_owned(),
                    table: TABLE.to_owned(),
                    layout,
                    row,
                    column,
                    byte,
                });
            }
            blob.extend_from_slice(text);
        }
    }
    Ok(blob)
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::{BLOB_LEN, import};
    use crate::import::ImportError;
    use crate::import::ctable::parse;
    use crate::import::fixture::{fixture_layouts, game_c};

    fn import_text(text: &str) -> Result<Vec<u8>, ImportError> {
        import(&parse("game.c", text.as_bytes())?)
    }

    #[test]
    fn levels_import_fixture() -> Result<(), Box<dyn Error>> {
        let layouts = fixture_layouts();
        let blob = import_text(&game_c(&layouts))?;
        assert_eq!(blob.len(), BLOB_LEN);
        let expected: Vec<u8> = layouts
            .iter()
            .flatten()
            .flat_map(|row| row.bytes())
            .collect();
        assert_eq!(blob, expected);
        // Layout-major, row-major: layout 2 row 0 starts at byte 150.
        assert_eq!(
            blob.get(150..165),
            layouts.get(1).and_then(|l| l.first()).map(String::as_bytes)
        );
        assert_eq!(
            blob.get(1185..),
            layouts.get(7).and_then(|l| l.get(9)).map(String::as_bytes)
        );
        Ok(())
    }

    #[test]
    fn levels_wrong_row_length_is_error() {
        let mut layouts = fixture_layouts();
        if let Some(row) = layouts.get_mut(2).and_then(|layout| layout.get_mut(4)) {
            row.push('C');
        }
        let error = import_text(&game_c(&layouts)).err();
        assert!(
            matches!(&error, Some(ImportError::Length { table, expected: 15, got: 16, .. })
                if table == "dgstate.leveldat (layout 3, row 4)"),
            "{error:?}"
        );
    }

    #[test]
    fn levels_wrong_count_is_error() {
        let mut layouts = fixture_layouts();
        layouts.pop();
        let error = import_text(&game_c(&layouts)).err();
        assert!(
            matches!(&error, Some(ImportError::Count { table, expected: 8, got: 7, .. })
                if table == "dgstate.leveldat"),
            "{error:?}"
        );
        let mut layouts = fixture_layouts();
        if let Some(layout) = layouts.get_mut(5) {
            layout.pop();
        }
        let error = import_text(&game_c(&layouts)).err();
        assert!(
            matches!(&error, Some(ImportError::Count { table, expected: 10, got: 9, .. })
                if table == "dgstate.leveldat (layout 6)"),
            "{error:?}"
        );
    }

    #[test]
    fn levels_character_outside_alphabet_is_error() {
        let mut layouts = fixture_layouts();
        if let Some(row) = layouts.get_mut(0).and_then(|layout| layout.get_mut(1)) {
            row.replace_range(3..4, "x");
        }
        let error = import_text(&game_c(&layouts)).err();
        assert!(
            matches!(
                &error,
                Some(ImportError::Character {
                    layout: 1,
                    row: 1,
                    column: 3,
                    byte: b'x',
                    ..
                })
            ),
            "{error:?}"
        );
        let message = error.map(|e| e.to_string()).unwrap_or_default();
        assert!(message.contains("layout 1, row 1, column 3"), "{message}");
    }

    #[test]
    fn levels_missing_table_is_error() {
        let error = import_text("int main(void) { return 0; }\n").err();
        assert!(
            matches!(&error, Some(ImportError::MissingTable { table, .. }) if table == "dgstate.leveldat"),
            "{error:?}"
        );
    }
}
