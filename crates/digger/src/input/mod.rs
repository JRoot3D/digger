//! The input mapping layer: `Action`, `KeyName`, `PadButton` and `Bindings`
//! (design D7).
//!
//! This module names the 20 bindable actions, the physical keys (W3C UI
//! Events code names) and the gamepad buttons, with their default bindings
//! (`input-controls` spec). The names are what the configuration file and
//! the redefinition screen use; the platform layer translates its own key
//! and button codes into these types.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the names and defaults are used by the bindings, shell and platform code of later task groups"
    )
)]

use std::collections::HashMap;
use std::fmt;

use digger_core::Direction;

use crate::config::Settings;

pub(crate) mod gamepad;
#[cfg(test)]
mod resolution_tests;
pub(crate) mod state;

#[allow(
    unused_imports,
    reason = "the input layer's vocabulary for the shell and platform code of later task groups"
)]
pub(crate) use gamepad::{PadEvent, PadId, PadInput, PadSignal, Pads, StickAxis};
#[allow(
    unused_imports,
    reason = "the input layer's vocabulary for the shell and platform code of later task groups"
)]
pub(crate) use state::{InputFrame, InputState, KeyPress, PadChange, PadPress};

/// One of the 20 bindable keyboard actions (`input-controls` spec, action
/// table), in the order of the `[keys]` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Action {
    /// Player 1 moves right.
    P1Right,
    /// Player 1 moves up.
    P1Up,
    /// Player 1 moves left.
    P1Left,
    /// Player 1 moves down.
    P1Down,
    /// Player 1 fires.
    P1Fire,
    /// Player 2 moves right.
    P2Right,
    /// Player 2 moves up.
    P2Up,
    /// Player 2 moves left.
    P2Left,
    /// Player 2 moves down.
    P2Down,
    /// Player 2 fires.
    P2Fire,
    /// Shorter frame time.
    SpeedUp,
    /// Longer frame time.
    SpeedDown,
    /// Music on or off.
    ToggleMusic,
    /// All sound on or off.
    ToggleSound,
    /// Leave the game, or the program on the title screen.
    Quit,
    /// Pause the game.
    Pause,
    /// Next game mode on the title screen.
    ChangeMode,
    /// Save the last game's recording.
    SaveRecording,
    /// Open key redefinition.
    RedefineKeys,
    /// Window or fullscreen.
    Fullscreen,
}

impl Action {
    /// Number of actions.
    pub(crate) const COUNT: usize = 20;

    /// Every action, in table order.
    pub(crate) const ALL: [Self; Self::COUNT] = [
        Self::P1Right,
        Self::P1Up,
        Self::P1Left,
        Self::P1Down,
        Self::P1Fire,
        Self::P2Right,
        Self::P2Up,
        Self::P2Left,
        Self::P2Down,
        Self::P2Fire,
        Self::SpeedUp,
        Self::SpeedDown,
        Self::ToggleMusic,
        Self::ToggleSound,
        Self::Quit,
        Self::Pause,
        Self::ChangeMode,
        Self::SaveRecording,
        Self::RedefineKeys,
        Self::Fullscreen,
    ];

    /// The position of this action in [`Action::ALL`].
    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    /// The action's name in the configuration file.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::P1Right => "p1_right",
            Self::P1Up => "p1_up",
            Self::P1Left => "p1_left",
            Self::P1Down => "p1_down",
            Self::P1Fire => "p1_fire",
            Self::P2Right => "p2_right",
            Self::P2Up => "p2_up",
            Self::P2Left => "p2_left",
            Self::P2Down => "p2_down",
            Self::P2Fire => "p2_fire",
            Self::SpeedUp => "speed_up",
            Self::SpeedDown => "speed_down",
            Self::ToggleMusic => "toggle_music",
            Self::ToggleSound => "toggle_sound",
            Self::Quit => "quit",
            Self::Pause => "pause",
            Self::ChangeMode => "change_mode",
            Self::SaveRecording => "save_recording",
            Self::RedefineKeys => "redefine_keys",
            Self::Fullscreen => "fullscreen",
        }
    }

    /// The action with this configuration-file name.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.name() == name)
    }

    /// The player control this action stands for, or `None` for a
    /// function action.
    pub(crate) const fn control(self) -> Option<Control> {
        let (slot, kind) = match self {
            Self::P1Right => (0, ControlKind::Right),
            Self::P1Up => (0, ControlKind::Up),
            Self::P1Left => (0, ControlKind::Left),
            Self::P1Down => (0, ControlKind::Down),
            Self::P1Fire => (0, ControlKind::Fire),
            Self::P2Right => (1, ControlKind::Right),
            Self::P2Up => (1, ControlKind::Up),
            Self::P2Left => (1, ControlKind::Left),
            Self::P2Down => (1, ControlKind::Down),
            Self::P2Fire => (1, ControlKind::Fire),
            _ => return None,
        };
        Some(Control { slot, kind })
    }

    /// Whether a key bound to this action is left out of "any key" on the
    /// title screen: mode change, key redefinition, quit, save recording,
    /// the sound and music toggles, the speed keys and fullscreen
    /// (`desktop-app` spec, starting a game).
    pub(crate) const fn excluded_from_start(self) -> bool {
        matches!(
            self,
            Self::ChangeMode
                | Self::RedefineKeys
                | Self::Quit
                | Self::SaveRecording
                | Self::ToggleSound
                | Self::ToggleMusic
                | Self::SpeedUp
                | Self::SpeedDown
                | Self::Fullscreen
        )
    }

    /// The default keys of this action (`input-controls` spec, default
    /// keys for player 1, player 2 and the function keys).
    pub(crate) const fn default_key_names(self) -> &'static [&'static str] {
        match self {
            Self::P1Right => &["ArrowRight"],
            Self::P1Up => &["ArrowUp"],
            Self::P1Left => &["ArrowLeft"],
            Self::P1Down => &["ArrowDown"],
            Self::P1Fire => &["F1"],
            Self::P2Right => &["KeyS"],
            Self::P2Up => &["KeyW"],
            Self::P2Left => &["KeyA"],
            Self::P2Down => &["KeyZ"],
            Self::P2Fire => &["Tab"],
            Self::SpeedUp => &["NumpadAdd", "Equal"],
            Self::SpeedDown => &["NumpadSubtract", "Minus"],
            Self::ToggleMusic => &["F7"],
            Self::ToggleSound => &["F9"],
            Self::Quit => &["F10"],
            Self::Pause => &["Space"],
            Self::ChangeMode => &["KeyN"],
            Self::SaveRecording => &["F8"],
            Self::RedefineKeys => &["KeyK"],
            Self::Fullscreen => &["F11"],
        }
    }

    /// The default keys of this action as key names.
    pub(crate) fn default_keys(self) -> Vec<KeyName> {
        self.default_key_names()
            .iter()
            .filter_map(|name| KeyName::parse(name))
            .collect()
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The physical keys, by their W3C UI Events `code` names (`input-controls`
/// spec, key names). The order is the order of the specification's tables.
const KEY_NAMES: &[&str] = &[
    // Alphanumeric section, writing-system keys.
    "Backquote",
    "Backslash",
    "BracketLeft",
    "BracketRight",
    "Comma",
    "Digit0",
    "Digit1",
    "Digit2",
    "Digit3",
    "Digit4",
    "Digit5",
    "Digit6",
    "Digit7",
    "Digit8",
    "Digit9",
    "Equal",
    "IntlBackslash",
    "IntlRo",
    "IntlYen",
    "KeyA",
    "KeyB",
    "KeyC",
    "KeyD",
    "KeyE",
    "KeyF",
    "KeyG",
    "KeyH",
    "KeyI",
    "KeyJ",
    "KeyK",
    "KeyL",
    "KeyM",
    "KeyN",
    "KeyO",
    "KeyP",
    "KeyQ",
    "KeyR",
    "KeyS",
    "KeyT",
    "KeyU",
    "KeyV",
    "KeyW",
    "KeyX",
    "KeyY",
    "KeyZ",
    "Minus",
    "Period",
    "Quote",
    "Semicolon",
    "Slash",
    // Alphanumeric section, functional keys.
    "AltLeft",
    "AltRight",
    "Backspace",
    "CapsLock",
    "ContextMenu",
    "ControlLeft",
    "ControlRight",
    "Enter",
    "MetaLeft",
    "MetaRight",
    "ShiftLeft",
    "ShiftRight",
    "Space",
    "Tab",
    "Convert",
    "KanaMode",
    "Lang1",
    "Lang2",
    "NonConvert",
    // Control pad section.
    "Delete",
    "End",
    "Help",
    "Home",
    "Insert",
    "PageDown",
    "PageUp",
    // Arrow pad section.
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "ArrowUp",
    // Numpad section.
    "NumLock",
    "Numpad0",
    "Numpad1",
    "Numpad2",
    "Numpad3",
    "Numpad4",
    "Numpad5",
    "Numpad6",
    "Numpad7",
    "Numpad8",
    "Numpad9",
    "NumpadAdd",
    "NumpadComma",
    "NumpadDecimal",
    "NumpadDivide",
    "NumpadEnter",
    "NumpadEqual",
    "NumpadMultiply",
    "NumpadSubtract",
    // Function section.
    "Escape",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
    "F13",
    "F14",
    "F15",
    "F16",
    "F17",
    "F18",
    "F19",
    "F20",
    "F21",
    "F22",
    "F23",
    "F24",
    "PrintScreen",
    "ScrollLock",
    "Pause",
];

/// A physical key, by its W3C UI Events code name (`KeyA`, `ArrowLeft`,
/// `NumpadAdd`). Only valid names can be represented.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct KeyName(u8);

impl KeyName {
    /// Every key name, in table order.
    pub(crate) fn all() -> impl Iterator<Item = Self> {
        (0..KEY_NAMES.len()).filter_map(|index| u8::try_from(index).ok().map(Self))
    }

    /// The key with this code name. Names are matched exactly first, then
    /// ignoring letter case (`space` is `Space`); anything else is not a
    /// key.
    pub(crate) fn parse(name: &str) -> Option<Self> {
        let index = KEY_NAMES
            .iter()
            .position(|known| *known == name)
            .or_else(|| {
                KEY_NAMES
                    .iter()
                    .position(|known| known.eq_ignore_ascii_case(name))
            })?;
        u8::try_from(index).ok().map(Self)
    }

    /// The key's code name.
    pub(crate) fn name(self) -> &'static str {
        KEY_NAMES.get(usize::from(self.0)).copied().unwrap_or("")
    }

    /// Whether this is a modifier key (Shift, Control, Alt, Meta), which
    /// never starts a game on its own.
    pub(crate) fn is_modifier(self) -> bool {
        matches!(
            self.name(),
            "ShiftLeft"
                | "ShiftRight"
                | "ControlLeft"
                | "ControlRight"
                | "AltLeft"
                | "AltRight"
                | "MetaLeft"
                | "MetaRight"
        )
    }

    /// Whether this is one of the Alt keys, which cannot be bound by
    /// redefinition.
    pub(crate) fn is_alt(self) -> bool {
        matches!(self.name(), "AltLeft" | "AltRight")
    }
}

impl fmt::Debug for KeyName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "KeyName({})", self.name())
    }
}

impl fmt::Display for KeyName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A bindable gamepad button, named after its position on a standard pad
/// (`South` is the bottom face button). The directional pad is not
/// bindable: it always moves the pad's Digger (design, non-goals).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum PadButton {
    /// Bottom face button.
    South,
    /// Right face button.
    East,
    /// Top face button.
    North,
    /// Left face button.
    West,
    /// Extra face button C.
    C,
    /// Extra face button Z.
    Z,
    /// Left shoulder button.
    LeftTrigger,
    /// Left trigger.
    LeftTrigger2,
    /// Right shoulder button.
    RightTrigger,
    /// Right trigger.
    RightTrigger2,
    /// Select or Back.
    Select,
    /// Start.
    Start,
    /// The vendor button in the middle.
    Mode,
    /// Pressing the left stick.
    LeftThumb,
    /// Pressing the right stick.
    RightThumb,
}

impl PadButton {
    /// Every bindable button.
    pub(crate) const ALL: [Self; 15] = [
        Self::South,
        Self::East,
        Self::North,
        Self::West,
        Self::C,
        Self::Z,
        Self::LeftTrigger,
        Self::LeftTrigger2,
        Self::RightTrigger,
        Self::RightTrigger2,
        Self::Select,
        Self::Start,
        Self::Mode,
        Self::LeftThumb,
        Self::RightThumb,
    ];

    /// The button's name in the configuration file.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::South => "South",
            Self::East => "East",
            Self::North => "North",
            Self::West => "West",
            Self::C => "C",
            Self::Z => "Z",
            Self::LeftTrigger => "LeftTrigger",
            Self::LeftTrigger2 => "LeftTrigger2",
            Self::RightTrigger => "RightTrigger",
            Self::RightTrigger2 => "RightTrigger2",
            Self::Select => "Select",
            Self::Start => "Start",
            Self::Mode => "Mode",
            Self::LeftThumb => "LeftThumb",
            Self::RightThumb => "RightThumb",
        }
    }

    /// The button with this name, matched exactly first, then ignoring
    /// letter case.
    pub(crate) fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|button| button.name() == name)
            .or_else(|| {
                Self::ALL
                    .into_iter()
                    .find(|button| button.name().eq_ignore_ascii_case(name))
            })
    }
}

impl fmt::Display for PadButton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What a gamepad button can be bound to in the `[gamepad]` table. Fire
/// means the fire control of the pad's own player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum PadAction {
    /// The pad's player fires.
    Fire,
    /// Pause the game.
    Pause,
    /// Next game mode on the title screen.
    ChangeMode,
    /// Leave the game, or the program on the title screen.
    Quit,
}

impl PadAction {
    /// Number of pad actions.
    pub(crate) const COUNT: usize = 4;

    /// Every pad action, in table order.
    pub(crate) const ALL: [Self; Self::COUNT] =
        [Self::Fire, Self::Pause, Self::ChangeMode, Self::Quit];

    /// The position of this action in [`PadAction::ALL`].
    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    /// The action's name in the `[gamepad]` table.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Fire => "fire",
            Self::Pause => "pause",
            Self::ChangeMode => "change_mode",
            Self::Quit => "quit",
        }
    }

    /// The default buttons (`input-controls` spec, gamepad buttons).
    pub(crate) const fn default_button_names(self) -> &'static [&'static str] {
        match self {
            Self::Fire => &["South", "West"],
            Self::Pause => &["Start"],
            Self::ChangeMode => &["Select"],
            Self::Quit => &[],
        }
    }

    /// The default buttons as button values.
    pub(crate) fn default_buttons(self) -> Vec<PadButton> {
        self.default_button_names()
            .iter()
            .filter_map(|name| PadButton::parse(name))
            .collect()
    }
}

impl fmt::Display for PadAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl PadAction {
    /// The keyboard action a press of a button bound to this pad action
    /// triggers, or `None` for fire, which is a player control.
    pub(crate) const fn action(self) -> Option<Action> {
        match self {
            Self::Fire => None,
            Self::Pause => Some(Action::Pause),
            Self::ChangeMode => Some(Action::ChangeMode),
            Self::Quit => Some(Action::Quit),
        }
    }
}

/// Which of a player's five controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum ControlKind {
    /// Move right.
    Right,
    /// Move up.
    Up,
    /// Move left.
    Left,
    /// Move down.
    Down,
    /// Fire.
    Fire,
}

impl ControlKind {
    /// The control that moves in `direction`.
    pub(crate) const fn of_direction(direction: Direction) -> Self {
        match direction {
            Direction::Right => Self::Right,
            Direction::Up => Self::Up,
            Direction::Left => Self::Left,
            Direction::Down => Self::Down,
        }
    }
}

/// One of the ten player controls: a control slot (0 drives the first
/// Digger and both alternating players, 1 the second Digger) and which of
/// its five controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Control {
    /// The control slot, 0 or 1.
    pub(crate) slot: u8,
    /// Which control of the slot.
    pub(crate) kind: ControlKind,
}

impl Control {
    /// Number of player controls.
    pub(crate) const COUNT: usize = 10;

    /// The position of this control among the ten (slot-major).
    pub(crate) const fn index(self) -> usize {
        self.slot as usize * 5 + self.kind as usize
    }
}

/// The key and pad-button bindings in effect, built from the resolved
/// settings (design D7). A key or button belongs to at most one action;
/// the configuration layer has already refused duplicates, and should one
/// slip through, the first action in table order keeps the key.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Bindings {
    /// Key → action.
    keys: HashMap<KeyName, Action>,
    /// Pad button → pad action.
    buttons: HashMap<PadButton, PadAction>,
}

impl Bindings {
    /// The bindings of `settings`' `[keys]` and `[gamepad]` tables.
    pub(crate) fn from_settings(settings: &Settings) -> Self {
        let mut keys = HashMap::new();
        for action in Action::ALL {
            for &key in settings.keys_of(action) {
                keys.entry(key).or_insert(action);
            }
        }
        let mut buttons = HashMap::new();
        for action in PadAction::ALL {
            for &button in settings.buttons_of(action) {
                buttons.entry(button).or_insert(action);
            }
        }
        Self { keys, buttons }
    }

    /// The action `key` is bound to.
    pub(crate) fn action_of(&self, key: KeyName) -> Option<Action> {
        self.keys.get(&key).copied()
    }

    /// The pad action `button` is bound to.
    pub(crate) fn pad_action_of(&self, button: PadButton) -> Option<PadAction> {
        self.buttons.get(&button).copied()
    }
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use std::collections::HashMap;

    use super::{Action, Bindings, Control, ControlKind, KEY_NAMES, KeyName, PadAction, PadButton};
    use crate::config::Settings;

    #[test]
    fn key_names_round_trip() {
        assert_eq!(KeyName::all().count(), KEY_NAMES.len());
        for key in KeyName::all() {
            assert_eq!(KeyName::parse(key.name()), Some(key));
            assert_eq!(key.to_string(), key.name());
        }
        // Spec examples.
        for name in [
            "ArrowLeft",
            "KeyA",
            "Digit1",
            "F1",
            "Space",
            "Tab",
            "NumpadAdd",
            "ControlRight",
        ] {
            let key = KeyName::parse(name).map(KeyName::name);
            assert_eq!(key, Some(name));
        }
        // Letter case is forgiven, the printed form is canonical.
        assert_eq!(KeyName::parse("space").map(KeyName::name), Some("Space"));
        assert_eq!(KeyName::parse("keya").map(KeyName::name), Some("KeyA"));
        // Not code names.
        for bad in ["", "A", "Ctrl", "Left", "Key", "F25", "Space "] {
            assert_eq!(KeyName::parse(bad), None, "{bad:?}");
        }
        // Names are unique, ignoring case too.
        let mut seen = std::collections::HashSet::new();
        for name in KEY_NAMES {
            assert!(seen.insert(name.to_ascii_lowercase()), "{name} twice");
        }
        // Pad buttons round-trip as well.
        for button in PadButton::ALL {
            assert_eq!(PadButton::parse(button.name()), Some(button));
        }
        assert_eq!(PadButton::parse("start"), Some(PadButton::Start));
        assert_eq!(PadButton::parse("DPadUp"), None);
        assert_eq!(PadButton::parse("A"), None);
    }

    #[test]
    fn default_bindings_unique() {
        assert_eq!(Action::ALL.len(), 20);
        let mut owner: HashMap<KeyName, Action> = HashMap::new();
        for (index, action) in Action::ALL.into_iter().enumerate() {
            assert_eq!(action.index(), index);
            assert_eq!(Action::from_name(action.name()), Some(action));
            let keys = action.default_keys();
            assert!(!keys.is_empty(), "{action} has no default key");
            assert_eq!(
                keys.len(),
                action.default_key_names().len(),
                "{action} has an unknown default key name"
            );
            for key in keys {
                if let Some(other) = owner.insert(key, action) {
                    panic!("{key} is bound to both {other} and {action}");
                }
            }
        }
        let names: Vec<&str> = Action::ALL.iter().map(|action| action.name()).collect();
        assert_eq!(
            names,
            [
                "p1_right",
                "p1_up",
                "p1_left",
                "p1_down",
                "p1_fire",
                "p2_right",
                "p2_up",
                "p2_left",
                "p2_down",
                "p2_fire",
                "speed_up",
                "speed_down",
                "toggle_music",
                "toggle_sound",
                "quit",
                "pause",
                "change_mode",
                "save_recording",
                "redefine_keys",
                "fullscreen",
            ]
        );
        // Spec defaults, spot checks.
        let keys = |action: Action| -> Vec<&str> {
            action
                .default_keys()
                .into_iter()
                .map(KeyName::name)
                .collect()
        };
        assert_eq!(keys(Action::P1Fire), ["F1"]);
        assert_eq!(keys(Action::P2Down), ["KeyZ"]);
        assert_eq!(keys(Action::SpeedUp), ["NumpadAdd", "Equal"]);
        assert_eq!(keys(Action::SpeedDown), ["NumpadSubtract", "Minus"]);
        assert_eq!(keys(Action::Fullscreen), ["F11"]);
        // Pad defaults: known names, no button twice.
        let mut pad_owner: HashMap<PadButton, PadAction> = HashMap::new();
        for action in PadAction::ALL {
            let buttons = action.default_buttons();
            assert_eq!(buttons.len(), action.default_button_names().len());
            for button in buttons {
                assert!(pad_owner.insert(button, action).is_none());
            }
        }
        assert_eq!(
            PadAction::Fire.default_buttons(),
            [PadButton::South, PadButton::West]
        );
        assert_eq!(PadAction::Pause.default_buttons(), [PadButton::Start]);
        assert_eq!(PadAction::ChangeMode.default_buttons(), [PadButton::Select]);
        assert_eq!(PadAction::Quit.default_buttons(), [] as [PadButton; 0]);
    }

    #[test]
    fn modifier_keys() {
        let key = |name| KeyName::parse(name).map(|key| (key.is_modifier(), key.is_alt()));
        assert_eq!(key("ShiftLeft"), Some((true, false)));
        assert_eq!(key("AltRight"), Some((true, true)));
        assert_eq!(key("MetaLeft"), Some((true, false)));
        assert_eq!(key("Space"), Some((false, false)));
    }

    #[test]
    fn bindings_lookup() {
        let key = |name| KeyName::parse(name).unwrap_or_else(|| panic!("{name} is a key"));
        let bindings = Bindings::from_settings(&Settings::default());
        // Every default key finds its action.
        for action in Action::ALL {
            for key in action.default_keys() {
                assert_eq!(bindings.action_of(key), Some(action), "{key}");
            }
        }
        assert_eq!(bindings.action_of(key("ArrowLeft")), Some(Action::P1Left));
        assert_eq!(bindings.action_of(key("Equal")), Some(Action::SpeedUp));
        assert_eq!(bindings.action_of(key("Tab")), Some(Action::P2Fire));
        assert_eq!(bindings.action_of(key("KeyQ")), None);
        assert_eq!(bindings.action_of(key("Enter")), None);
        // Pad buttons.
        assert_eq!(
            bindings.pad_action_of(PadButton::South),
            Some(PadAction::Fire)
        );
        assert_eq!(
            bindings.pad_action_of(PadButton::West),
            Some(PadAction::Fire)
        );
        assert_eq!(
            bindings.pad_action_of(PadButton::Start),
            Some(PadAction::Pause)
        );
        assert_eq!(
            bindings.pad_action_of(PadButton::Select),
            Some(PadAction::ChangeMode)
        );
        assert_eq!(bindings.pad_action_of(PadButton::East), None);
        // Bindings follow the settings: a redefined key moves, the old one
        // is free.
        let mut settings = Settings::default();
        settings.keys[Action::P1Fire.index()] = vec![key("ControlLeft")];
        settings.pad_buttons[PadAction::Quit.index()] = vec![PadButton::Mode];
        let bindings = Bindings::from_settings(&settings);
        assert_eq!(bindings.action_of(key("ControlLeft")), Some(Action::P1Fire));
        assert_eq!(bindings.action_of(key("F1")), None);
        assert_eq!(
            bindings.pad_action_of(PadButton::Mode),
            Some(PadAction::Quit)
        );
        assert_eq!(PadAction::Quit.action(), Some(Action::Quit));
        assert_eq!(PadAction::Fire.action(), None);
        // Player controls and their indices.
        let mut seen = [false; Control::COUNT];
        for action in &Action::ALL[..10] {
            let control = action.control().unwrap_or_else(|| panic!("{action}"));
            assert!(!seen[control.index()]);
            seen[control.index()] = true;
        }
        assert!(Action::ALL[10..].iter().all(|a| a.control().is_none()));
        assert_eq!(
            Action::P2Down.control(),
            Some(Control {
                slot: 1,
                kind: ControlKind::Down
            })
        );
        // The start exclusions of the desktop-app spec.
        let excluded: Vec<Action> = Action::ALL
            .into_iter()
            .filter(|a| a.excluded_from_start())
            .collect();
        assert_eq!(excluded.len(), 9);
        assert!(!Action::Pause.excluded_from_start());
        assert!(!Action::P1Fire.excluded_from_start());
    }
}
