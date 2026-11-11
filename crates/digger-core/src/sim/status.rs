//! The status line: scores, lives, gauntlet clock and top-line messages
//! (design D1).
//!
//! The status line is logical rows 0..=11 of the picture: 12-pixel opaque
//! text cells and 16×12 life icons (research 03 §8.5). Numbers are
//! formatted into fixed-size stack buffers, so nothing here allocates
//! (design D9).

use crate::assets::ImageId;
use crate::geom::Px;
use crate::video::{CELL_ADVANCE, Framebuffer, TextColor};

use super::ids::PlayerId;

/// The status line's row.
const TOP: i32 = 0;

/// Life-icon slots of the first side, left to right (research 03 §8.5).
const FIRST_SLOTS: [i32; 4] = [80, 100, 120, 140];
/// Life-icon slots of the second side, in the order spare lives fill them.
const SECOND_SLOTS: [i32; 4] = [224, 204, 184, 164];
/// More spare lives than this are shown as one icon and a count.
const MAX_ICONS: u32 = 4;
/// Where the first side's `X<n>` count starts.
const FIRST_COUNT_X: i32 = 100;
/// Where the second side's `<n>X` count ends (exclusive).
const SECOND_COUNT_END: i32 = 220;
/// Cells erased before the first side's lives are drawn, from x = 96.
const FIRST_ERASE: (usize, i32) = (5, 96);
/// Cells erased before the second side's lives are drawn, from x = 164.
const SECOND_ERASE: (usize, i32) = (5, 164);

/// Where the gauntlet clock is drawn.
const CLOCK_X: i32 = 124;
/// Clock units per second: the clock counts 1,193,181ths of a second
/// (research 01 §7.4).
const CLOCK_UNITS_PER_SECOND: u32 = 1_193_181;

/// Where the `PLAYER n` label is drawn, and its width in cells.
const PLAYER_LABEL: (i32, usize) = (108, 8);

/// A short line of text built on the stack.
#[derive(Debug, Clone, Copy)]
struct StackText<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> StackText<N> {
    const fn new() -> Self {
        Self {
            bytes: [b' '; N],
            len: 0,
        }
    }

    /// Appends the ASCII byte `b`; a full buffer drops it.
    fn push(&mut self, b: u8) {
        if let Some(slot) = self.bytes.get_mut(self.len) {
            *slot = b;
            self.len += 1;
        }
    }

    /// Appends `n` in decimal, at least `width` characters wide, padded on
    /// the left with `pad`.
    fn push_number(&mut self, n: u32, width: usize, pad: u8) {
        let mut digits = [0u8; 10];
        let mut count = 0;
        let mut rest = n;
        loop {
            if let Some(slot) = digits.get_mut(count) {
                // `rest % 10` is a single digit.
                *slot = b'0' + u8::try_from(rest % 10).unwrap_or(0);
            }
            count += 1;
            rest /= 10;
            if rest == 0 {
                break;
            }
        }
        for _ in count..width {
            self.push(pad);
        }
        for &digit in digits.iter().take(count).rev() {
            self.push(digit);
        }
    }

    /// The number of characters.
    const fn len(&self) -> usize {
        self.len
    }

    /// The text; only ASCII is ever pushed.
    fn as_str(&self) -> &str {
        self.bytes
            .get(..self.len)
            .and_then(|bytes| core::str::from_utf8(bytes).ok())
            .unwrap_or("")
    }
}

/// Who the second side of the lives display belongs to (research 03 §8.5).
#[allow(dead_code, reason = "constructed by the round flow (task group 7)")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SecondSide {
    /// The second player of an alternating game: icon 111, count in text
    /// colour 2.
    Player2 {
        /// The second player's lives.
        lives: u32,
    },
    /// The second digger of a simultaneous game: icon 113, count in text
    /// colour 1.
    Digger2 {
        /// The second digger's lives.
        lives: u32,
    },
}

impl SecondSide {
    const fn lives(self) -> u32 {
        match self {
            Self::Player2 { lives } | Self::Digger2 { lives } => lives,
        }
    }

    const fn icon(self) -> ImageId {
        match self {
            Self::Player2 { .. } => ImageId::LIFE_PLAYER_2,
            Self::Digger2 { .. } => ImageId::LIFE_SECOND_DIGGER,
        }
    }

    const fn count_color(self) -> TextColor {
        match self {
            Self::Player2 { .. } => TextColor::Two,
            Self::Digger2 { .. } => TextColor::One,
        }
    }
}

/// Draws one life icon (or blank slot) at `x` of the status line. No sprite
/// can overlap the status line, so the icon is drawn unprotected
/// (research 03 §7.5).
fn draw_life(fb: &mut Framebuffer, icon: ImageId, x: i32) {
    fb.blit_masked(icon, Px::new(x, TOP));
}

/// Draws the lives display outside gauntlet mode: spare lives (lives − 1)
/// of the first player or digger as up to four icons at x = 80, 100, 120,
/// 140, blank where none remains, or one icon at 80 and `X<n>` from x = 100
/// for more than four; and, mirrored, those of `second` (playfield spec,
/// "Lives display"; research 01 §6.5, research 03 §8.5).
#[allow(dead_code, reason = "called by the round flow (task group 7)")]
pub(crate) fn draw_lives(fb: &mut Framebuffer, first: u32, second: Option<SecondSide>) {
    let (cells, x) = FIRST_ERASE;
    fb.erase_text(cells, Px::new(x, TOP));
    let spare = first.saturating_sub(1);
    if spare > MAX_ICONS {
        draw_life(fb, ImageId::LIFE_PLAYER_1, FIRST_SLOTS[0]);
        let mut text = StackText::<12>::new();
        text.push(b'X');
        text.push_number(spare, 0, b' ');
        fb.draw_text(text.as_str(), Px::new(FIRST_COUNT_X, TOP), TextColor::Two);
    } else {
        for (slot, x) in (0..).zip(FIRST_SLOTS) {
            let icon = if slot < spare {
                ImageId::LIFE_PLAYER_1
            } else {
                ImageId::LIFE_BLANK
            };
            draw_life(fb, icon, x);
        }
    }

    let Some(side) = second else {
        return;
    };
    let (cells, x) = SECOND_ERASE;
    fb.erase_text(cells, Px::new(x, TOP));
    let spare = side.lives().saturating_sub(1);
    if spare > MAX_ICONS {
        let mut text = StackText::<12>::new();
        text.push_number(spare, 0, b' ');
        text.push(b'X');
        let width = i32::try_from(text.len()).unwrap_or(0) * CELL_ADVANCE;
        fb.draw_text(
            text.as_str(),
            Px::new(SECOND_COUNT_END - width, TOP),
            side.count_color(),
        );
        draw_life(fb, side.icon(), SECOND_SLOTS[0]);
    } else {
        for (slot, x) in (0..).zip(SECOND_SLOTS) {
            let icon = if slot < spare {
                side.icon()
            } else {
                ImageId::LIFE_BLANK
            };
            draw_life(fb, icon, x);
        }
    }
}

/// Draws the gauntlet clock in place of the lives: minutes right-aligned in
/// three characters, a colon and two-digit seconds, at x = 124 in text
/// colour 3, where the seconds shown are `clock` (in 1,193,181ths of a
/// second) divided by 1,193,181, rounded down (playfield spec, "Gauntlet
/// clock display"; research 01 §6.5, §7.4).
#[allow(dead_code, reason = "called by the gauntlet tick (task group 12)")]
pub(crate) fn draw_gauntlet_clock(fb: &mut Framebuffer, clock: u32) {
    let seconds = clock / CLOCK_UNITS_PER_SECOND;
    let mut text = StackText::<8>::new();
    text.push_number(seconds / 60, 3, b' ');
    text.push(b':');
    text.push_number(seconds % 60, 2, b'0');
    fb.draw_text(text.as_str(), Px::new(CLOCK_X, TOP), TextColor::Three);
}

/// Erases the whole top line: 26 cells from x = 0 and one from x = 308
/// (research 01 §3.3, research 03 §8.5).
#[allow(dead_code, reason = "called by the round flow (task group 7)")]
pub(crate) fn clear_top_line(fb: &mut Framebuffer) {
    fb.erase_text(26, Px::new(0, TOP));
    fb.erase_text(1, Px::new(308, TOP));
}

/// Draws `PLAYER 1` or `PLAYER 2` at (108, 0) in `color`, as the player
/// flash does (game-flow spec; research 01 §3.2, research 03 §8.5).
#[allow(dead_code, reason = "called by the player flash (task group 7)")]
pub(crate) fn draw_player_label(fb: &mut Framebuffer, player: PlayerId, color: TextColor) {
    let mut text = StackText::<8>::new();
    for &b in b"PLAYER " {
        text.push(b);
    }
    text.push_number(u32::from(player.number()) + 1, 1, b' ');
    let (x, _) = PLAYER_LABEL;
    fb.draw_text(text.as_str(), Px::new(x, TOP), color);
}

/// Erases the eight cells of the `PLAYER n` label at (108, 0).
#[allow(dead_code, reason = "called by the round start (task group 7)")]
pub(crate) fn erase_player_label(fb: &mut Framebuffer) {
    let (x, cells) = PLAYER_LABEL;
    fb.erase_text(cells, Px::new(x, TOP));
}

/// A message shown alone on the top line (game-flow spec; research 03
/// §8.5).
#[allow(
    dead_code,
    reason = "constructed by the pause and notice phases (task group 7)"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TopLineMessage {
    /// `PRESS ANY KEY` at (80, 0) in text colour 1, while paused.
    PressAnyKey,
    /// `GAME OVER` at (104, 0) in text colour 3.
    GameOver,
    /// `TIME UP` at (120, 0) in text colour 3.
    TimeUp,
}

#[allow(
    dead_code,
    reason = "shown by the pause and notice phases (task group 7)"
)]
impl TopLineMessage {
    /// The message's text.
    pub(crate) const fn text(self) -> &'static str {
        match self {
            Self::PressAnyKey => "PRESS ANY KEY",
            Self::GameOver => "GAME OVER",
            Self::TimeUp => "TIME UP",
        }
    }

    /// Where the message starts.
    pub(crate) const fn at(self) -> Px {
        let x = match self {
            Self::PressAnyKey => 80,
            Self::GameOver => 104,
            Self::TimeUp => 120,
        };
        Px::new(x, TOP)
    }

    /// The message's text colour.
    pub(crate) const fn color(self) -> TextColor {
        match self {
            Self::PressAnyKey => TextColor::One,
            Self::GameOver | Self::TimeUp => TextColor::Three,
        }
    }

    /// Clears the top line and shows the message.
    pub(crate) fn show(self, fb: &mut Framebuffer) {
        clear_top_line(fb);
        fb.draw_text(self.text(), self.at(), self.color());
    }

    /// Erases the message's cells, leaving the rest of the top line.
    pub(crate) fn erase(self, fb: &mut Framebuffer) {
        fb.erase_text(self.text().len(), self.at());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = Framebuffer::WIDTH;

    /// The real rows of the status line, 0..24, from real column 0.
    fn status_rows(fb: &Framebuffer) -> Vec<&[u8]> {
        fb.pixels()
            .as_chunks::<W>()
            .0
            .iter()
            .take(24)
            .map(<[u8; W]>::as_slice)
            .collect()
    }

    /// The real pixels of the logical rectangle at (`x`, 0) that is `w`
    /// logical pixels wide and 12 high.
    fn block(fb: &Framebuffer, x: i32, w: i32) -> Vec<u8> {
        let (Ok(x), Ok(w)) = (usize::try_from(x), usize::try_from(w)) else {
            unreachable!()
        };
        status_rows(fb)
            .iter()
            .flat_map(|row| {
                row.get(2 * x..2 * (x + w))
                    .unwrap_or_default()
                    .iter()
                    .copied()
            })
            .collect()
    }

    /// A picture with only `image` drawn at (`x`, 0).
    fn icon_at(image: ImageId, x: i32) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.blit_masked(image, Px::new(x, 0));
        fb
    }

    fn text_at(text: &str, x: i32, color: TextColor) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.draw_text(text, Px::new(x, 0), color);
        fb
    }

    /// Whether the 16-pixel-wide slot at `x` shows `image`.
    fn shows(fb: &Framebuffer, image: ImageId, x: i32) -> bool {
        block(fb, x, 16) == block(&icon_at(image, x), x, 16)
    }

    fn player(n: u8) -> PlayerId {
        PlayerId::new(n).unwrap_or_else(|| unreachable!())
    }

    #[test]
    fn stack_text_formats_numbers() {
        let mut text = StackText::<12>::new();
        text.push_number(0, 0, b' ');
        assert_eq!(text.as_str(), "0");
        let mut text = StackText::<12>::new();
        text.push_number(7, 3, b' ');
        text.push(b':');
        text.push_number(5, 2, b'0');
        assert_eq!(text.as_str(), "  7:05");
        let mut text = StackText::<12>::new();
        text.push_number(u32::MAX, 0, b' ');
        assert_eq!(text.as_str(), "4294967295");
        let mut full = StackText::<2>::new();
        full.push_number(123, 0, b' ');
        assert_eq!(full.as_str(), "12", "a full buffer drops the rest");
    }

    #[test]
    fn three_lives_show_two_icons() {
        let mut fb = Framebuffer::new();
        draw_lives(&mut fb, 3, None);
        assert!(shows(&fb, ImageId::LIFE_PLAYER_1, 80));
        assert!(shows(&fb, ImageId::LIFE_PLAYER_1, 100));
        assert!(shows(&fb, ImageId::LIFE_BLANK, 120));
        assert!(shows(&fb, ImageId::LIFE_BLANK, 140));
        assert!(!shows(&fb, ImageId::LIFE_BLANK, 100));
        // One life left shows four blanks; the second side is untouched.
        let mut fb = Framebuffer::new();
        fb.draw_text("ABCDE", Px::new(164, 0), TextColor::Three);
        let right = block(&fb, 164, 60);
        draw_lives(&mut fb, 1, None);
        for x in FIRST_SLOTS {
            assert!(shows(&fb, ImageId::LIFE_BLANK, x), "{x}");
        }
        assert_eq!(block(&fb, 164, 60), right);
        // Drawing fewer lives over more replaces the icons.
        let mut fb = Framebuffer::new();
        draw_lives(&mut fb, 9, None);
        draw_lives(&mut fb, 2, None);
        let mut fresh = Framebuffer::new();
        draw_lives(&mut fresh, 2, None);
        assert_eq!(fb, fresh, "the count text is erased");
    }

    #[test]
    fn seven_lives_show_x6() {
        let mut fb = Framebuffer::new();
        draw_lives(&mut fb, 7, None);
        assert!(shows(&fb, ImageId::LIFE_PLAYER_1, 80));
        let x6 = text_at("X6", 100, TextColor::Two);
        assert_eq!(block(&fb, 100, 24), block(&x6, 100, 24));
        // Five lives (four spare) are still icons.
        let mut fb = Framebuffer::new();
        draw_lives(&mut fb, 5, None);
        for x in FIRST_SLOTS {
            assert!(shows(&fb, ImageId::LIFE_PLAYER_1, x), "{x}");
        }
        // Larger counts grow to the right.
        let mut fb = Framebuffer::new();
        draw_lives(&mut fb, 124, None);
        let x123 = text_at("X123", 100, TextColor::Two);
        assert_eq!(block(&fb, 100, 48), block(&x123, 100, 48));
    }

    #[test]
    fn second_side_is_mirrored() {
        // Alternating game: player 2 with 3 lives fills 224 and 204.
        let mut fb = Framebuffer::new();
        draw_lives(&mut fb, 3, Some(SecondSide::Player2 { lives: 3 }));
        assert!(shows(&fb, ImageId::LIFE_PLAYER_2, 224));
        assert!(shows(&fb, ImageId::LIFE_PLAYER_2, 204));
        assert!(shows(&fb, ImageId::LIFE_BLANK, 184));
        assert!(shows(&fb, ImageId::LIFE_BLANK, 164));
        assert!(shows(&fb, ImageId::LIFE_PLAYER_1, 80));

        // Simultaneous game: the second digger's icon.
        let mut fb = Framebuffer::new();
        draw_lives(&mut fb, 2, Some(SecondSide::Digger2 { lives: 2 }));
        assert!(shows(&fb, ImageId::LIFE_SECOND_DIGGER, 224));
        assert!(shows(&fb, ImageId::LIFE_BLANK, 204));

        // Many lives: the count ends at x = 220 and the icon sits at 224.
        let mut fb = Framebuffer::new();
        draw_lives(&mut fb, 3, Some(SecondSide::Player2 { lives: 7 }));
        assert!(shows(&fb, ImageId::LIFE_PLAYER_2, 224));
        let six_x = text_at("6X", 196, TextColor::Two);
        assert_eq!(block(&fb, 196, 24), block(&six_x, 196, 24));
        let mut fb = Framebuffer::new();
        draw_lives(&mut fb, 3, Some(SecondSide::Digger2 { lives: 12 }));
        assert!(shows(&fb, ImageId::LIFE_SECOND_DIGGER, 224));
        let eleven_x = text_at("11X", 184, TextColor::One);
        assert_eq!(block(&fb, 184, 36), block(&eleven_x, 184, 36));
    }

    #[test]
    fn gauntlet_120s_shows_2_00() {
        let mut fb = Framebuffer::new();
        draw_gauntlet_clock(&mut fb, 120 * CLOCK_UNITS_PER_SECOND);
        assert_eq!(fb, text_at("  2:00", 124, TextColor::Three));
        // No life icon is drawn.
        assert!(block(&fb, 80, 44).iter().all(|&b| b == 0));
        // Rounded down: one unit short of 120 seconds shows 1:59.
        let mut fb = Framebuffer::new();
        draw_gauntlet_clock(&mut fb, 120 * CLOCK_UNITS_PER_SECOND - 1);
        assert_eq!(fb, text_at("  1:59", 124, TextColor::Three));
        let mut fb = Framebuffer::new();
        draw_gauntlet_clock(&mut fb, u32::MAX);
        assert_eq!(fb, text_at(" 59:59", 124, TextColor::Three));
        let mut fb = Framebuffer::new();
        draw_gauntlet_clock(&mut fb, 0);
        assert_eq!(fb, text_at("  0:00", 124, TextColor::Three));
    }

    #[test]
    fn top_line_messages_at_spec_positions() {
        let cases = [
            (
                TopLineMessage::PressAnyKey,
                "PRESS ANY KEY",
                80,
                TextColor::One,
            ),
            (TopLineMessage::GameOver, "GAME OVER", 104, TextColor::Three),
            (TopLineMessage::TimeUp, "TIME UP", 120, TextColor::Three),
        ];
        for (message, text, x, color) in cases {
            let mut fb = text_at("000250", 0, TextColor::Three);
            fb.blit_masked(ImageId::LIFE_PLAYER_1, Px::new(80, 0));
            fb.draw_text("12", Px::new(296, 0), TextColor::Two);
            message.show(&mut fb);
            assert_eq!(fb, text_at(text, x, color), "{message:?} alone on the line");
            message.erase(&mut fb);
            assert_eq!(fb, Framebuffer::new(), "{message:?} erased");
        }

        // Clearing the line blanks rows 0..=11 across all 320 columns and
        // nothing below.
        let mut fb = Framebuffer::new();
        fb.draw_text("ABCDEFGHIJKLMNOPQRSTUVWXYZ0", Px::new(0, 0), TextColor::Two);
        fb.draw_text("ROW", Px::new(0, 12), TextColor::Two);
        let below = text_at("", 0, TextColor::Zero);
        clear_top_line(&mut fb);
        let mut row = below;
        row.draw_text("ROW", Px::new(0, 12), TextColor::Two);
        assert_eq!(fb, row);

        // `PLAYER n` at 108 in the given colour, erased over 8 cells.
        for (n, label) in [(0, "PLAYER 1"), (1, "PLAYER 2")] {
            for color in [TextColor::One, TextColor::Two, TextColor::Three] {
                let mut fb = Framebuffer::new();
                draw_player_label(&mut fb, player(n), color);
                assert_eq!(fb, text_at(label, 108, color));
                erase_player_label(&mut fb);
                assert_eq!(fb, Framebuffer::new());
            }
        }
    }
}
