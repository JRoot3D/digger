//! The key redefinition scene (design D2, D7; `input-controls` spec, key
//! redefinition flow, redefinition conflicts, aborting redefinition and
//! redefinition is saved; research 05 §1.3).
//!
//! The scene draws on a screen of its own: `PRESS NEW KEY FOR` on the top
//! line, then one line per action it asks for, the action being asked for
//! in colour 2 and turning to colour 1 once a key is accepted. It always
//! asks for player 1's five keys; in a mode with two Diggers it then asks
//! for player 2's five under `PLAYER 2:` (and labels the first five
//! `PLAYER 1:`). After that it walks the remaining actions in table order
//! and asks for each one whose keys clash with a key of an earlier action;
//! with the "all" variant it asks for every function action under
//! `OTHER:`. In a mode with one Digger, a player-2 key that player 1 took
//! is replaced by player 1's old key for the same control, and player 2's
//! action is asked for only when that key is taken as well. A line that would reach the bottom of the screen starts a
//! second column.
//!
//! The scene works on a copy of the key table: the pressed key becomes the
//! asked action's only key in the copy, which is handed to the shell only
//! once every action is settled. Aborting with the quit key, or closing
//! the window, drops the copy, so the keys in effect never change half
//! way.

use digger_core::geom::Px;
use digger_core::video::{Framebuffer, TextColor};

use super::{Context, ShellInput, Transition};
use crate::input::{Action, KeyName};

/// The key table: the keys of every action, in [`Action::ALL`] order.
pub(crate) type KeyTable = [Vec<KeyName>; Action::COUNT];

/// The screen's heading.
pub(crate) const HEADING: &str = "PRESS NEW KEY FOR";
/// The header above player 1's keys in a mode with two Diggers.
pub(crate) const PLAYER1_HEADER: &str = "PLAYER 1:";
/// The header above player 2's keys.
pub(crate) const PLAYER2_HEADER: &str = "PLAYER 2:";
/// The header above the function actions with the "all" variant.
pub(crate) const OTHER_HEADER: &str = "OTHER:";

/// The height of one line: one text cell (research 05 §1.3, `CHR_H`).
pub(crate) const LINE_HEIGHT: i32 = 12;
/// A line starting at this y or lower moves to the second column: the
/// screen's 200 rows less one line (`MAX_H - CHR_H`).
pub(crate) const WRAP_Y: i32 = 200 - LINE_HEIGHT;
/// The x of the second column: half of the 26 cells of a line.
pub(crate) const SECOND_COLUMN_X: i32 = 13 * 12;

/// The colour of the heading and the headers.
const HEADER_COLOR: TextColor = TextColor::Three;
/// The colour of the action being asked for.
const ASKING_COLOR: TextColor = TextColor::Two;
/// The colour of an action whose key was accepted.
const DONE_COLOR: TextColor = TextColor::One;

/// The name the screen shows for `action` (upstream's key names, upper
/// case as the font draws them; the fullscreen action is the port's).
pub(crate) const fn label(action: Action) -> &'static str {
    match action {
        Action::P1Right | Action::P2Right => "RIGHT",
        Action::P1Up | Action::P2Up => "UP",
        Action::P1Left | Action::P2Left => "LEFT",
        Action::P1Down | Action::P2Down => "DOWN",
        Action::P1Fire | Action::P2Fire => "FIRE",
        Action::SpeedUp => "ACCEL",
        Action::SpeedDown => "BRAKE",
        Action::ToggleMusic => "MUSIC",
        Action::ToggleSound => "SOUND",
        Action::Quit => "EXIT",
        Action::Pause => "PAUSE",
        Action::ChangeMode => "MODE CHANGE",
        Action::SaveRecording => "SAVE DRF",
        Action::RedefineKeys => "REDEFINE",
        Action::Fullscreen => "FULLSCREEN",
    }
}

/// Which actions are asked for besides the player keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Variant {
    /// Only the actions whose keys clash with a newly chosen key.
    Usual,
    /// Every function action (`--redefine-keys=all`).
    All,
}

/// What a frame of redefinition decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RedefineStep {
    /// The screen keeps asking.
    Asking,
    /// The player pressed the quit key: nothing changes.
    Aborted,
    /// Every action is settled: the complete new key table.
    Complete(Box<KeyTable>),
}

/// Where the next line goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cursor {
    /// The position of the next line.
    at: Px,
    /// The y the second column starts at: the first line below the
    /// heading and the `PLAYER 1:` header.
    column_top: i32,
}

impl Cursor {
    /// The position of a new line, moving to the second column when the
    /// line would reach the bottom of the screen.
    fn line(&mut self) -> Px {
        if self.at.y >= WRAP_Y {
            self.at = Px::new(SECOND_COLUMN_X, self.column_top);
        }
        let at = self.at;
        self.at = Px::new(at.x, at.y + LINE_HEIGHT);
        at
    }
}

/// The action being asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Asking {
    /// The action.
    action: Action,
    /// Where its name is drawn.
    at: Px,
}

/// The key redefinition screen (design D2).
#[derive(Debug, Clone)]
pub(crate) struct RedefineScene {
    /// The shell-owned screen.
    screen: Framebuffer,
    /// The working copy of the key table.
    keys: KeyTable,
    /// The key table as it was when the scene opened.
    before: KeyTable,
    /// Which actions are asked for besides the player keys.
    variant: Variant,
    /// Whether the mode has two Diggers, so player 2's keys are always
    /// asked for.
    two_diggers: bool,
    /// Where the next line goes.
    cursor: Cursor,
    /// Whether the `PLAYER 2:` header is shown.
    player2_header: bool,
    /// Whether the `OTHER:` header is shown.
    other_header: bool,
    /// The action being asked for, `None` once every action is settled.
    asking: Option<Asking>,
}

impl RedefineScene {
    /// The screen as it opens, over the keys in effect, asking for player
    /// 1's right key.
    pub(crate) fn new(keys: &KeyTable, two_diggers: bool, variant: Variant) -> Self {
        let mut screen = Framebuffer::new();
        screen.draw_text(HEADING, Px::new(0, 0), HEADER_COLOR);
        let mut cursor = Cursor {
            at: Px::new(0, LINE_HEIGHT),
            column_top: LINE_HEIGHT,
        };
        if two_diggers {
            let at = cursor.line();
            screen.draw_text(PLAYER1_HEADER, at, HEADER_COLOR);
            cursor.column_top = cursor.at.y;
        }
        let mut scene = Self {
            screen,
            keys: keys.clone(),
            before: keys.clone(),
            variant,
            two_diggers,
            cursor,
            player2_header: false,
            other_header: false,
            asking: None,
        };
        scene.ask_from(0);
        scene
    }

    /// Whether the action at `index` is asked for: player 1's keys always,
    /// player 2's in a mode with two Diggers, every function action with
    /// the "all" variant, and any other action when one of its keys is
    /// also a key of an earlier action.
    fn must_ask(&self, index: usize) -> bool {
        let Some(action) = Action::ALL.get(index).copied() else {
            return false;
        };
        let always = match action.control() {
            Some(control) => control.slot == 0 || self.two_diggers,
            None => self.variant == Variant::All,
        };
        always
            || self
                .keys
                .get(index)
                .is_some_and(|keys| keys.iter().any(|&key| self.earlier_owner(index, key)))
    }

    /// Whether `key` is a key of an action before the one at `index`.
    fn earlier_owner(&self, index: usize, key: KeyName) -> bool {
        self.keys.iter().take(index).any(|keys| keys.contains(&key))
    }

    /// Finds the first action from `index` on that must be asked for and
    /// shows it in colour 2, with the headers that come before it.
    fn ask_from(&mut self, index: usize) {
        self.asking = None;
        for index in index..Action::COUNT {
            self.hand_over_player2_key(index);
            if !self.must_ask(index) {
                continue;
            }
            let Some(action) = Action::ALL.get(index).copied() else {
                break;
            };
            self.headers_before(action);
            let at = self.cursor.line();
            self.screen.draw_text(label(action), at, ASKING_COLOR);
            self.asking = Some(Asking { action, at });
            return;
        }
    }

    /// In a mode with one Digger, player 2's keys are not asked for: when
    /// the action at `index` is one of player 2's and lost a key to an
    /// earlier action, it takes player 1's old keys for the same control
    /// that are still free instead (a port decision, compat quirk SH9:
    /// the configuration file allows a key for one action only). If none
    /// is free, the clash remains and the action is asked for.
    fn hand_over_player2_key(&mut self, index: usize) {
        let Some(control) = Action::ALL.get(index).and_then(|action| action.control()) else {
            return;
        };
        if control.slot != 1 || self.two_diggers {
            return;
        }
        let Some(current) = self.keys.get(index) else {
            return;
        };
        let kept: Vec<KeyName> = current
            .iter()
            .copied()
            .filter(|&key| !self.earlier_owner(index, key))
            .collect();
        if kept.len() == current.len() {
            return;
        }
        let player1 = index - 5;
        let free: Vec<KeyName> = self
            .before
            .get(player1)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&key| !self.earlier_owner(index, key) && !kept.contains(&key))
            .collect();
        if kept.is_empty() && free.is_empty() {
            return;
        }
        if let Some(keys) = self.keys.get_mut(index) {
            *keys = kept.into_iter().chain(free).collect();
        }
    }

    /// Draws the header that belongs above `action`, once.
    fn headers_before(&mut self, action: Action) {
        let player2 = action.control().is_some_and(|control| control.slot == 1);
        if player2 && !self.player2_header {
            self.player2_header = true;
            let at = self.cursor.line();
            self.screen.draw_text(PLAYER2_HEADER, at, HEADER_COLOR);
        }
        if action.control().is_none() && self.variant == Variant::All && !self.other_header {
            self.other_header = true;
            let at = self.cursor.line();
            self.screen.draw_text(OTHER_HEADER, at, HEADER_COLOR);
        }
    }

    /// Runs one frame of the screen: closing the window ends the program
    /// without saving anything; the quit key returns to the title screen
    /// with the keys as they were; once every action is settled the new
    /// keys are handed to the shell and the title screen returns.
    pub(crate) fn frame(&mut self, input: &ShellInput, cx: &mut Context<'_>) -> Transition {
        if input.close_requested {
            return Transition::Exit(0);
        }
        match self.keys_pressed(&input.input.keys) {
            RedefineStep::Asking => Transition::Stay,
            RedefineStep::Aborted => Transition::To(cx.title()),
            RedefineStep::Complete(keys) => {
                cx.redefined(keys);
                Transition::To(cx.title())
            }
        }
    }

    /// The action being asked for.
    pub(crate) fn asking(&self) -> Option<Action> {
        self.asking.map(|asking| asking.action)
    }

    /// Handles the new key presses of one frame, in order. Keys pressed
    /// after the one that completes or aborts the redefinition are
    /// ignored.
    pub(crate) fn keys_pressed(&mut self, keys: &[KeyName]) -> RedefineStep {
        for &key in keys {
            match self.press(key) {
                RedefineStep::Asking => {}
                step => return step,
            }
        }
        RedefineStep::Asking
    }

    /// Handles one key press.
    fn press(&mut self, key: KeyName) -> RedefineStep {
        let Some(Asking { action, at }) = self.asking else {
            return RedefineStep::Complete(Box::new(self.keys.clone()));
        };
        // The quit key in effect abandons the redefinition, unless quit
        // itself is being asked for (research 05 §1.3; restoring every
        // key is a port decision, compat quirk SH8).
        if action != Action::Quit && self.keys_of(Action::Quit).contains(&key) {
            return RedefineStep::Aborted;
        }
        // Alt cannot be bound; a key of an earlier action is refused and
        // the same action asked for again.
        if key.is_alt() || self.earlier_owner(action.index(), key) {
            return RedefineStep::Asking;
        }
        if let Some(keys) = self.keys.get_mut(action.index()) {
            *keys = vec![key];
        }
        self.screen.draw_text(label(action), at, DONE_COLOR);
        self.ask_from(action.index() + 1);
        if self.asking.is_none() {
            return RedefineStep::Complete(Box::new(self.keys.clone()));
        }
        RedefineStep::Asking
    }

    /// The keys of `action` in the working copy.
    fn keys_of(&self, action: Action) -> &[KeyName] {
        self.keys.get(action.index()).map_or(&[], Vec::as_slice)
    }

    /// The picture to present.
    pub(crate) const fn picture(&self) -> &Framebuffer {
        &self.screen
    }
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::{Cursor, LINE_HEIGHT, RedefineScene, RedefineStep, SECOND_COLUMN_X, Variant};
    use crate::config::Settings;
    use crate::input::{Action, KeyName};
    use digger_core::geom::Px;

    fn key(name: &str) -> KeyName {
        KeyName::parse(name).unwrap_or_else(|| panic!("{name} is a key"))
    }

    #[test]
    fn cursor_wraps_to_second_column() {
        let mut cursor = Cursor {
            at: Px::new(0, 180),
            column_top: 24,
        };
        assert_eq!(cursor.line(), Px::new(0, 180));
        assert_eq!(cursor.line(), Px::new(SECOND_COLUMN_X, 24));
        assert_eq!(cursor.line(), Px::new(SECOND_COLUMN_X, 24 + LINE_HEIGHT));
    }

    #[test]
    fn pressed_key_becomes_only_binding() {
        let keys = Settings::default().keys;
        let mut scene = RedefineScene::new(&keys, false, Variant::Usual);
        let step = scene.keys_pressed(&[
            key("KeyD"),
            key("KeyE"),
            key("KeyA"),
            key("KeyX"),
            key("ControlLeft"),
        ]);
        // KeyA was player 2's left: in one-player mode player 2's left
        // takes player 1's old left key instead of being asked for.
        let RedefineStep::Complete(table) = step else {
            panic!("complete after the five player keys, got {step:?}");
        };
        assert_eq!(table[Action::P1Left.index()], [key("KeyA")]);
        assert_eq!(table[Action::P1Fire.index()], [key("ControlLeft")]);
        assert_eq!(table[Action::P2Left.index()], [key("ArrowLeft")]);
        assert_eq!(table[Action::Pause.index()], [key("Space")]);
        let settings = Settings {
            keys: *table,
            ..Settings::default()
        };
        assert_eq!(settings.duplicate_key(), None);
    }

    #[test]
    fn player2_asked_when_old_key_taken_too() {
        let keys = Settings::default().keys;
        let mut scene = RedefineScene::new(&keys, false, Variant::Usual);
        // Player 1's right takes ArrowLeft, its left takes KeyA: player 2's
        // left cannot fall back on ArrowLeft and is asked for.
        let step = scene.keys_pressed(&[
            key("ArrowLeft"),
            key("ArrowUp"),
            key("KeyA"),
            key("ArrowDown"),
            key("F1"),
        ]);
        assert_eq!(step, RedefineStep::Asking);
        assert_eq!(scene.asking(), Some(Action::P2Left));
        // Taken keys are refused for it too.
        assert_eq!(scene.keys_pressed(&[key("KeyA")]), RedefineStep::Asking);
        let RedefineStep::Complete(table) = scene.keys_pressed(&[key("KeyJ")]) else {
            panic!("complete after player 2's left");
        };
        assert_eq!(table[Action::P2Left.index()], [key("KeyJ")]);
    }
}
