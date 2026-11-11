//! Text and number drawing: opaque 12x12 glyph cells in four colours.

use super::framebuffer::Framebuffer;
use crate::assets::{GLYPH_BYTES, GLYPH_SIDE, Glyph, glyph_for};
use crate::geom::Px;

/// Logical width of one text cell, and the advance from one character to the
/// next (research 03 §6).
pub const CELL_ADVANCE: i32 = 12;

/// Glyph byte of the letter body.
const BODY: u8 = 10;
/// Glyph byte of the right and bottom shadow.
const SHADOW: u8 = 12;

/// One of the four text colours (research 03 §4.7).
///
/// A colour recolours the glyph body (stored as index 10) and shadow (stored
/// as index 12); the background (0) and highlight (15) are drawn unchanged.
/// Under palette 0 colour 0 is green with a red shadow, colour 1 green,
/// colour 2 red and colour 3 yellow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextColor {
    /// Colour 0: body 10, shadow 12, the glyph bytes as stored.
    #[default]
    Zero,
    /// Colour 1: body 10, shadow 2.
    One,
    /// Colour 2: body 12, shadow 4.
    Two,
    /// Colour 3: body 14, shadow 6.
    Three,
}

impl TextColor {
    /// The colour numbered `n` (0 to 3), or `None` for any other number.
    pub const fn new(n: u8) -> Option<Self> {
        match n {
            0 => Some(Self::Zero),
            1 => Some(Self::One),
            2 => Some(Self::Two),
            3 => Some(Self::Three),
            _ => None,
        }
    }

    /// The colour's number, 0 to 3.
    pub const fn number(self) -> u8 {
        match self {
            Self::Zero => 0,
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
        }
    }

    /// The palette indices the body and the shadow are drawn in.
    const fn body_and_shadow(self) -> (u8, u8) {
        match self {
            Self::Zero => (BODY, SHADOW),
            Self::One => (10, 2),
            Self::Two => (12, 4),
            Self::Three => (14, 6),
        }
    }

    /// The palette index a stored glyph byte is drawn as in this colour.
    const fn recolour(self, value: u8) -> u8 {
        let (body, shadow) = self.body_and_shadow();
        match value {
            BODY => body,
            SHADOW => shadow,
            other => other,
        }
    }
}

/// The glyph drawn for a character of a string: its own glyph, or the space
/// glyph when the font has none.
fn glyph_or_space(c: char) -> &'static Glyph {
    // Quirk R5: inside a string a character without a glyph is drawn as a
    // space, which erases its cell (research 03 §6).
    glyph_for(c).unwrap_or(&BLANK_GLYPH)
}

/// The space glyph: every byte background. Identical to the font's own space
/// glyph, which a test checks.
static BLANK_GLYPH: Glyph = [0; GLYPH_BYTES];

/// The logical x of the cell `index` cells right of `x`, saturating far
/// outside the picture, where nothing is drawn anyway.
fn cell_x(x: i32, index: usize) -> i32 {
    let index = i32::try_from(index).unwrap_or(i32::MAX);
    x.saturating_add(index.saturating_mul(CELL_ADVANCE))
}

impl Framebuffer {
    /// Draws character `c` in `color` at logical `at`, overwriting the whole
    /// 12x12 logical cell (24x24 real pixels), background included, clipped
    /// to the picture; `at.x` is used as given (research 03 §4.7).
    ///
    /// A character the font has no glyph for (see
    /// [`glyph_for`]) leaves the picture unchanged.
    /// Lower-case letters are drawn as upper case.
    pub fn draw_char(&mut self, c: char, at: Px, color: TextColor) {
        // Quirk R5: a lone character without a glyph draws nothing.
        if let Some(glyph) = glyph_for(c) {
            self.draw_glyph(glyph, at, color);
        }
    }

    /// Draws `text` left to right from logical `at`, one 12x12 cell per
    /// character, 12 logical pixels apart, all in `color` (research 03 §6).
    ///
    /// Each character is one cell, so a string of n characters covers
    /// `12n` logical columns. A character without a glyph is drawn as a
    /// space and erases its cell; lower-case letters appear as upper case.
    pub fn draw_text(&mut self, text: &str, at: Px, color: TextColor) {
        for (index, c) in text.chars().enumerate() {
            self.draw_glyph(glyph_or_space(c), Px::new(cell_x(at.x, index), at.y), color);
        }
    }

    /// Erases `cells` text cells from logical `at`: identical to drawing a
    /// string of `cells` spaces there (research 03 §6).
    pub fn erase_text(&mut self, cells: usize, at: Px) {
        for index in 0..cells {
            self.draw_glyph(
                &BLANK_GLYPH,
                Px::new(cell_x(at.x, index), at.y),
                TextColor::Zero,
            );
        }
    }

    /// Draws `n` in decimal, right-aligned in a field of `width` cells at
    /// logical `at` (research 03 §6).
    ///
    /// The digit for 10^k goes in the cell at `at.x + (width - 1 - k) * 12`.
    /// The `width - 1` rightmost cells are always drawn, zero-padded; the
    /// leftmost cell is drawn only when its digit is not zero, and is
    /// otherwise left as it was, not erased. Digits above 10^(width - 1) are
    /// not shown. A field of width 0 draws nothing, and a field of width 1
    /// draws nothing for a zero digit.
    pub fn draw_number(&mut self, n: u32, width: usize, at: Px, color: TextColor) {
        let mut rest = n;
        for k in 0..width {
            let digit = rest % 10;
            rest /= 10;
            let leftmost = k + 1 == width;
            // Quirk R6: the leftmost cell is skipped, not erased, for a zero
            // digit, so a stale digit there stays visible.
            if leftmost && digit == 0 {
                continue;
            }
            let Some(c) = char::from_digit(digit, 10) else {
                continue;
            };
            let cell = width - 1 - k;
            self.draw_char(c, Px::new(cell_x(at.x, cell), at.y), color);
        }
    }

    /// Writes `glyph`, recoloured for `color`, opaquely at logical `at`.
    fn draw_glyph(&mut self, glyph: &Glyph, at: Px, color: TextColor) {
        let mut cell = [0u8; GLYPH_BYTES];
        for (dst, &src) in cell.iter_mut().zip(glyph) {
            *dst = color.recolour(src);
        }
        self.put_opaque(at, GLYPH_SIDE, &cell);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = Framebuffer::WIDTH;

    /// A picture whose every pixel is a non-zero index that no glyph value
    /// equals after recolouring in any colour.
    fn filled(value: u8) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.pixels_mut().fill(value);
        fb
    }

    /// The 24x24 real pixels of the cell whose top-left real corner is
    /// (`column`, `row`), row-major.
    fn cell_at(fb: &Framebuffer, column: usize, row: usize) -> Vec<u8> {
        (row..row + GLYPH_SIDE)
            .flat_map(|r| {
                fb.pixels()
                    .get(r * W + column..r * W + column + GLYPH_SIDE)
                    .unwrap_or_else(|| unreachable!("the cell is inside the picture"))
                    .iter()
                    .copied()
            })
            .collect()
    }

    fn glyph(c: char) -> &'static Glyph {
        glyph_for(c).unwrap_or_else(|| unreachable!("{c:?} has a glyph"))
    }

    /// Real pixels that differ between two pictures, as (column, row).
    fn changed(before: &Framebuffer, after: &Framebuffer) -> Vec<(usize, usize)> {
        before
            .pixels()
            .iter()
            .zip(after.pixels())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| (i % W, i / W))
            .collect()
    }

    #[test]
    fn text_cell_is_opaque() {
        let mut fb = filled(9);
        fb.draw_char('A', Px::new(100, 0), TextColor::Zero);
        let cell = cell_at(&fb, 200, 0);
        assert_eq!(cell.len(), 576);
        assert_eq!(cell.as_slice(), glyph('A').as_slice());
        assert!(cell.iter().all(|&b| matches!(b, 0 | 10 | 12 | 15)));
        assert!(cell.contains(&0), "background pixels overwrite the 9s");
        // Nothing outside the cell changed.
        for (column, row) in changed(&filled(9), &fb) {
            assert!(
                (200..224).contains(&column) && row < 24,
                "({column}, {row})"
            );
        }
    }

    #[test]
    fn text_recolour_table() {
        let expected = [
            (TextColor::Zero, 10, 12),
            (TextColor::One, 10, 2),
            (TextColor::Two, 12, 4),
            (TextColor::Three, 14, 6),
        ];
        for (color, body, shadow) in expected {
            let mut fb = Framebuffer::new();
            fb.draw_char('W', Px::new(0, 0), color);
            let cell = cell_at(&fb, 0, 0);
            let stored = glyph('W');
            for (&got, &value) in cell.iter().zip(stored) {
                let want = match value {
                    10 => body,
                    12 => shadow,
                    other => other,
                };
                assert_eq!(got, want, "colour {} glyph byte {value}", color.number());
            }
            assert!(stored.contains(&10) && stored.contains(&12) && stored.contains(&15));
        }
        for n in 0..=3 {
            assert_eq!(TextColor::new(n).map(TextColor::number), Some(n));
        }
        assert_eq!(TextColor::new(4), None);
    }

    #[test]
    fn text_unaligned_position() {
        let mut fb = Framebuffer::new();
        fb.draw_char('8', Px::new(101, 3), TextColor::Three);
        let mut origin = Framebuffer::new();
        origin.draw_char('8', Px::new(0, 0), TextColor::Three);
        assert_eq!(cell_at(&fb, 202, 6), cell_at(&origin, 0, 0));
        let touched = changed(&Framebuffer::new(), &fb);
        let min_col = touched.iter().map(|p| p.0).min();
        let min_row = touched.iter().map(|p| p.1).min();
        assert!(min_col >= Some(202) && min_row >= Some(6));
        assert!(touched.iter().all(|&(c, r)| c < 226 && r < 30));
    }

    #[test]
    fn draw_char_without_glyph_is_noop() {
        let before = filled(9);
        for c in ['-', '!', '/', '\u{e9}', '\n'] {
            let mut fb = before.clone();
            fb.draw_char(c, Px::new(40, 40), TextColor::Two);
            assert_eq!(fb, before, "{c:?}");
        }
    }

    #[test]
    fn text_advance_is_12() {
        let mut fb = Framebuffer::new();
        fb.draw_text("D I G G E R", Px::new(100, 0), TextColor::Three);
        for (index, c) in "D I G G E R".chars().enumerate() {
            let mut single = Framebuffer::new();
            single.draw_char(c, Px::new(0, 0), TextColor::Three);
            assert_eq!(
                cell_at(&fb, 200 + 24 * index, 0),
                cell_at(&single, 0, 0),
                "cell {index}"
            );
        }
        // Covers logical columns 100–231 of rows 0–11 and nothing else.
        let mut over = filled(9);
        over.draw_text("D I G G E R", Px::new(100, 0), TextColor::Three);
        for (column, row) in changed(&filled(9), &over) {
            assert!(
                (200..464).contains(&column) && row < 24,
                "({column}, {row})"
            );
        }
        assert!(cell_at(&over, 200, 0).iter().all(|&b| b != 9));
        assert!(cell_at(&over, 440, 0).iter().all(|&b| b != 9));
    }

    #[test]
    fn text_invalid_char_becomes_space() {
        let mut fb = filled(9);
        fb.draw_text("A-B", Px::new(0, 0), TextColor::Zero);
        assert!(cell_at(&fb, 24, 0).iter().all(|&b| b == 0));
        assert_eq!(cell_at(&fb, 0, 0).as_slice(), glyph('A').as_slice());
        assert_eq!(cell_at(&fb, 48, 0).as_slice(), glyph('B').as_slice());
        // The built-in blank glyph equals the font's space glyph.
        assert_eq!(&BLANK_GLYPH, glyph(' '));
    }

    #[test]
    fn text_lowercase_equals_uppercase() {
        let mut lower = Framebuffer::new();
        let mut upper = Framebuffer::new();
        lower.draw_text("abc", Px::new(30, 50), TextColor::One);
        upper.draw_text("ABC", Px::new(30, 50), TextColor::One);
        assert_eq!(lower, upper);
        assert_ne!(lower, Framebuffer::new());
    }

    #[test]
    fn erase_top_line_covers_320_columns() {
        let mut fb = filled(9);
        fb.erase_text(26, Px::new(0, 0));
        fb.erase_text(1, Px::new(308, 0));
        let rows = fb.pixels().as_chunks::<W>().0;
        assert!(rows.iter().take(24).flatten().all(|&b| b == 0));
        assert!(rows.iter().skip(24).flatten().all(|&b| b == 9));
        // Erasing equals drawing spaces.
        let mut erased = filled(9);
        let mut spaces = filled(9);
        erased.erase_text(5, Px::new(96, 0));
        spaces.draw_text("     ", Px::new(96, 0), TextColor::Two);
        assert_eq!(erased, spaces);
    }

    /// The picture after drawing `text` at `at` in colour 3 on a picture
    /// filled with 9.
    fn text_on_nines(text: &str, at: Px) -> Framebuffer {
        let mut fb = filled(9);
        fb.draw_text(text, at, TextColor::Three);
        fb
    }

    #[test]
    fn number_zero_width_6() {
        let mut fb = filled(9);
        fb.draw_number(0, 6, Px::new(0, 0), TextColor::Three);
        let mut expected = filled(9);
        expected.draw_text("00000", Px::new(12, 0), TextColor::Three);
        assert_eq!(fb, expected);
        assert!(cell_at(&fb, 0, 0).iter().all(|&b| b == 9));
    }

    #[test]
    fn number_six_digits() {
        let mut fb = filled(9);
        fb.draw_number(123_456, 6, Px::new(0, 0), TextColor::Three);
        assert_eq!(fb, text_on_nines("123456", Px::new(0, 0)));
        // Digit k sits at x + (w - 1 - k) * 12: 42 in a six-cell field at
        // x = 236 is "00042" from x = 248, the cell at 236 untouched.
        let mut padded = filled(9);
        padded.draw_number(42, 6, Px::new(236, 0), TextColor::Three);
        assert_eq!(padded, text_on_nines("00042", Px::new(248, 0)));
    }

    #[test]
    fn number_leftmost_cell_not_erased() {
        let mut fb = filled(9);
        fb.draw_number(100_000, 6, Px::new(0, 0), TextColor::Three);
        fb.draw_number(99_999, 6, Px::new(0, 0), TextColor::Three);
        assert_eq!(fb, text_on_nines("199999", Px::new(0, 0)));
    }

    #[test]
    fn number_width_1_zero_draws_nothing() {
        let before = filled(9);
        let mut fb = before.clone();
        fb.draw_number(0, 1, Px::new(50, 50), TextColor::One);
        assert_eq!(fb, before);
        fb.draw_number(10, 1, Px::new(50, 50), TextColor::One);
        assert_eq!(fb, before, "the digit for 10^0 of 10 is zero");
        fb.draw_number(123, 0, Px::new(50, 50), TextColor::One);
        assert_eq!(fb, before, "a field of width 0 draws nothing");
        fb.draw_number(7, 1, Px::new(50, 50), TextColor::One);
        let mut seven = before.clone();
        seven.draw_char('7', Px::new(50, 50), TextColor::One);
        assert_eq!(fb, seven);
    }

    #[test]
    fn number_drops_high_digits() {
        let mut fb = filled(9);
        fb.draw_number(1_234_567, 6, Px::new(0, 0), TextColor::Three);
        assert_eq!(fb, text_on_nines("234567", Px::new(0, 0)));
        let mut fb = filled(9);
        fb.draw_number(1_000_000, 6, Px::new(0, 0), TextColor::Three);
        assert_eq!(fb, text_on_nines("00000", Px::new(12, 0)));
        let mut wide = filled(9);
        wide.draw_number(u32::MAX, 12, Px::new(0, 0), TextColor::Zero);
        let mut expected = filled(9);
        expected.draw_text("04294967295", Px::new(12, 0), TextColor::Zero);
        assert_eq!(wide, expected);
    }
}
