//! Font glyph lookup.

use super::{FONT, GLYPH_INDEX};

/// Side of a glyph in real pixels: 24x24 real, 12x12 logical (research 03 §6).
pub const GLYPH_SIDE: usize = 24;

/// Bytes in one glyph: 24x24, one palette index per real pixel.
pub const GLYPH_BYTES: usize = GLYPH_SIDE * GLYPH_SIDE;

/// One font glyph, row-major, values 0 (background), 10 (body), 12 (shadow)
/// and 15 (highlight); ink lies in real columns 0–19 (research 03 §6).
pub type Glyph = [u8; GLYPH_BYTES];

/// First character code of the glyph table.
const FIRST_CODE: u32 = 32;

/// The glyph of `c`, or `None` when the font has none.
///
/// 66 of the codes 32–126 have a glyph: space, `.`, `0`–`9`, `:`, `A`–`Z`,
/// `_` and `a`–`z`, the lower-case letters sharing the glyph of their
/// upper-case letter. Every other character, including `-`, `!` and `/`,
/// has none (research 03 §6). Drawing an unsupported character as a space is
/// the text drawing's rule, not the font's.
pub fn glyph_for(c: char) -> Option<&'static Glyph> {
    let slot = u32::from(c).checked_sub(FIRST_CODE)?;
    let glyph = (*GLYPH_INDEX.get(usize::try_from(slot).ok()?)?)?;
    FONT.as_chunks::<GLYPH_BYTES>().0.get(usize::from(glyph))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_glyphs() -> &'static [Glyph] {
        FONT.as_chunks::<GLYPH_BYTES>().0
    }

    #[test]
    fn glyph_coverage_is_66_of_95() {
        let covered: String = (32u8..=126)
            .map(char::from)
            .filter(|&c| glyph_for(c).is_some())
            .collect();
        assert_eq!(
            covered,
            " .0123456789:ABCDEFGHIJKLMNOPQRSTUVWXYZ_abcdefghijklmnopqrstuvwxyz"
        );
        assert_eq!(covered.len(), 66);
        let missing = (32u8..=126)
            .filter(|&c| glyph_for(char::from(c)).is_none())
            .count();
        assert_eq!(missing, 29);
    }

    #[test]
    fn lowercase_shares_uppercase_glyph() {
        for lower in 'a'..='z' {
            let upper = lower.to_ascii_uppercase();
            let (l, u) = (glyph_for(lower), glyph_for(upper));
            assert!(u.is_some(), "{upper} has a glyph");
            assert_eq!(l, u, "{lower} and {upper}");
        }
    }

    #[test]
    fn hyphen_has_no_glyph() {
        for c in [
            '-', '!', '/', '\n', '\u{c8}', '\u{7f}', '\u{1f}', '\u{263a}',
        ] {
            assert_eq!(glyph_for(c), None, "{c:?}");
        }
    }

    #[test]
    fn glyph_values_and_ink_columns() {
        let glyphs = all_glyphs();
        assert_eq!(glyphs.len(), 40);
        assert_eq!(FONT.len(), 23_040);
        for (number, glyph) in glyphs.iter().enumerate() {
            for (row, line) in glyph.as_chunks::<GLYPH_SIDE>().0.iter().enumerate() {
                for (column, &value) in line.iter().enumerate() {
                    assert!(
                        matches!(value, 0 | 10 | 12 | 15),
                        "glyph {number} row {row} column {column}: {value}"
                    );
                    if column >= 20 {
                        assert_eq!(value, 0, "glyph {number} row {row} column {column}");
                    }
                }
            }
        }
    }

    #[test]
    fn space_glyph_is_blank() {
        let space = glyph_for(' ');
        assert!(space.is_some());
        assert!(space.is_some_and(|g| g.iter().all(|&b| b == 0)));
        // The space is the last glyph of font.bin (A–Z, 0–9, '.', ':', '_', ' ').
        assert_eq!(space, all_glyphs().last());
    }
}
