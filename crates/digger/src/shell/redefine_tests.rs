//! Shell tests of task group 10: key redefinition, its prompts and layout,
//! conflicts, the "all" variant, aborting and saving.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;

use digger_core::geom::Px;
use digger_core::video::{Framebuffer, TextColor};

use super::harness::{Harness, key};
use super::{Request, Setup};
use crate::cli::Redefine;
use crate::config::file::tests::TempDir;
use crate::config::{GameMode, Layer, Settings};
use crate::input::{Action, Bindings, KeyPress};
use crate::store::{NativeStore, Storage};

/// The golden CRC of the redefinition screen as it opens in one-player
/// mode: the heading and player 1's right key asked for.
const REDEFINE_SCREEN_CRC: u32 = 0xBD42_3DEB;

/// Whether `text` is shown at `at` in `color`: drawing it there again
/// changes nothing.
fn shows(h: &Harness, text: &str, at: (i32, i32), color: u8) -> bool {
    let picture = h.shell.picture();
    let mut copy = picture.clone();
    copy.draw_text(text, Px::new(at.0, at.1), TextColor::new(color).unwrap());
    copy == *picture
}

/// A harness on the title screen in `mode`.
fn in_mode(mode: GameMode) -> Harness {
    Harness::with_settings(Settings {
        mode,
        ..Settings::default()
    })
}

/// A harness that opens on key redefinition (`--redefine-keys[=all]`)
/// in `mode`.
fn opening(mode: GameMode, redefine: Redefine) -> Harness {
    Harness::with(
        Settings {
            mode,
            ..Settings::default()
        },
        Setup {
            redefine_keys: Some(redefine),
            ..Setup::default()
        },
    )
}

/// Opens redefinition from the title screen with K.
fn open(h: &mut Harness) {
    h.frame();
    assert!(h.on_title());
    h.tap("KeyK");
    h.frame();
    assert!(h.redefining().is_some(), "K opens key redefinition");
}

/// The action being asked for.
fn asking(h: &Harness) -> Option<Action> {
    h.redefining().and_then(super::RedefineScene::asking)
}

/// Taps the key `name` and runs a frame.
fn give(h: &mut Harness, name: &str) {
    h.tap(name);
    h.frame();
}

#[test]
fn redefine_prompts_in_order() {
    // Two Diggers: player 1's five keys under `PLAYER 1:`, then player
    // 2's five under `PLAYER 2:`; each name turns from colour 2 to 1.
    let mut h = in_mode(GameMode::TwoDiggers);
    open(&mut h);
    assert!(shows(&h, "PRESS NEW KEY FOR", (0, 0), 3));
    assert!(shows(&h, "PLAYER 1:", (0, 12), 3));
    let names = ["RIGHT", "UP", "LEFT", "DOWN", "FIRE"];
    let p1 = ["KeyL", "KeyI", "KeyJ", "KeyM", "ControlRight"];
    for (i, (&name, &pressed)) in names.iter().zip(&p1).enumerate() {
        let y = 24 + 12 * i32::try_from(i).unwrap();
        assert_eq!(asking(&h), Some(Action::ALL[i]));
        assert!(shows(&h, name, (0, y), 2), "{name} asked in colour 2");
        give(&mut h, pressed);
        assert!(shows(&h, name, (0, y), 1), "{name} accepted in colour 1");
    }
    assert!(shows(&h, "PLAYER 2:", (0, 84), 3));
    let p2 = ["KeyD", "KeyR", "KeyQ", "KeyF", "ShiftLeft"];
    for (i, (&name, &pressed)) in names.iter().zip(&p2).enumerate() {
        let y = 96 + 12 * i32::try_from(i).unwrap();
        assert_eq!(asking(&h), Some(Action::ALL[5 + i]));
        assert!(shows(&h, name, (0, y), 2), "player 2's {name} asked");
        give(&mut h, pressed);
    }
    // No clash with a function key: the title screen returns, with the
    // new keys in effect.
    assert!(h.on_title());
    let bindings = h.input.bindings();
    for (i, name) in p1.iter().chain(&p2).enumerate() {
        assert_eq!(
            bindings.action_of(key(name)),
            Some(Action::ALL[i]),
            "{name}"
        );
    }
    assert_eq!(bindings.action_of(key("ArrowRight")), None);
    assert_eq!(bindings.action_of(key("KeyS")), None);

    // A long list: with the "all" variant in a two-Digger mode the
    // function actions follow `OTHER:` and continue in a second column,
    // next to player 1's keys.
    let mut h = opening(GameMode::TwoDiggers, Redefine::All);
    for name in p1.iter().chain(&p2) {
        give(&mut h, name);
    }
    assert!(shows(&h, "OTHER:", (0, 156), 3));
    assert!(shows(&h, "ACCEL", (0, 168), 2));
    give(&mut h, "NumpadAdd");
    assert!(shows(&h, "ACCEL", (0, 168), 1));
    assert!(shows(&h, "BRAKE", (0, 180), 2));
    give(&mut h, "NumpadSubtract");
    assert_eq!(asking(&h), Some(Action::ToggleMusic));
    assert!(shows(&h, "MUSIC", (156, 24), 2), "second column");
    for (i, (name, label)) in [
        ("F7", "MUSIC"),
        ("F9", "SOUND"),
        ("F10", "EXIT"),
        ("Space", "PAUSE"),
        ("KeyN", "MODE CHANGE"),
        ("F8", "SAVE DRF"),
        ("KeyK", "REDEFINE"),
        ("F11", "FULLSCREEN"),
    ]
    .into_iter()
    .enumerate()
    {
        let y = 24 + 12 * i32::try_from(i).unwrap();
        assert!(shows(&h, label, (156, y), 2), "{label} asked");
        give(&mut h, name);
        if i < 7 {
            assert!(shows(&h, label, (156, y), 1), "{label} accepted");
        }
    }
    assert!(h.on_title());
}

#[test]
fn redefine_screen_crc() {
    let mut h = in_mode(GameMode::OnePlayer);
    open(&mut h);
    let mut expected = Framebuffer::new();
    expected.draw_text("PRESS NEW KEY FOR", Px::new(0, 0), TextColor::Three);
    expected.draw_text("RIGHT", Px::new(0, 12), TextColor::Two);
    assert!(*h.shell.picture() == expected, "the screen's layout");
    assert_eq!(h.frames.last().unwrap().picture_crc, expected.crc32());
    assert_eq!(h.shell.picture().crc32(), REDEFINE_SCREEN_CRC, "golden CRC");
}

#[test]
fn redefine_duplicate_refused() {
    let mut h = in_mode(GameMode::OnePlayer);
    open(&mut h);
    give(&mut h, "KeyA");
    assert_eq!(asking(&h), Some(Action::P1Up));
    // The same key again is refused and up is asked for again.
    give(&mut h, "KeyA");
    assert_eq!(asking(&h), Some(Action::P1Up));
    assert!(shows(&h, "UP", (0, 24), 2), "still asked in colour 2");
    // Alt keys cannot be bound.
    give(&mut h, "AltLeft");
    give(&mut h, "AltRight");
    assert_eq!(asking(&h), Some(Action::P1Up));
    // Function keys do nothing while a key is asked for: F7 is taken as
    // up's key, and the music stays on.
    give(&mut h, "F7");
    assert_eq!(asking(&h), Some(Action::P1Left));
    assert!(h.shell.music_on());
    assert!(h.frames.iter().all(|frame| frame.controls.is_empty()));
    // Several keys in one frame are taken in order; a duplicate among
    // them is refused.
    h.tap("KeyB");
    h.tap("KeyB");
    h.tap("KeyC");
    h.frame();
    assert_eq!(asking(&h), Some(Action::P1Fire));
    // The Alt+Enter chord still switches fullscreen and binds nothing.
    h.press("AltLeft");
    h.tap("Enter");
    h.release("AltLeft");
    let requests = h.frame().requests.clone();
    assert_eq!(requests, [Request::ToggleFullscreen]);
    assert_eq!(asking(&h), Some(Action::P1Fire));
    give(&mut h, "ControlLeft");
    // F7 now moves up, so the music action lost its key and is asked
    // for after the player keys.
    assert_eq!(asking(&h), Some(Action::ToggleMusic));
    give(&mut h, "KeyA");
    assert_eq!(asking(&h), Some(Action::ToggleMusic), "KeyA is right's");
    give(&mut h, "F6");
    assert!(h.on_title());
    let bindings = h.input.bindings();
    assert_eq!(bindings.action_of(key("KeyA")), Some(Action::P1Right));
    assert_eq!(bindings.action_of(key("F7")), Some(Action::P1Up));
    assert_eq!(bindings.action_of(key("KeyB")), Some(Action::P1Left));
    assert_eq!(bindings.action_of(key("KeyC")), Some(Action::P1Down));
    assert_eq!(bindings.action_of(key("F6")), Some(Action::ToggleMusic));
    // KeyA was player 2's left: it takes player 1's old left key.
    assert_eq!(bindings.action_of(key("ArrowLeft")), Some(Action::P2Left));
    let mut settings = Settings::default();
    settings.apply(h.shell.runtime_changes());
    assert_eq!(settings.duplicate_key(), None, "a key for one action only");
}

#[test]
fn redefine_conflict_asks_pause() {
    let mut h = in_mode(GameMode::OnePlayer);
    open(&mut h);
    for name in ["ArrowRight", "ArrowUp", "ArrowLeft", "ArrowDown", "Space"] {
        give(&mut h, name);
    }
    // Space was the pause key: pause is asked for after the player keys.
    assert_eq!(asking(&h), Some(Action::Pause));
    assert!(shows(&h, "PAUSE", (0, 72), 2));
    // Space belongs to fire now and is refused.
    give(&mut h, "Space");
    assert_eq!(asking(&h), Some(Action::Pause));
    give(&mut h, "KeyP");
    assert!(h.on_title());
    let bindings = h.input.bindings();
    assert_eq!(bindings.action_of(key("Space")), Some(Action::P1Fire));
    assert_eq!(bindings.action_of(key("KeyP")), Some(Action::Pause));
    assert_eq!(bindings.action_of(key("F1")), None);
}

#[test]
fn redefine_all_asks_every_action() {
    let mut h = opening(GameMode::OnePlayer, Redefine::All);
    h.frame();
    for name in ["KeyD", "KeyE", "KeyJ", "KeyX", "ControlLeft"] {
        give(&mut h, name);
    }
    // No clash, but every function action is asked for, under `OTHER:`.
    assert!(shows(&h, "OTHER:", (0, 72), 3));
    let function_keys = [
        "NumpadAdd",
        "NumpadSubtract",
        "F7",
        "F9",
        "F10",
        "Space",
        "KeyN",
        "F8",
        "KeyK",
        "F11",
    ];
    for (action, name) in Action::ALL[10..].iter().zip(function_keys) {
        assert_eq!(asking(&h), Some(*action));
        give(&mut h, name);
    }
    assert!(h.on_title());
    // Each pressed key is now its action's only key: `Equal` no longer
    // speeds up.
    let bindings = h.input.bindings();
    assert_eq!(bindings.action_of(key("NumpadAdd")), Some(Action::SpeedUp));
    assert_eq!(bindings.action_of(key("Equal")), None);
    assert_eq!(bindings.action_of(key("Minus")), None);
    assert_eq!(bindings.action_of(key("F10")), Some(Action::Quit));
    // Player 2's keys are not asked for in one-player mode.
    assert_eq!(bindings.action_of(key("KeyS")), Some(Action::P2Right));
}

#[test]
fn redefine_abort_restores() {
    let dir = TempDir::new("redefine-abort");
    let path = dir.path().join("config.toml");
    let mut h = Harness::with_storage(
        Settings::default(),
        Setup { ..Setup::default() },
        config_storage(&path),
    );
    let before = h.input.bindings().clone();
    open(&mut h);
    give(&mut h, "KeyD");
    give(&mut h, "KeyE");
    // F10, the quit key, abandons the redefinition.
    give(&mut h, "F10");
    assert!(h.on_title(), "the title screen returns");
    assert_eq!(*h.input.bindings(), before);
    assert_eq!(h.shell.bindings(), before);
    assert_eq!(
        h.input.bindings().action_of(key("ArrowRight")),
        Some(Action::P1Right),
        "the arrow keys still move Digger"
    );
    assert_eq!(h.input.bindings().action_of(key("KeyD")), None);
    assert!(!path.exists(), "nothing is written");
    assert!(h.shell.runtime_changes().is_empty());
    assert!(
        !h.requests().contains(&Request::Exit(0)),
        "the program runs on"
    );

    // Closing the window during redefinition ends the program without
    // saving anything.
    open(&mut h);
    give(&mut h, "KeyD");
    h.close();
    assert_eq!(h.frame().requests, [Request::Exit(0)]);
    assert_eq!(*h.input.bindings(), before);
    assert!(!path.exists());
}

#[test]
fn redefine_persists_and_applies() {
    let dir = TempDir::new("redefine-save");
    let path = dir.path().join("config.toml");
    // `--redefine-keys` opens the program on key redefinition.
    let mut h = Harness::with_storage(
        Settings::default(),
        Setup {
            redefine_keys: Some(Redefine::Usual),
            ..Setup::default()
        },
        config_storage(&path),
    );
    h.frame();
    assert_eq!(asking(&h), Some(Action::P1Right));
    for name in ["ArrowRight", "ArrowUp", "ArrowLeft", "ArrowDown"] {
        give(&mut h, name);
    }
    assert!(!path.exists(), "nothing is written before completion");
    give(&mut h, "ControlLeft");
    assert!(h.on_title(), "the title screen follows");
    assert!(
        h.frames
            .last()
            .unwrap()
            .requests
            .contains(&Request::BindingsChanged)
    );
    // In effect at once.
    assert_eq!(
        h.input.bindings().action_of(key("ControlLeft")),
        Some(Action::P1Fire)
    );
    assert_eq!(h.input.bindings().action_of(key("F1")), None);
    // Written at once, as key names under the action names.
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("p1_fire = [\"ControlLeft\"]"), "{text}");
    assert!(text.contains("pause = [\"Space\"]"), "{text}");

    // The next run reads the new keys.
    let file = crate::config::file::read(&path).unwrap().unwrap();
    assert_eq!(file.warnings, [] as [String; 0]);
    let settings = Settings::resolve(&file.layer, &Layer::default());
    assert_eq!(settings.duplicate_key(), None);
    let bindings = Bindings::from_settings(&settings);
    assert_eq!(bindings.action_of(key("ControlLeft")), Some(Action::P1Fire));
    assert_eq!(bindings.action_of(key("F1")), None);
    let mut next = Harness::with_settings(settings);
    next.frame();
    assert!(next.on_title(), "a normal start opens on the title screen");
    next.input.key_down(KeyPress {
        key: Some(key("ControlLeft")),
        repeat: false,
        text: None,
    });
    assert!(next.input.take_frame().slots[0].fire, "left Control fires");
    next.release("ControlLeft");
    next.input.take_frame();
    next.press("F1");
    assert!(!next.input.take_frame().slots[0].fire, "F1 does not");
}

#[test]
fn redefine_player1_keys_scenario() {
    // `input-controls` spec, redefining player 1's keys: KeyA was player
    // 2's left, which takes ArrowLeft without being asked for.
    let mut h = in_mode(GameMode::OnePlayer);
    open(&mut h);
    for name in ["KeyD", "KeyE", "KeyA", "KeyX", "ControlLeft"] {
        give(&mut h, name);
    }
    assert!(h.on_title(), "the title screen returns");
    let bindings = h.input.bindings();
    for (name, action) in [
        ("KeyD", Action::P1Right),
        ("KeyE", Action::P1Up),
        ("KeyA", Action::P1Left),
        ("KeyX", Action::P1Down),
        ("ControlLeft", Action::P1Fire),
        ("ArrowLeft", Action::P2Left),
    ] {
        assert_eq!(bindings.action_of(key(name)), Some(action), "{name}");
    }
}

/// Storage whose settings are the configuration file `path`.
fn config_storage(path: &std::path::Path) -> Storage {
    Storage::new(NativeStore::new(
        path.to_path_buf(),
        path.with_file_name("scores.toml"),
    ))
}
