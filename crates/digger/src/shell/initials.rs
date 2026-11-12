//! The new-high-score scene: initials entry (design D2, D10; `high-scores`
//! spec, new high score screen, initials entry and flashing during entry;
//! `input-controls` spec, typing initials and choosing initials with a
//! gamepad; research 01 §6.3, research 05 §1.10).
//!
//! The scene draws on a screen of its own: the scores on the top line,
//! `PLAYER n`, ` NEW HIGH SCORE `, `ENTER YOUR`, ` INITIALS` and three
//! underscores. Only the letters A to Z are accepted, from the text the
//! keyboard layout produces, and are shown and stored in upper case;
//! Backspace and Delete step back one position. A gamepad steps the
//! current position through the alphabet with up and down, accepts the
//! letter with a fire button and steps back with d-pad left. After the
//! third letter the screen stays for 20 more frames.
//!
//! Every frame the picture switches between the normal and the alternate
//! palette, ending on the normal one; with `video.reduce_flashing` it stays
//! in the normal palette.

use digger_core::Direction;
use digger_core::geom::Px;
use digger_core::video::{Framebuffer, Palette, TextColor};

use crate::input::{InputFrame, PadButton, PadInput};

/// The number of initials.
const LETTERS: usize = 3;
/// The frames the screen stays after the third letter (research 01
/// §6.3: 20 `flashywait` calls).
pub(crate) const CLOSING_FRAMES: u8 = 20;
/// The x of the first initial; each further one is 24 pixels right.
const FIRST_X: i32 = 128;
/// The distance between two initials.
const STEP_X: i32 = 24;
/// The row of the initials.
const LETTERS_Y: i32 = 130;
/// The colour of the entry texts and the letters.
const ENTRY_COLOR: TextColor = TextColor::Three;
/// The colour of `PLAYER n` and ` NEW HIGH SCORE `.
const HEADING_COLOR: TextColor = TextColor::Two;
/// The digit positions of a score on the top line.
const SCORE_DIGITS: usize = 6;
/// The x of the second score below 100000 points.
const SECOND_SCORE_X: i32 = 236;
/// The x of the second score from 100000 points.
const SECOND_SCORE_X_SHIFTED: i32 = 248;
/// The score from which the second score is drawn further right.
const SECOND_SHIFT_AT: u32 = 100_000;

/// What a frame of entry decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryStep {
    /// The screen stays.
    Entering,
    /// The entry is complete: the three initials, upper case. The screen
    /// was shown for its last frame on the previous frame.
    Complete([u8; LETTERS]),
}

/// Where the entry stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Waiting for the letter at this position (0 to 2).
    Typing(usize),
    /// All three letters are in; this many closing frames remain.
    Closing(u8),
}

/// The new-high-score screen of one digger.
#[derive(Debug, Clone)]
pub(crate) struct InitialsScene {
    /// The shell-owned screen.
    screen: Framebuffer,
    /// The letters accepted so far, in their positions.
    letters: [u8; LETTERS],
    /// Where the entry stands.
    stage: Stage,
    /// The letter a gamepad has chosen at the current position, if it
    /// moved one (otherwise the position shows its underscore and the
    /// pad's choice is `A`).
    pad_letter: Option<u8>,
    /// Frames run since the screen appeared.
    frames: u32,
    /// Whether the palette alternates (`video.reduce_flashing` off).
    flashing: bool,
}

/// The x of initial `position`.
fn letter_x(position: usize) -> i32 {
    FIRST_X + STEP_X * i32::try_from(position).unwrap_or(0)
}

/// The letter after `letter`, wrapping from Z to A.
const fn next_letter(letter: u8) -> u8 {
    if letter >= b'Z' { b'A' } else { letter + 1 }
}

/// The letter before `letter`, wrapping from A to Z.
const fn previous_letter(letter: u8) -> u8 {
    if letter <= b'A' { b'Z' } else { letter - 1 }
}

/// Draws the top line's scores as the game does: six digit positions,
/// the first score from x = 0, the second from x = 236 (x = 248 from
/// 100000 points), in colour 3 (research 01 §6.2).
fn draw_scores(screen: &mut Framebuffer, scores: &[u32]) {
    for (index, &score) in scores.iter().enumerate() {
        let x = match index {
            0 => 0,
            _ if score < SECOND_SHIFT_AT => SECOND_SCORE_X,
            _ => SECOND_SCORE_X_SHIFTED,
        };
        screen.draw_number(score, SCORE_DIGITS, Px::new(x, 0), ENTRY_COLOR);
    }
}

impl InitialsScene {
    /// The screen for `player` (1 or 2) with the top line's `scores`
    /// (one or two); `flashing` unless `video.reduce_flashing` is set
    /// (research 01 §6.3).
    pub(crate) fn new(player: u8, scores: &[u32], flashing: bool) -> Self {
        let mut screen = Framebuffer::new();
        draw_scores(&mut screen, scores);
        screen.draw_text(&format!("PLAYER {player}"), Px::new(108, 0), HEADING_COLOR);
        screen.draw_text(" NEW HIGH SCORE ", Px::new(64, 40), HEADING_COLOR);
        screen.draw_text("ENTER YOUR", Px::new(100, 70), ENTRY_COLOR);
        screen.draw_text(" INITIALS", Px::new(100, 90), ENTRY_COLOR);
        screen.draw_text("_ _ _", Px::new(FIRST_X, LETTERS_Y), ENTRY_COLOR);
        Self {
            screen,
            letters: [b'_'; LETTERS],
            stage: Stage::Typing(0),
            pad_letter: None,
            frames: 0,
            flashing,
        }
    }

    /// Runs one frame of entry with the frame's `input`; `fire` are the
    /// pad buttons bound to fire.
    pub(crate) fn frame(&mut self, input: &InputFrame, fire: &[PadButton]) -> EntryStep {
        self.frames = self.frames.wrapping_add(1);
        if let Stage::Closing(left) = self.stage {
            if left == 0 {
                return EntryStep::Complete(self.letters);
            }
            self.stage = Stage::Closing(left - 1);
            // The last closing frame shows the normal palette.
            self.set_palette(left == 1);
            return EntryStep::Entering;
        }
        self.take_input(input, fire);
        if let Stage::Typing(LETTERS) = self.stage {
            self.stage = Stage::Closing(CLOSING_FRAMES);
        }
        self.set_palette(false);
        EntryStep::Entering
    }

    /// Applies the frame's keyboard and pad input. Within a frame the
    /// steps back come before the letters typed, then the pad's presses
    /// follow in order.
    fn take_input(&mut self, input: &InputFrame, fire: &[PadButton]) {
        for key in &input.keys {
            if matches!(key.name(), "Backspace" | "Delete") {
                self.step_back();
            }
        }
        for c in input.letters.chars() {
            if c.is_ascii_alphabetic()
                && let Ok(letter) = u8::try_from(c.to_ascii_uppercase())
            {
                self.accept(letter);
            }
        }
        for press in &input.pad_presses {
            match press.input {
                PadInput::DPad(Direction::Up) | PadInput::Stick(Direction::Up) => {
                    self.pad_choose(next_letter);
                }
                PadInput::DPad(Direction::Down) | PadInput::Stick(Direction::Down) => {
                    self.pad_choose(previous_letter);
                }
                PadInput::DPad(Direction::Left) => self.step_back(),
                PadInput::Button(button) if fire.contains(&button) => {
                    let letter = self.pad_letter.unwrap_or(b'A');
                    self.accept(letter);
                }
                _ => {}
            }
        }
    }

    /// The position waiting for a letter, if any.
    const fn position(&self) -> Option<usize> {
        match self.stage {
            Stage::Typing(position) if position < LETTERS => Some(position),
            _ => None,
        }
    }

    /// Puts `letter` (A to Z) at the current position.
    fn accept(&mut self, letter: u8) {
        let Some(position) = self.position() else {
            return;
        };
        if let Some(slot) = self.letters.get_mut(position) {
            *slot = letter;
        }
        self.draw_at(position, char::from(letter));
        self.pad_letter = None;
        self.stage = Stage::Typing(position + 1);
    }

    /// Returns to the previous position and shows its underscore again;
    /// nothing happens at the first position.
    fn step_back(&mut self) {
        let Some(position) = self.position() else {
            return;
        };
        if self.pad_letter.take().is_some() {
            // The pad's choice at this position is withdrawn with it.
            self.draw_at(position, '_');
        }
        if position == 0 {
            return;
        }
        let previous = position - 1;
        if let Some(slot) = self.letters.get_mut(previous) {
            *slot = b'_';
        }
        self.draw_at(previous, '_');
        self.stage = Stage::Typing(previous);
    }

    /// Moves the pad's choice at the current position with `step`.
    fn pad_choose(&mut self, step: fn(u8) -> u8) {
        let Some(position) = self.position() else {
            return;
        };
        let letter = step(self.pad_letter.unwrap_or(b'A'));
        self.pad_letter = Some(letter);
        self.draw_at(position, char::from(letter));
    }

    /// Draws `c` at initial `position`.
    fn draw_at(&mut self, position: usize, c: char) {
        self.screen
            .draw_char(c, Px::new(letter_x(position), LETTERS_Y), ENTRY_COLOR);
    }

    /// Selects this frame's palette: the alternate one on every other
    /// frame while flashing, the normal one when `last` or not flashing.
    fn set_palette(&mut self, last: bool) {
        let alternate = self.flashing && !last && self.frames % 2 == 1;
        self.screen.set_palette(if alternate {
            Palette::One
        } else {
            Palette::Zero
        });
    }

    /// The initials accepted so far, in order, upper case.
    pub(crate) fn typed(&self) -> String {
        let count = match self.stage {
            Stage::Typing(position) => position,
            Stage::Closing(_) => LETTERS,
        };
        self.letters
            .iter()
            .take(count)
            .copied()
            .map(char::from)
            .collect()
    }

    /// The picture to present.
    pub(crate) const fn picture(&self) -> &Framebuffer {
        &self.screen
    }
}
