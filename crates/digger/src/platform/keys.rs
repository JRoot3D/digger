//! Physical keys of winit translated to the port's key names (design D4,
//! D7; `input-controls` spec, key names).
//!
//! winit names its key codes after the W3C UI Events `code` values the
//! port uses, except for the Meta keys, which winit calls `Super`. Keys the
//! port has no name for (media keys, `F25` and above) are not identified;
//! they still count as nothing at all, not as "any key".

use winit::keyboard::{KeyCode, PhysicalKey};

use crate::input::KeyName;

/// Every winit key code that has a port key name, with that name, in the
/// order of the port's key table.
const TABLE: &[(KeyCode, &str)] = &[
    (KeyCode::Backquote, "Backquote"),
    (KeyCode::Backslash, "Backslash"),
    (KeyCode::BracketLeft, "BracketLeft"),
    (KeyCode::BracketRight, "BracketRight"),
    (KeyCode::Comma, "Comma"),
    (KeyCode::Digit0, "Digit0"),
    (KeyCode::Digit1, "Digit1"),
    (KeyCode::Digit2, "Digit2"),
    (KeyCode::Digit3, "Digit3"),
    (KeyCode::Digit4, "Digit4"),
    (KeyCode::Digit5, "Digit5"),
    (KeyCode::Digit6, "Digit6"),
    (KeyCode::Digit7, "Digit7"),
    (KeyCode::Digit8, "Digit8"),
    (KeyCode::Digit9, "Digit9"),
    (KeyCode::Equal, "Equal"),
    (KeyCode::IntlBackslash, "IntlBackslash"),
    (KeyCode::IntlRo, "IntlRo"),
    (KeyCode::IntlYen, "IntlYen"),
    (KeyCode::KeyA, "KeyA"),
    (KeyCode::KeyB, "KeyB"),
    (KeyCode::KeyC, "KeyC"),
    (KeyCode::KeyD, "KeyD"),
    (KeyCode::KeyE, "KeyE"),
    (KeyCode::KeyF, "KeyF"),
    (KeyCode::KeyG, "KeyG"),
    (KeyCode::KeyH, "KeyH"),
    (KeyCode::KeyI, "KeyI"),
    (KeyCode::KeyJ, "KeyJ"),
    (KeyCode::KeyK, "KeyK"),
    (KeyCode::KeyL, "KeyL"),
    (KeyCode::KeyM, "KeyM"),
    (KeyCode::KeyN, "KeyN"),
    (KeyCode::KeyO, "KeyO"),
    (KeyCode::KeyP, "KeyP"),
    (KeyCode::KeyQ, "KeyQ"),
    (KeyCode::KeyR, "KeyR"),
    (KeyCode::KeyS, "KeyS"),
    (KeyCode::KeyT, "KeyT"),
    (KeyCode::KeyU, "KeyU"),
    (KeyCode::KeyV, "KeyV"),
    (KeyCode::KeyW, "KeyW"),
    (KeyCode::KeyX, "KeyX"),
    (KeyCode::KeyY, "KeyY"),
    (KeyCode::KeyZ, "KeyZ"),
    (KeyCode::Minus, "Minus"),
    (KeyCode::Period, "Period"),
    (KeyCode::Quote, "Quote"),
    (KeyCode::Semicolon, "Semicolon"),
    (KeyCode::Slash, "Slash"),
    (KeyCode::AltLeft, "AltLeft"),
    (KeyCode::AltRight, "AltRight"),
    (KeyCode::Backspace, "Backspace"),
    (KeyCode::CapsLock, "CapsLock"),
    (KeyCode::ContextMenu, "ContextMenu"),
    (KeyCode::ControlLeft, "ControlLeft"),
    (KeyCode::ControlRight, "ControlRight"),
    (KeyCode::Enter, "Enter"),
    (KeyCode::SuperLeft, "MetaLeft"),
    (KeyCode::SuperRight, "MetaRight"),
    (KeyCode::ShiftLeft, "ShiftLeft"),
    (KeyCode::ShiftRight, "ShiftRight"),
    (KeyCode::Space, "Space"),
    (KeyCode::Tab, "Tab"),
    (KeyCode::Convert, "Convert"),
    (KeyCode::KanaMode, "KanaMode"),
    (KeyCode::Lang1, "Lang1"),
    (KeyCode::Lang2, "Lang2"),
    (KeyCode::NonConvert, "NonConvert"),
    (KeyCode::Delete, "Delete"),
    (KeyCode::End, "End"),
    (KeyCode::Help, "Help"),
    (KeyCode::Home, "Home"),
    (KeyCode::Insert, "Insert"),
    (KeyCode::PageDown, "PageDown"),
    (KeyCode::PageUp, "PageUp"),
    (KeyCode::ArrowDown, "ArrowDown"),
    (KeyCode::ArrowLeft, "ArrowLeft"),
    (KeyCode::ArrowRight, "ArrowRight"),
    (KeyCode::ArrowUp, "ArrowUp"),
    (KeyCode::NumLock, "NumLock"),
    (KeyCode::Numpad0, "Numpad0"),
    (KeyCode::Numpad1, "Numpad1"),
    (KeyCode::Numpad2, "Numpad2"),
    (KeyCode::Numpad3, "Numpad3"),
    (KeyCode::Numpad4, "Numpad4"),
    (KeyCode::Numpad5, "Numpad5"),
    (KeyCode::Numpad6, "Numpad6"),
    (KeyCode::Numpad7, "Numpad7"),
    (KeyCode::Numpad8, "Numpad8"),
    (KeyCode::Numpad9, "Numpad9"),
    (KeyCode::NumpadAdd, "NumpadAdd"),
    (KeyCode::NumpadComma, "NumpadComma"),
    (KeyCode::NumpadDecimal, "NumpadDecimal"),
    (KeyCode::NumpadDivide, "NumpadDivide"),
    (KeyCode::NumpadEnter, "NumpadEnter"),
    (KeyCode::NumpadEqual, "NumpadEqual"),
    (KeyCode::NumpadMultiply, "NumpadMultiply"),
    (KeyCode::NumpadSubtract, "NumpadSubtract"),
    (KeyCode::Escape, "Escape"),
    (KeyCode::F1, "F1"),
    (KeyCode::F2, "F2"),
    (KeyCode::F3, "F3"),
    (KeyCode::F4, "F4"),
    (KeyCode::F5, "F5"),
    (KeyCode::F6, "F6"),
    (KeyCode::F7, "F7"),
    (KeyCode::F8, "F8"),
    (KeyCode::F9, "F9"),
    (KeyCode::F10, "F10"),
    (KeyCode::F11, "F11"),
    (KeyCode::F12, "F12"),
    (KeyCode::F13, "F13"),
    (KeyCode::F14, "F14"),
    (KeyCode::F15, "F15"),
    (KeyCode::F16, "F16"),
    (KeyCode::F17, "F17"),
    (KeyCode::F18, "F18"),
    (KeyCode::F19, "F19"),
    (KeyCode::F20, "F20"),
    (KeyCode::F21, "F21"),
    (KeyCode::F22, "F22"),
    (KeyCode::F23, "F23"),
    (KeyCode::F24, "F24"),
    (KeyCode::PrintScreen, "PrintScreen"),
    (KeyCode::ScrollLock, "ScrollLock"),
    (KeyCode::Pause, "Pause"),
];

/// The port's name for the physical key `key`, if it has one.
pub(super) fn key_name(key: PhysicalKey) -> Option<KeyName> {
    let PhysicalKey::Code(code) = key else {
        return None;
    };
    TABLE
        .iter()
        .find(|(known, _)| *known == code)
        .and_then(|(_, name)| KeyName::parse(name))
}

#[cfg(test)]
mod tests {
    use winit::keyboard::{KeyCode, NativeKeyCode, PhysicalKey};

    use super::{TABLE, key_name};
    use crate::input::KeyName;

    #[test]
    fn winit_keycode_table_covers_all_key_names() {
        // Every name of the table is a key name, spelled exactly.
        for (code, name) in TABLE {
            let key = key_name(PhysicalKey::Code(*code));
            assert_eq!(key.map(KeyName::name), Some(*name), "{code:?}");
        }
        // Every key name is reached by exactly one key code.
        for key in KeyName::all() {
            let codes = TABLE.iter().filter(|(_, name)| *name == key.name()).count();
            assert_eq!(codes, 1, "{key}");
        }
        assert_eq!(TABLE.len(), KeyName::all().count());
    }

    #[test]
    fn meta_keys_are_winit_super_keys() {
        let left = key_name(PhysicalKey::Code(KeyCode::SuperLeft));
        assert_eq!(left.map(KeyName::name), Some("MetaLeft"));
        let right = key_name(PhysicalKey::Code(KeyCode::SuperRight));
        assert_eq!(right.map(KeyName::name), Some("MetaRight"));
    }

    #[test]
    fn keys_without_a_name_are_not_identified() {
        assert_eq!(key_name(PhysicalKey::Code(KeyCode::F25)), None);
        assert_eq!(key_name(PhysicalKey::Code(KeyCode::MediaPlayPause)), None);
        assert_eq!(
            key_name(PhysicalKey::Unidentified(NativeKeyCode::Unidentified)),
            None
        );
    }
}
