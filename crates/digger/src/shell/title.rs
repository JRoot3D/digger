//! The title scene: the title picture, the cast animation, mode selection
//! and the title screen's own actions (design D2; `desktop-app` spec, title
//! screen picture, title animation, mode selection, starting a game and
//! other title screen actions; research 01 §2, research 03 §9).
//!
//! The title is drawn on a screen of its own, never on a game's
//! framebuffer. Each frame it first looks at the frame's input (quit, mode
//! change, start), then plays one frame of the 287-frame cast animation.

use digger_core::assets::TitlePicture;
use digger_core::geom::Px;
use digger_core::sprite::SpriteLayer;
use digger_core::video::{Framebuffer, TextColor};

use super::{Context, ShellInput, Transition};
use crate::config::GameMode;
use crate::input::Action;
use crate::scores::ScoreTable;

mod cast;

use cast::Cast;

/// The game's name on the top line, at (100, 0) in colour 3.
const NAME: &str = "D I G G E R";
/// Where the name is drawn.
const NAME_AT: Px = Px::new(100, 0);

/// The y of the mode label's first line.
const LABEL_Y1: i32 = 25;
/// The y of the mode label's second line.
const LABEL_Y2: i32 = 39;
/// The area erased before the first line is drawn: 10 cells from x = 180.
const LABEL_ERASE1: (usize, i32) = (10, 180);
/// The area erased before the second line is drawn: 12 cells from x = 170.
const LABEL_ERASE2: (usize, i32) = (12, 170);

/// The table's left edge.
const TABLE_X: i32 = 16;
/// The y of the `HIGH SCORES` heading.
const TABLE_HEADING_Y: i32 = 25;
/// The cells of the heading and of each row, erased before drawing.
const TABLE_CELLS: usize = 11;

/// The colour of the title's fixed texts and the mode label.
const TITLE_COLOR: TextColor = TextColor::Three;

/// The two lines of `mode`'s label, each with its x (research 01 §2.1).
const fn mode_label(mode: GameMode) -> [(&'static str, i32); 2] {
    match mode {
        GameMode::OnePlayer => [("ONE", 220), (" PLAYER ", 192)],
        GameMode::TwoPlayers => [("TWO", 220), (" PLAYERS", 184)],
        GameMode::TwoDiggers => [("TWO PLAYER", 180), ("SIMULTANEOUS", 170)],
        GameMode::Gauntlet => [("GAUNTLET", 192), ("MODE", 216)],
        GameMode::TwoDiggerGauntlet => [("TWO PLAYER", 180), ("GAUNTLET", 192)],
    }
}

/// The mode the mode-change action selects after `mode`: one player, two
/// players, two players simultaneous, gauntlet, two-player gauntlet, then
/// one player again (research 01 §2.1).
pub(crate) const fn next_mode(mode: GameMode) -> GameMode {
    match mode {
        GameMode::OnePlayer => GameMode::TwoPlayers,
        GameMode::TwoPlayers => GameMode::TwoDiggers,
        GameMode::TwoDiggers => GameMode::Gauntlet,
        GameMode::Gauntlet => GameMode::TwoDiggerGauntlet,
        GameMode::TwoDiggerGauntlet => GameMode::OnePlayer,
    }
}

/// The y of table row `i` (1 to 10): 31 + 13 × i.
fn row_y(i: usize) -> i32 {
    31 + 13 * i32::try_from(i).unwrap_or(0)
}

/// The title screen (design D2).
#[derive(Debug, Clone)]
pub(crate) struct TitleScene {
    /// The shell-owned screen the title is drawn on.
    screen: Framebuffer,
    /// The sprites of the cast animation.
    sprites: SpriteLayer,
    /// The cast animation's actors.
    cast: Cast,
    /// The animation frame played next, 0 to 286.
    frame: u16,
}

impl TitleScene {
    /// The title screen as it opens, for `mode` with its score `table`:
    /// the title picture, the name, the mode label and the table
    /// (research 01 §2, research 03 §9.1–9.2).
    pub(crate) fn new(mode: GameMode, table: &ScoreTable) -> Self {
        let mut screen = Framebuffer::new();
        // The picture is embedded and checked by the core's asset tests;
        // should it ever fail to decode, the screen stays black.
        if let Ok(picture) = TitlePicture::decode() {
            screen.draw_title(&picture);
        }
        screen.draw_text(NAME, NAME_AT, TITLE_COLOR);
        let mut sprites = SpriteLayer::new();
        let cast = Cast::new(&mut sprites);
        let mut scene = Self {
            screen,
            sprites,
            cast,
            frame: 0,
        };
        scene.show_mode(mode, table);
        scene
    }

    /// Shows the label of `mode` and its `table`.
    fn show_mode(&mut self, mode: GameMode, table: &ScoreTable) {
        let screen = &mut self.screen;
        screen.erase_text(LABEL_ERASE1.0, Px::new(LABEL_ERASE1.1, LABEL_Y1));
        screen.erase_text(LABEL_ERASE2.0, Px::new(LABEL_ERASE2.1, LABEL_Y2));
        let [(line1, x1), (line2, x2)] = mode_label(mode);
        screen.draw_text(line1, Px::new(x1, LABEL_Y1), TITLE_COLOR);
        screen.draw_text(line2, Px::new(x2, LABEL_Y2), TITLE_COLOR);

        screen.erase_text(TABLE_CELLS, Px::new(TABLE_X, TABLE_HEADING_Y));
        for i in 1..=table.entries().len() {
            screen.erase_text(TABLE_CELLS, Px::new(TABLE_X, row_y(i)));
        }
        screen.draw_text(
            "HIGH SCORES",
            Px::new(TABLE_X, TABLE_HEADING_Y),
            TITLE_COLOR,
        );
        for (i, entry) in (1..).zip(table.entries()) {
            let color = if i == 1 {
                TextColor::Two
            } else {
                TextColor::One
            };
            screen.draw_text(&entry.row_text(), Px::new(TABLE_X, row_y(i)), color);
        }
    }

    /// Runs one frame of the title screen.
    pub(crate) fn frame(&mut self, input: &ShellInput, cx: &mut Context<'_>) -> Transition {
        if input.close_requested || input.has(Action::Quit) {
            return Transition::Exit(0);
        }
        // Key redefinition (task group 10) and saving the last game's
        // recording (task 9.6) are title actions too; neither starts a
        // game, as the input layer already leaves them out of `start_key`.
        for _ in input
            .input
            .actions
            .iter()
            .filter(|&&a| a == Action::ChangeMode)
        {
            cx.change_mode();
            let mode = *cx.mode;
            let table = *cx.table();
            self.show_mode(mode, &table);
        }
        if input.input.start_key || input.input.pad_start {
            return Transition::To(cx.new_game());
        }
        self.cast
            .step(self.frame, &mut self.screen, &mut self.sprites);
        self.frame = if self.frame >= cast::LAST_FRAME {
            0
        } else {
            self.frame + 1
        };
        Transition::Stay
    }

    /// The picture to present.
    pub(crate) const fn picture(&self) -> &Framebuffer {
        &self.screen
    }
}

#[cfg(test)]
mod tests {
    use super::{GameMode, next_mode};

    #[test]
    fn next_mode_visits_every_mode() {
        let mut mode = GameMode::OnePlayer;
        let mut seen = Vec::new();
        for _ in 0..GameMode::ALL.len() {
            seen.push(mode);
            mode = next_mode(mode);
        }
        assert_eq!(seen, GameMode::ALL);
        assert_eq!(mode, GameMode::OnePlayer);
    }
}
