//! `InputState`: held and pressed-since-frame flags per control, the action
//! queue and the typed-letter queue (design D7).
//!
//! The platform layer feeds key, text, modifier, focus and pad events in as
//! they arrive; once per frame the shell calls [`InputState::take_frame`],
//! which returns everything collected since the previous frame as an
//! [`InputFrame`] and starts the next one.
//!
//! - **Player controls.** Each of the ten controls is held while at least
//!   one key or pad input bound to it is down, and is also flagged as
//!   "pressed since the previous frame" by every press. The frame reports
//!   `held || pressed` and clears the flag, so a tap shorter than a frame is
//!   seen exactly once and a release is seen on the next frame (research 05
//!   §1.5–1.6, without the original's 30-entry FIFO). The core turns these
//!   raw states into one direction and a fire latch (compat quirks IN1–IN3).
//! - **Function actions** are edge-triggered: a key or button press that is
//!   not a repeat queues its action once, so holding a key never repeats it
//!   and several keys in one frame each act once (IN5). A press counts as a
//!   repeat when the platform says so or when the key is already down.
//! - **Any key.** `any_key` is set by a new press of any key that is not a
//!   modifier (Shift, Control, Alt, Meta) and not the fullscreen key or
//!   chord; it ends a pause. `start_key` is the same minus the keys of the
//!   actions the title screen excludes (`desktop-app` spec, starting a
//!   game). A key already held when a scene starts never sets either again:
//!   only new presses do.
//! - **Alt+Enter.** Enter or keypad Enter pressed while Alt is held is the
//!   fullscreen chord: it queues [`Action::Fullscreen`] and nothing else, so
//!   neither key reaches the game, starts a game or ends a pause.
//! - **Focus loss** releases every key and pad input, forgets the sticky
//!   presses and reports `focus_lost`, so a game can pause and nothing moves
//!   by itself after focus returns.
//! - **Typed letters** for initials entry come from the text the keyboard
//!   layout produces, never from physical key names; only the ASCII letters
//!   are kept, in the case typed (the initials scene upper-cases them), and
//!   repeated or Alt-modified text is dropped.

use std::collections::HashMap;

use digger_core::Controls;

use super::gamepad::{PadEvent, PadInput, PadSignal, Pads};
use super::{Action, Bindings, Control, ControlKind, KeyName, PadAction};

/// One key-down event from the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct KeyPress<'a> {
    /// The physical key, or `None` when the platform could not identify
    /// it.
    pub(crate) key: Option<KeyName>,
    /// Whether the platform reports this as keyboard auto-repeat.
    pub(crate) repeat: bool,
    /// The text the keyboard layout produces for this press.
    pub(crate) text: Option<&'a str>,
}

/// A pad being connected or removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PadChange {
    /// A pad now plays for this slot.
    Connected(u8),
    /// The pad of this slot was removed.
    Removed(u8),
}

/// A new press of a pad input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PadPress {
    /// The slot of the pad.
    pub(crate) slot: u8,
    /// The input pressed.
    pub(crate) input: PadInput,
}

/// Everything the input layer collected for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent per-frame flags, each read by a different scene"
)]
pub(crate) struct InputFrame {
    /// The raw controls of the two slots: held now or pressed since the
    /// previous frame.
    pub(crate) slots: [Controls; 2],
    /// The function actions triggered, in the order of their presses.
    pub(crate) actions: Vec<Action>,
    /// Every new key press, in order, modifiers included, the keys of the
    /// fullscreen chord not.
    pub(crate) keys: Vec<KeyName>,
    /// The letters typed, in order.
    pub(crate) letters: String,
    /// A new press of a key other than a modifier or fullscreen.
    pub(crate) any_key: bool,
    /// `any_key` without the keys the title screen excludes.
    pub(crate) start_key: bool,
    /// Every new press of a pad input, in order.
    pub(crate) pad_presses: Vec<PadPress>,
    /// A new press of a pad button or d-pad button (not the stick).
    pub(crate) any_button: bool,
    /// A new press of a pad button bound to fire or pause.
    pub(crate) pad_start: bool,
    /// The window lost focus during this frame.
    pub(crate) focus_lost: bool,
    /// Pads connected or removed, in order.
    pub(crate) pad_changes: Vec<PadChange>,
}

/// The live input state between two frames (design D7).
#[derive(Debug, Clone)]
pub(crate) struct InputState {
    /// The bindings presses are looked up in.
    bindings: Bindings,
    /// How many keys and pad inputs hold each control.
    held: [u8; Control::COUNT],
    /// Each control pressed since the previous frame.
    pressed: [bool; Control::COUNT],
    /// The keys down now, with the control each one holds.
    keys_down: HashMap<KeyName, Option<Control>>,
    /// The pad inputs down now, with the control each one holds.
    pad_down: HashMap<(u8, PadInput), Option<Control>>,
    /// The platform's Alt modifier state.
    alt_modifier: bool,
    /// The rest of the frame being collected.
    frame: InputFrame,
}

/// Whether `key` is an Enter key of the Alt+Enter chord.
fn is_enter(key: KeyName) -> bool {
    matches!(key.name(), "Enter" | "NumpadEnter")
}

impl InputState {
    /// Nothing held, looking presses up in `bindings`.
    pub(crate) fn new(bindings: Bindings) -> Self {
        Self {
            bindings,
            held: [0; Control::COUNT],
            pressed: [false; Control::COUNT],
            keys_down: HashMap::new(),
            pad_down: HashMap::new(),
            alt_modifier: false,
            frame: InputFrame::default(),
        }
    }

    /// The bindings in effect.
    pub(crate) fn bindings(&self) -> &Bindings {
        &self.bindings
    }

    /// Replaces the bindings (after redefinition). Keys already down
    /// release the control they pressed.
    pub(crate) fn set_bindings(&mut self, bindings: Bindings) {
        self.bindings = bindings;
    }

    /// Whether Alt is held.
    fn alt_held(&self) -> bool {
        self.alt_modifier || self.keys_down.keys().any(|key| key.is_alt())
    }

    /// The platform's Alt modifier state changed.
    pub(crate) fn set_alt(&mut self, held: bool) {
        self.alt_modifier = held;
    }

    /// Holds `control` and flags it as pressed.
    fn hold(&mut self, control: Control) {
        if let Some(count) = self.held.get_mut(control.index()) {
            *count = count.saturating_add(1);
        }
        if let Some(flag) = self.pressed.get_mut(control.index()) {
            *flag = true;
        }
    }

    /// Lets go of one hold of `control`.
    fn release(&mut self, control: Control) {
        if let Some(count) = self.held.get_mut(control.index()) {
            *count = count.saturating_sub(1);
        }
    }

    /// A key went down.
    pub(crate) fn key_down(&mut self, press: KeyPress<'_>) {
        // Compat quirk IN5: a repeat never triggers anything.
        let repeat = press.repeat
            || press
                .key
                .is_some_and(|key| self.keys_down.contains_key(&key));
        if repeat {
            return;
        }
        let alt = self.alt_held();
        let Some(key) = press.key else {
            // An unidentified key: it binds nothing, but counts as a key
            // and may type.
            if !alt {
                self.frame.any_key = true;
                self.frame.start_key = true;
                self.type_text(press.text);
            }
            return;
        };
        if alt && is_enter(key) {
            // The fullscreen chord (desktop-app spec, fullscreen): nothing
            // else sees it.
            self.keys_down.insert(key, None);
            self.frame.actions.push(Action::Fullscreen);
            return;
        }
        let action = self.bindings.action_of(key);
        let control = action.and_then(Action::control);
        self.keys_down.insert(key, control);
        self.frame.keys.push(key);
        match (control, action) {
            (Some(control), _) => self.hold(control),
            (None, Some(action)) => self.frame.actions.push(action),
            (None, None) => {}
        }
        if !key.is_modifier() && action != Some(Action::Fullscreen) {
            self.frame.any_key = true;
            if !action.is_some_and(Action::excluded_from_start) {
                self.frame.start_key = true;
            }
        }
        if !alt {
            self.type_text(press.text);
        }
    }

    /// Queues the ASCII letters of `text`.
    fn type_text(&mut self, text: Option<&str>) {
        if let Some(text) = text {
            self.frame
                .letters
                .extend(text.chars().filter(char::is_ascii_alphabetic));
        }
    }

    /// A key went up.
    pub(crate) fn key_up(&mut self, key: KeyName) {
        if let Some(Some(control)) = self.keys_down.remove(&key) {
            self.release(control);
        }
    }

    /// The window lost keyboard focus: everything counts as released.
    pub(crate) fn focus_lost(&mut self) {
        self.held = [0; Control::COUNT];
        self.pressed = [false; Control::COUNT];
        self.keys_down.clear();
        self.pad_down.clear();
        self.alt_modifier = false;
        self.frame.focus_lost = true;
    }

    /// Passes `event` through `pads` and applies the resulting signals.
    pub(crate) fn pad_event(&mut self, pads: &mut Pads, event: PadEvent) {
        let mut signals = Vec::new();
        pads.handle(event, &mut signals);
        for signal in signals {
            self.pad_signal(signal);
        }
    }

    /// Applies one pad signal.
    pub(crate) fn pad_signal(&mut self, signal: PadSignal) {
        match signal {
            PadSignal::Connected(slot) => self.frame.pad_changes.push(PadChange::Connected(slot)),
            PadSignal::Removed(slot) => {
                let released: Vec<Option<Control>> = self
                    .pad_down
                    .iter()
                    .filter(|((pad_slot, _), _)| *pad_slot == slot)
                    .map(|(_, control)| *control)
                    .collect();
                self.pad_down.retain(|(pad_slot, _), _| *pad_slot != slot);
                for control in released.into_iter().flatten() {
                    self.release(control);
                }
                self.frame.pad_changes.push(PadChange::Removed(slot));
            }
            PadSignal::Input {
                slot,
                input,
                pressed: true,
            } => self.pad_press(slot, input),
            PadSignal::Input {
                slot,
                input,
                pressed: false,
            } => {
                if let Some(Some(control)) = self.pad_down.remove(&(slot, input)) {
                    self.release(control);
                }
            }
        }
    }

    /// A pad input went down.
    fn pad_press(&mut self, slot: u8, input: PadInput) {
        if self.pad_down.contains_key(&(slot, input)) {
            return;
        }
        let kind = match input {
            PadInput::DPad(direction) | PadInput::Stick(direction) => {
                Some(ControlKind::of_direction(direction))
            }
            PadInput::Button(button) => match self.bindings.pad_action_of(button) {
                Some(PadAction::Fire) => Some(ControlKind::Fire),
                Some(action) => {
                    if let Some(action) = action.action() {
                        self.frame.actions.push(action);
                    }
                    None
                }
                None => None,
            },
        };
        let control = kind.map(|kind| Control { slot, kind });
        self.pad_down.insert((slot, input), control);
        if let Some(control) = control {
            self.hold(control);
        }
        self.frame.pad_presses.push(PadPress { slot, input });
        if let PadInput::Button(button) = input {
            self.frame.any_button = true;
            if matches!(
                self.bindings.pad_action_of(button),
                Some(PadAction::Fire | PadAction::Pause)
            ) {
                self.frame.pad_start = true;
            }
        } else if matches!(input, PadInput::DPad(_)) {
            self.frame.any_button = true;
        }
    }

    /// Everything collected since the previous call; starts the next
    /// frame.
    pub(crate) fn take_frame(&mut self) -> InputFrame {
        // Compat quirks IN1-IN4: held || pressed-since-frame per control,
        // player-1 keys to slot 1 and player-2 keys to slot 2 in every mode.
        let mut frame = std::mem::take(&mut self.frame);
        for (slot, controls) in frame.slots.iter_mut().enumerate() {
            let state = |kind: ControlKind| {
                let index = slot * 5 + kind as usize;
                self.held.get(index).is_some_and(|count| *count > 0)
                    || self.pressed.get(index).copied().unwrap_or(false)
            };
            *controls = Controls {
                up: state(ControlKind::Up),
                down: state(ControlKind::Down),
                left: state(ControlKind::Left),
                right: state(ControlKind::Right),
                fire: state(ControlKind::Fire),
            };
        }
        self.pressed = [false; Control::COUNT];
        frame
    }
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use digger_core::{Controls, Direction};

    use super::{InputFrame, InputState, KeyPress};
    use crate::config::Settings;
    use crate::input::{Action, Bindings, KeyName, PadButton, PadInput, PadSignal};

    fn key(name: &str) -> KeyName {
        KeyName::parse(name).unwrap_or_else(|| panic!("{name} is a key"))
    }

    fn state() -> InputState {
        InputState::new(Bindings::from_settings(&Settings::default()))
    }

    fn down(state: &mut InputState, name: &str) {
        state.key_down(KeyPress {
            key: Some(key(name)),
            ..KeyPress::default()
        });
    }

    fn repeat(state: &mut InputState, name: &str) {
        state.key_down(KeyPress {
            key: Some(key(name)),
            repeat: true,
            text: None,
        });
    }

    fn up(state: &mut InputState, name: &str) {
        state.key_up(key(name));
    }

    fn typed(state: &mut InputState, name: &str, text: &str) {
        state.key_down(KeyPress {
            key: Some(key(name)),
            repeat: false,
            text: Some(text),
        });
    }

    const LEFT: Controls = Controls {
        left: true,
        ..Controls::NONE
    };

    #[test]
    fn input_tap_is_seen_once() {
        let mut input = state();
        down(&mut input, "ArrowLeft");
        up(&mut input, "ArrowLeft");
        assert_eq!(input.take_frame().slots, [LEFT, Controls::NONE]);
        assert_eq!(input.take_frame().slots, [Controls::NONE; 2]);
        // A tap on fire, and one on player 2's keys.
        down(&mut input, "F1");
        up(&mut input, "F1");
        down(&mut input, "KeyW");
        up(&mut input, "KeyW");
        let frame = input.take_frame();
        assert!(frame.slots[0].fire && !frame.slots[0].left);
        assert!(frame.slots[1].up);
        assert_eq!(input.take_frame().slots, [Controls::NONE; 2]);
    }

    #[test]
    fn input_held_persists() {
        let mut input = state();
        down(&mut input, "ArrowLeft");
        for _ in 0..5 {
            assert_eq!(input.take_frame().slots, [LEFT, Controls::NONE]);
            // Auto-repeat changes nothing.
            repeat(&mut input, "ArrowLeft");
        }
        // Held by a key and a pad d-pad: both must let go.
        input.pad_signal(PadSignal::Input {
            slot: 0,
            input: PadInput::DPad(Direction::Left),
            pressed: true,
        });
        up(&mut input, "ArrowLeft");
        assert_eq!(input.take_frame().slots, [LEFT, Controls::NONE]);
        input.pad_signal(PadSignal::Input {
            slot: 0,
            input: PadInput::DPad(Direction::Left),
            pressed: false,
        });
        assert_eq!(input.take_frame().slots, [Controls::NONE; 2]);
    }

    #[test]
    fn input_release_clears() {
        let mut input = state();
        down(&mut input, "ArrowRight");
        down(&mut input, "Tab");
        let frame = input.take_frame();
        assert!(frame.slots[0].right && frame.slots[1].fire);
        up(&mut input, "ArrowRight");
        let frame = input.take_frame();
        assert!(!frame.slots[0].right, "released on the next frame");
        assert!(frame.slots[1].fire, "the other key is still held");
        up(&mut input, "Tab");
        assert_eq!(input.take_frame().slots, [Controls::NONE; 2]);
        // Rebinding while a key is held: its release lets go of the control
        // it pressed, and the key then acts on its new binding.
        down(&mut input, "KeyD");
        let mut settings = Settings::default();
        settings.keys[Action::P1Right.index()] = vec![key("KeyD")];
        input.set_bindings(Bindings::from_settings(&settings));
        assert_eq!(
            input.bindings().action_of(key("KeyD")),
            Some(Action::P1Right)
        );
        assert_eq!(input.take_frame().slots, [Controls::NONE; 2]);
        up(&mut input, "KeyD");
        down(&mut input, "ArrowRight");
        assert_eq!(input.take_frame().slots, [Controls::NONE; 2]);
        up(&mut input, "ArrowRight");
        down(&mut input, "KeyD");
        assert!(input.take_frame().slots[0].right);
        up(&mut input, "KeyD");
        // A release of a key never pressed (held before the window had
        // focus) is ignored.
        up(&mut input, "ArrowDown");
        assert_eq!(input.take_frame().slots, [Controls::NONE; 2]);
    }

    #[test]
    fn action_once_per_press() {
        let mut input = state();
        down(&mut input, "NumpadAdd");
        for _ in 0..25 {
            repeat(&mut input, "NumpadAdd");
        }
        // A key-down without the repeat flag for a key already down is a
        // repeat too.
        down(&mut input, "NumpadAdd");
        assert_eq!(input.take_frame().actions, [Action::SpeedUp]);
        for _ in 0..10 {
            repeat(&mut input, "NumpadAdd");
            assert_eq!(input.take_frame().actions, []);
        }
        up(&mut input, "NumpadAdd");
        down(&mut input, "NumpadAdd");
        assert_eq!(input.take_frame().actions, [Action::SpeedUp]);
        // Function keys never hold a player control.
        assert_eq!(input.take_frame().slots, [Controls::NONE; 2]);
        // A pad's pause button once per press as well.
        let start = PadSignal::Input {
            slot: 0,
            input: PadInput::Button(PadButton::Start),
            pressed: true,
        };
        input.pad_signal(start);
        input.pad_signal(start);
        assert_eq!(input.take_frame().actions, [Action::Pause]);
    }

    #[test]
    fn two_actions_same_frame() {
        let mut input = state();
        down(&mut input, "F7");
        down(&mut input, "F9");
        repeat(&mut input, "F7");
        up(&mut input, "F7");
        let frame = input.take_frame();
        assert_eq!(frame.actions, [Action::ToggleMusic, Action::ToggleSound]);
        assert_eq!(input.take_frame().actions, []);
        // Both speed keys of one action, in one frame: each is a press.
        down(&mut input, "Equal");
        down(&mut input, "NumpadAdd");
        assert_eq!(
            input.take_frame().actions,
            [Action::SpeedUp, Action::SpeedUp]
        );
    }

    #[test]
    fn any_key_flags() {
        let mut input = state();
        // Modifiers are no key.
        for name in ["ShiftLeft", "ControlRight", "MetaLeft"] {
            down(&mut input, name);
        }
        let frame = input.take_frame();
        assert!(!frame.any_key && !frame.start_key);
        assert_eq!(frame.keys.len(), 3, "but they are reported as keys");
        // Excluded actions end a pause but do not start a game.
        for name in ["KeyN", "KeyK", "F10", "F8", "F9", "F7", "Equal", "Minus"] {
            down(&mut input, name);
            let frame = input.take_frame();
            assert!(frame.any_key && !frame.start_key, "{name}");
        }
        // Fullscreen is neither.
        down(&mut input, "F11");
        let frame = input.take_frame();
        assert!(!frame.any_key && !frame.start_key);
        assert_eq!(frame.actions, [Action::Fullscreen]);
        // Player keys, pause and unbound keys are both.
        for name in ["Space", "ArrowUp", "Tab", "KeyQ", "Digit5", "Enter"] {
            down(&mut input, name);
            let frame = input.take_frame();
            assert!(frame.any_key && frame.start_key, "{name}");
        }
        // A key held from before is never a new press.
        repeat(&mut input, "KeyQ");
        down(&mut input, "KeyQ");
        let frame = input.take_frame();
        assert!(!frame.any_key && !frame.start_key);
    }

    #[test]
    fn alt_enter_not_any_key() {
        let mut input = state();
        down(&mut input, "AltLeft");
        typed(&mut input, "Enter", "\r");
        let frame = input.take_frame();
        assert_eq!(frame.actions, [Action::Fullscreen]);
        assert!(!frame.any_key && !frame.start_key);
        assert_eq!(frame.keys, [key("AltLeft")], "Enter is not reported");
        assert_eq!(frame.letters, "");
        // Releasing Enter, then Alt, gives nothing either.
        up(&mut input, "Enter");
        up(&mut input, "AltLeft");
        assert_eq!(input.take_frame(), InputFrame::default());
        // Keypad Enter with the platform's Alt modifier (Alt held from
        // before focus, so no key event for it).
        input.set_alt(true);
        down(&mut input, "NumpadEnter");
        let frame = input.take_frame();
        assert_eq!(frame.actions, [Action::Fullscreen]);
        assert!(!frame.any_key);
        up(&mut input, "NumpadEnter");
        input.set_alt(false);
        // Enter bound to fire is still caught by the chord.
        let mut settings = Settings::default();
        settings.keys[Action::P1Fire.index()] = vec![key("Enter")];
        let mut input = InputState::new(Bindings::from_settings(&settings));
        down(&mut input, "AltRight");
        down(&mut input, "Enter");
        let frame = input.take_frame();
        assert!(!frame.slots[0].fire);
        assert_eq!(frame.actions, [Action::Fullscreen]);
        // Without Alt, Enter is an ordinary key.
        up(&mut input, "Enter");
        up(&mut input, "AltRight");
        down(&mut input, "Enter");
        let frame = input.take_frame();
        assert!(frame.slots[0].fire && frame.any_key);
        assert_eq!(frame.actions, []);
    }

    #[test]
    fn focus_loss_releases_all() {
        let mut input = state();
        down(&mut input, "ArrowRight");
        down(&mut input, "KeyS");
        down(&mut input, "AltLeft");
        input.pad_signal(PadSignal::Input {
            slot: 1,
            input: PadInput::Button(PadButton::South),
            pressed: true,
        });
        // A tap just before focus goes is forgotten too.
        down(&mut input, "F1");
        up(&mut input, "F1");
        input.focus_lost();
        let frame = input.take_frame();
        assert!(frame.focus_lost);
        assert_eq!(frame.slots, [Controls::NONE; 2]);
        let frame = input.take_frame();
        assert!(!frame.focus_lost, "reported once");
        assert_eq!(frame.slots, [Controls::NONE; 2]);
        // The releases arriving later are harmless, and a key pressed after
        // focus returns is a new press (Alt is no longer held, so Enter is
        // no chord).
        up(&mut input, "ArrowRight");
        down(&mut input, "ArrowRight");
        down(&mut input, "Enter");
        let frame = input.take_frame();
        assert!(frame.slots[0].right && frame.any_key);
        assert_eq!(frame.actions, []);
    }

    #[test]
    fn text_queue_letters_only_passthrough() {
        let mut input = state();
        typed(&mut input, "KeyM", "m");
        typed(&mut input, "KeyB", "B");
        typed(&mut input, "Digit1", "1");
        typed(&mut input, "Space", " ");
        typed(&mut input, "Backspace", "\u{8}");
        typed(&mut input, "Semicolon", "é");
        // An arrow key types nothing, whatever it is bound to.
        down(&mut input, "ArrowRight");
        // An unidentified key that types a letter.
        input.key_down(KeyPress {
            key: None,
            repeat: false,
            text: Some("q"),
        });
        let frame = input.take_frame();
        assert_eq!(frame.letters, "mBq");
        assert_eq!(
            frame.keys,
            [
                "KeyM",
                "KeyB",
                "Digit1",
                "Space",
                "Backspace",
                "Semicolon",
                "ArrowRight"
            ]
            .map(key)
        );
        // Repeated text and Alt-modified text are dropped.
        state_repeat_text(&mut input);
        assert_eq!(input.take_frame().letters, "");
    }

    fn state_repeat_text(input: &mut InputState) {
        input.key_down(KeyPress {
            key: Some(key("KeyM")),
            repeat: true,
            text: Some("m"),
        });
        down(input, "AltLeft");
        typed(input, "KeyX", "x");
    }
}
