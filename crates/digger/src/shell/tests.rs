//! Shell tests: scripted frames against the scenes (design D15).

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a failed expectation in a test is a test failure"
)]

use digger_core::assets::{ImageId, TitlePicture};
use digger_core::geom::Px;
use digger_core::recording::RecordingKind;
use digger_core::replay::ReplayOptions;
use digger_core::video::{Framebuffer, TextColor};
use digger_core::{DiggerId, Direction, FinishReason, FrameKind, SoundCommand};
use digger_sound::{ManualDriver, SoundConfig, SoundControl, SoundSink, TuneTables};

use super::harness::{Harness, key};
use super::{Request, Scene, Setup, Shell};
use crate::config::{FrameTime, GameMode, Settings, Speed};
use crate::input::{KeyPress, PadButton, PadInput, PadSignal};
use crate::scores::{Entry, LevelSet, Scores, TableKey};

/// Settings with `speed`.
fn at_speed(speed: u16) -> Settings {
    Settings {
        speed: Speed::new(speed).unwrap(),
        ..Settings::default()
    }
}

/// The frame time in microseconds.
fn micros(time: FrameTime) -> u128 {
    time.as_micros()
}

/// The first digger's position.
fn digger_position(harness: &Harness) -> digger_core::geom::Px {
    harness.game().state().digger(DiggerId::ALL[0]).position()
}

#[test]
fn shell_starts_on_title() {
    let mut harness = Harness::new();
    assert!(harness.on_title());
    let blank = Framebuffer::new().crc32();
    let frame = harness.frame().clone();
    assert!(harness.on_title());
    // The title scene shows its own screen, not a game's framebuffer.
    assert_eq!(frame.picture_crc, harness.shell.picture().crc32());
    assert_ne!(frame.picture_crc, blank);
    assert_eq!(frame.requests, [] as [Request; 0]);
    assert_eq!(frame.sound, [] as [SoundCommand; 0]);
    assert_eq!(micros(frame.frame_time), 80_000);
    // A close request on the title ends the program with status 0.
    harness.close();
    assert_eq!(harness.frame().requests, [Request::Exit(0)]);
}

#[test]
fn play_scene_steps_one_frame_per_call() {
    let mut harness = Harness::new();
    harness.shell.start_game();
    let mut ticks = 0_u32;
    let mut waits = 0_u32;
    for _ in 0..300 {
        harness.frame();
        match harness.last_frame() {
            Some(FrameKind::Tick) => ticks += 1,
            Some(FrameKind::Wait) => waits += 1,
            other => panic!("unexpected frame {other:?}"),
        }
    }
    let Scene::Playing(play) = harness.shell.scene() else {
        panic!("the game is still running");
    };
    assert_eq!(play.frames(), 300, "one step per shell frame");
    assert_eq!(
        play.game().state().ticks(),
        ticks,
        "ticks equal tick frames"
    );
    assert!(waits > 0, "the level draw waits");
    assert!(ticks > 200, "the main phase ticks");
    // The presented picture is the game's own framebuffer.
    assert!(std::ptr::eq(
        harness.shell.picture(),
        play.game().framebuffer()
    ));
    // The core's sound commands are forwarded: the main tune starts.
    assert!(
        harness
            .sound_since(0)
            .iter()
            .any(|command| matches!(command, SoundCommand::Music { .. }))
    );
}

#[test]
fn pause_consumes_no_ticks() {
    let mut harness = Harness::with_settings(Settings {
        mode: GameMode::Gauntlet,
        ..Settings::default()
    });
    harness.play();
    harness.run(20);
    harness.tap("Space");
    let pausing = harness.frames.len();
    harness.frame();
    assert!(harness.game().is_paused(), "the pause takes effect at once");
    assert!(harness.frames[pausing].sound.contains(&SoundCommand::Pause));
    let ticks = harness.game().state().ticks();
    let clock = harness.game().state().gauntlet_clock();
    assert!(clock.is_some());
    for _ in 0..100 {
        harness.frame();
        assert_eq!(harness.last_frame(), Some(FrameKind::Paused));
    }
    assert_eq!(harness.game().state().ticks(), ticks, "no tick elapsed");
    assert_eq!(
        harness.game().state().gauntlet_clock(),
        clock,
        "clock stood"
    );
    // Holding the pause key does not resume; a new press does.
    harness.tap("KeyX");
    let resuming = harness.frames.len();
    harness.frame();
    assert!(!harness.game().is_paused());
    assert!(
        harness.frames[resuming]
            .sound
            .contains(&SoundCommand::Resume)
    );
    assert_eq!(harness.game().state().ticks(), ticks);
    harness.run(10);
    assert_eq!(harness.game().state().ticks(), ticks + 10);
}

#[test]
fn pause_text_comes_from_core() {
    let mut harness = Harness::new();
    harness.play();
    harness.run(5);
    harness.tap("Space");
    harness.run(3);
    assert!(harness.game().is_paused());
    let shown = harness.frames.last().unwrap().picture_crc;
    assert_eq!(shown, harness.game().framebuffer().crc32());
    assert!(std::ptr::eq(
        harness.shell.picture(),
        harness.game().framebuffer()
    ));
    // Nothing is drawn over the paused game: the picture stays the core's.
    harness.run(10);
    assert!(
        harness
            .frames
            .iter()
            .rev()
            .take(10)
            .all(|frame| frame.picture_crc == shown)
    );
}

#[test]
fn resume_key_not_a_control() {
    let mut harness = Harness::new();
    harness.play();
    harness.run(3);
    harness.tap("Space");
    harness.run(3);
    assert!(harness.game().is_paused());
    // The fire key resumes and is held on.
    harness.press("F1");
    let resumed = harness.frames.len();
    harness.frame();
    assert!(!harness.game().is_paused());
    harness.run(20);
    let fired = |sound: &[SoundCommand]| {
        sound
            .iter()
            .any(|command| matches!(command, SoundCommand::Fire(_)))
    };
    assert!(
        !fired(&harness.sound_since(resumed)),
        "the resume press fired"
    );
    // Released and pressed again, the fire key fires.
    harness.release("F1");
    harness.frame();
    harness.press("F1");
    let pressed = harness.frames.len();
    harness.run(5);
    assert!(fired(&harness.sound_since(pressed)), "a new press fires");
}

#[test]
fn focus_loss_pauses_live_game() {
    let mut harness = Harness::new();
    harness.play();
    harness.press("ArrowRight");
    harness.run(3);
    harness.lose_focus();
    harness.frame();
    assert!(harness.game().is_paused(), "focus loss pauses");
    harness.run(5);
    assert!(harness.game().is_paused(), "and the pause holds");
    // Back with focus, a key resumes, and Digger does not keep moving.
    harness.tap("KeyX");
    harness.frame();
    assert!(!harness.game().is_paused());
    harness.run(2);
    let at = digger_position(&harness);
    harness.run(10);
    assert_eq!(digger_position(&harness), at, "nothing moves by itself");

    // A removed pad pauses the same way, and its held d-pad is released.
    harness.pad(PadSignal::Input {
        slot: 0,
        input: PadInput::DPad(Direction::Right),
        pressed: true,
    });
    harness.run(3);
    harness.pad(PadSignal::Removed(0));
    harness.frame();
    assert!(harness.game().is_paused(), "pad removal pauses");
    harness.tap("KeyX");
    harness.run(3);
    assert!(!harness.game().is_paused());
    let at = digger_position(&harness);
    harness.run(10);
    assert_eq!(digger_position(&harness), at, "the d-pad was released");
}

#[test]
fn focus_loss_pauses_during_level_draw_once_play_starts() {
    let mut harness = Harness::new();
    harness.shell.start_game();
    harness.frame();
    assert_eq!(harness.last_frame(), Some(FrameKind::Wait));
    harness.lose_focus();
    // The request is kept until the core honours it on its first tick.
    harness.until_first_tick();
    assert!(harness.game().is_paused());
    assert_eq!(harness.game().state().ticks(), 1);
    harness.run(5);
    assert_eq!(harness.game().state().ticks(), 1);
}

#[test]
fn focus_loss_ignored_on_title_and_playback() {
    // The title keeps running.
    let mut harness = Harness::new();
    harness.lose_focus();
    let frame = harness.frame().clone();
    assert!(harness.on_title());
    assert_eq!(frame.requests, [] as [Request; 0]);

    // A recording to play back: a live game quit after 200 frames.
    let mut source = Harness::new();
    source.play();
    source.run(200);
    source.tap("F10");
    source.frame();
    let text = source.shell.last_game().unwrap().text.clone();
    let recording = digger_core::recording::parse(text.as_bytes()).unwrap();

    let mut harness = Harness::new();
    harness.shell.start_playback(&recording).unwrap();
    harness.until_first_tick();
    harness.run(10);
    let ticks = harness.game().state().ticks();
    harness.lose_focus();
    harness.run(5);
    assert!(!harness.game().is_paused(), "playback does not pause");
    assert_eq!(harness.game().state().ticks(), ticks + 5);
    // The pad removal does not pause a playback either.
    harness.pad(PadSignal::Removed(0));
    harness.run(5);
    assert!(!harness.game().is_paused());
    assert_eq!(harness.game().state().ticks(), ticks + 10);
    // Playback runs to its end and returns to the title.
    for _ in 0..1000 {
        if harness.on_title() {
            break;
        }
        harness.frame();
    }
    assert!(harness.on_title(), "the playback ended");
}

#[test]
fn speed_floor() {
    let mut harness = Harness::new();
    for _ in 0..7 {
        harness.tap("NumpadAdd");
        harness.frame();
    }
    assert_eq!(micros(harness.shell.frame_time()), 10_000);
    // The frame's output carries the new frame time for the pacer.
    assert_eq!(micros(harness.frames.last().unwrap().frame_time), 10_000);
    harness.tap("Equal");
    harness.frame();
    assert_eq!(micros(harness.shell.frame_time()), 10_000, "no lower");

    // Above 10000 µs a step may go below it: 14000 → 4000, then no more.
    let mut harness = Harness::with_settings(at_speed(7));
    harness.tap("NumpadAdd");
    harness.frame();
    assert_eq!(micros(harness.shell.frame_time()), 4_000);
    harness.tap("NumpadAdd");
    harness.frame();
    assert_eq!(micros(harness.shell.frame_time()), 4_000);

    // Holding the key acts once.
    let mut harness = Harness::new();
    harness.press("NumpadAdd");
    harness.run(25);
    assert_eq!(micros(harness.shell.frame_time()), 70_000);
    // Speed changes are never saved.
    assert_eq!(harness.shell.runtime_changes().speed, None);
    assert!(harness.shell.runtime_changes().is_empty());
}

#[test]
fn speed_cap() {
    let mut harness = Harness::new();
    harness.tap("NumpadSubtract");
    harness.frame();
    assert_eq!(micros(harness.shell.frame_time()), 90_000);

    let mut harness = Harness::with_settings(at_speed(495));
    harness.tap("Minus");
    harness.frame();
    assert_eq!(micros(harness.shell.frame_time()), 1_000_000);
    harness.tap("Minus");
    harness.frame();
    assert_eq!(micros(harness.shell.frame_time()), 1_000_000, "capped");
    assert!(harness.shell.runtime_changes().is_empty());

    // Speed works during a game and does not change its course: the
    // same game at two speeds ticks the same.
    let mut fast = Harness::new();
    let mut slow = Harness::new();
    fast.play();
    slow.play();
    fast.tap("NumpadAdd");
    slow.tap("NumpadSubtract");
    fast.run(100);
    slow.run(100);
    assert_eq!(
        fast.game().framebuffer().crc32(),
        slow.game().framebuffer().crc32()
    );
    assert!(fast.shell.frame_time() < slow.shell.frame_time());
}

#[test]
fn toggles_recorded_for_save() {
    let mut harness = Harness::new();
    harness.tap("F7");
    let frame = harness.frame().clone();
    assert_eq!(frame.controls, [SoundControl::ToggleMusic]);
    assert!(!harness.shell.music_on());
    assert_eq!(harness.shell.runtime_changes().music, Some(false));
    assert_eq!(harness.shell.runtime_changes().sound_enabled, None);

    harness.tap("F9");
    let frame = harness.frame().clone();
    assert_eq!(frame.controls, [SoundControl::ToggleSound]);
    assert_eq!(
        frame.requests,
        [] as [Request; 0],
        "switching off needs no device"
    );
    assert!(!harness.shell.sound_on());
    assert_eq!(harness.shell.runtime_changes().sound_enabled, Some(false));

    // Switching sound on also switches music on (sound quirk SQ10), and
    // asks the platform to open the device if it is closed.
    harness.tap("F9");
    let frame = harness.frame().clone();
    assert_eq!(frame.requests, [Request::RetryAudio]);
    assert!(harness.shell.sound_on() && harness.shell.music_on());
    assert_eq!(harness.shell.runtime_changes().sound_enabled, Some(true));
    assert_eq!(harness.shell.runtime_changes().music, Some(true));
    // Nothing else is entered for saving.
    let mut rest = harness.shell.runtime_changes().clone();
    rest.sound_enabled = None;
    rest.music = None;
    assert!(rest.is_empty());

    // Two toggles in one frame each act once; they work in a game too.
    let mut harness = Harness::new();
    harness.play();
    harness.press("F7");
    harness.press("F9");
    let frame = harness.frame().clone();
    assert_eq!(
        frame.controls,
        [SoundControl::ToggleMusic, SoundControl::ToggleSound]
    );
    harness.run(5);
    assert!(
        harness
            .frames
            .iter()
            .rev()
            .take(5)
            .all(|f| f.controls.is_empty())
    );
}

#[test]
fn quiet_start_then_toggle_enables_sound() {
    let settings = Settings {
        sound_enabled: false,
        ..Settings::default()
    };
    let mut driver = ManualDriver::new(
        SoundConfig {
            sound_on: settings.sound_enabled,
            music_on: settings.music,
            ..SoundConfig::DEFAULT
        },
        TuneTables::embedded().unwrap(),
    );
    let mut harness = Harness::with_settings(settings);
    harness.play();
    harness.run(5);
    harness.tap("F9");
    harness.run(5);
    for frame in &harness.frames {
        for &command in &frame.sound {
            driver.send(command);
        }
        for &control in &frame.controls {
            driver.control(control);
        }
    }
    driver.advance_ticks(1);
    assert!(harness.shell.sound_on());
    assert!(driver.synth().sound_on(), "the engine's sound is on");
    assert!(driver.synth().music_on());
    assert_eq!(harness.shell.runtime_changes().sound_enabled, Some(true));
    assert!(harness.requests().contains(&Request::RetryAudio));
}

#[test]
fn quit_returns_to_title_without_banner() {
    let mut harness = Harness::new();
    harness.play();
    harness.run(20);
    harness.tap("F10");
    let frame = harness.frame().clone();
    // Straight to the title on the quit frame: no 50-frame notice.
    assert!(harness.on_title());
    assert_eq!(
        frame.picture_crc,
        Harness::new().shell.picture().crc32(),
        "the title screen as it opens"
    );
    assert_eq!(frame.sound.last(), Some(&SoundCommand::StopAll));
    assert_eq!(
        frame.requests,
        [] as [Request; 0],
        "quit in a game does not exit"
    );
    let last = harness.shell.last_game().unwrap();
    assert_eq!(last.summary.reason, FinishReason::Quit);

    // Quitting from a pause works the same way.
    let mut harness = Harness::new();
    harness.play();
    harness.tap("Space");
    harness.run(3);
    harness.tap("F10");
    harness.frame();
    assert!(harness.on_title());
    assert_eq!(
        harness.shell.last_game().unwrap().summary.reason,
        FinishReason::Quit
    );
}

#[test]
fn close_during_game_finalises_recording() {
    for kind in [RecordingKind::Drf, RecordingKind::Edrf] {
        let mut harness = Harness::recording(kind);
        harness.play();
        harness.press("ArrowLeft");
        harness.run(60);
        harness.release("ArrowLeft");
        harness.tap("Space");
        harness.run(5);
        harness.close();
        let frame = harness.frame().clone();
        assert_eq!(frame.requests, [Request::Exit(0)], "{kind:?}");
        let last = harness.shell.last_game().unwrap();
        assert_eq!(last.kind, kind);
        assert_eq!(last.summary.reason, FinishReason::Quit);
        // The end-of-game marker is there.
        let end = match kind {
            RecordingKind::Drf => "EOG",
            RecordingKind::Edrf => "Z ",
        };
        assert!(
            last.text.lines().any(|line| line.starts_with(end)),
            "{kind:?} has its end marker"
        );
        // And the recording replays to the same game.
        let report =
            digger_core::replay::verify(last.text.as_bytes(), ReplayOptions::default()).unwrap();
        assert_eq!(
            report.exit_status(),
            0,
            "{kind:?}: {}",
            report.result_line()
        );
        // A DRF playback counts the tick whose control read finds the end
        // of the data before it quits (compat quirk P2).
        let extra = u32::from(kind == RecordingKind::Drf);
        assert_eq!(report.ticks, last.summary.ticks + extra, "{kind:?}");
    }
}

// Title screen (task group 8).

/// Whether `text` is shown at `at` in `color` on `picture`: drawing it
/// there again changes nothing.
fn shows(picture: &Framebuffer, text: &str, at: (i32, i32), color: u8) -> bool {
    let mut copy = picture.clone();
    copy.draw_text(text, Px::new(at.0, at.1), TextColor::new(color).unwrap());
    copy == *picture
}

/// The title picture with the texts every title screen shows.
fn title_backdrop() -> Framebuffer {
    let mut expected = Framebuffer::new();
    expected.draw_title(&TitlePicture::decode().unwrap());
    expected.draw_text("D I G G E R", Px::new(100, 0), TextColor::Three);
    expected.draw_text("HIGH SCORES", Px::new(16, 25), TextColor::Three);
    expected
}

/// Score tables holding one entry `initials` with `score` in the table of
/// each listed mode.
fn scores_with(entries: &[(GameMode, &[u8; 3], u32)]) -> Scores {
    let mut scores = Scores::default();
    for &(mode, initials, score) in entries {
        let key = TableKey::from_mode(mode, LevelSet::Builtin);
        assert!(scores.insert(&key, Entry::new(*initials, score)).is_some());
    }
    scores
}

/// A harness opening on `mode` with `scores`.
fn title_harness(mode: GameMode, scores: Scores) -> Harness {
    let settings = Settings {
        mode,
        ..Settings::default()
    };
    let mut harness = Harness::with_settings(settings.clone());
    harness.shell = Shell::with_scores(settings, Setup::default(), scores);
    harness
}

#[test]
fn title_static_layout() {
    // First start with default settings and no score file.
    let harness = Harness::new();
    let mut expected = title_backdrop();
    expected.draw_text("ONE", Px::new(220, 25), TextColor::Three);
    expected.draw_text(" PLAYER ", Px::new(192, 39), TextColor::Three);
    for i in 1..=10 {
        let color = if i == 1 {
            TextColor::Two
        } else {
            TextColor::One
        };
        expected.draw_text("...       0", Px::new(16, 31 + 13 * i), color);
    }
    assert!(
        *harness.shell.picture() == expected,
        "title picture, name, label and an empty table"
    );

    // A table's third entry is shown at (16, 70) in colour 1, the first in
    // colour 2.
    let scores = scores_with(&[
        (GameMode::OnePlayer, b"ABC", 12_345),
        (GameMode::OnePlayer, b"XYZ", 20_000),
        (GameMode::OnePlayer, b"MSX", 30_000),
    ]);
    let harness = title_harness(GameMode::OnePlayer, scores);
    let picture = harness.shell.picture();
    assert!(shows(picture, "MSX   30000", (16, 44), 2));
    assert!(shows(picture, "XYZ   20000", (16, 57), 1));
    assert!(shows(picture, "ABC   12345", (16, 70), 1));
    assert!(shows(picture, "...       0", (16, 161), 1));
}

#[test]
fn title_frame0_crc() {
    let mut harness = Harness::new();
    let crc = harness.frame().picture_crc;
    assert_eq!(crc, TITLE_FRAME0_CRC, "golden CRC of animation frame 0");
}

/// Golden CRCs of the default title screen after animation frames 0, 164,
/// 240 and 286, recorded from the port.
const TITLE_FRAME0_CRC: u32 = 2_938_196_278;
const TITLE_FRAME164_CRC: u32 = 1_154_256_484;
const TITLE_FRAME240_CRC: u32 = 2_636_550_824;
const TITLE_FRAME286_CRC: u32 = 4_128_195_795;

#[test]
fn title_script_keyframes() {
    let labels = [
        ("NOBBIN", (216, 64), 83),
        ("HOBBIN", (216, 83), 156),
        ("DIGGER", (216, 102), 163),
        ("GOLD", (216, 121), 183),
        ("EMERALD", (216, 140), 203),
        ("BONUS", (216, 159), 223),
    ];
    let mut harness = Harness::new();
    let mut pictures = Vec::new();
    // Two whole cycles and the first frame of a third.
    for _ in 0..=(2 * 287) {
        harness.frame();
        pictures.push(harness.shell.picture().clone());
    }
    // `pictures[f]` is the picture after animation frame f.
    for cycle in [0, 287] {
        for &(text, at, from) in &labels {
            assert!(
                !shows(&pictures[cycle + from - 1], text, at, 2),
                "{text} before frame {from}"
            );
            for f in from..=286 {
                assert!(
                    shows(&pictures[cycle + f], text, at, 2),
                    "{text} at frame {f}"
                );
            }
        }
    }
    // The spec's scenario: after 164 frames shown, the first three labels.
    let after_164 = &pictures[163];
    assert!(shows(after_164, "NOBBIN", (216, 64), 2));
    assert!(shows(after_164, "HOBBIN", (216, 83), 2));
    assert!(shows(after_164, "DIGGER", (216, 102), 2));
    assert!(!shows(after_164, "GOLD", (216, 121), 2));

    // Frame 0 clears the cast panel: each cycle starts from the same
    // picture, with nothing between x = 164 and 308, y = 54 and 174.
    let panel_is_clear = |picture: &Framebuffer| {
        let pixels = picture.pixels();
        (108..348).all(|row| (328..616).all(|col| pixels[row * 640 + col] == 0))
    };
    assert!(!panel_is_clear(&pictures[286]), "the cast is on show");
    assert!(panel_is_clear(&pictures[0]));
    assert!(panel_is_clear(&pictures[287]), "the cycle restarts");
    assert_eq!(pictures[287].crc32(), pictures[0].crc32());
    assert_eq!(pictures[574].crc32(), pictures[0].crc32());

    // A nobbin appears at (292, 63) on frame 50; the hobbin and the nobbin
    // are shot (they leave the picture on frames 237 and 260).
    let nobbin_at = |picture: &Framebuffer, image: ImageId, at: (i32, i32)| {
        let mut copy = picture.clone();
        copy.blit_masked(image, Px::new(at.0, at.1));
        copy == *picture
    };
    assert!(!nobbin_at(&pictures[49], ImageId::NOBBIN_1, (292, 63)));
    assert!(nobbin_at(&pictures[50], ImageId::NOBBIN_1, (292, 63)));

    assert_eq!(pictures[0].crc32(), TITLE_FRAME0_CRC);
    assert_eq!(pictures[164].crc32(), TITLE_FRAME164_CRC);
    assert_eq!(pictures[240].crc32(), TITLE_FRAME240_CRC);
    assert_eq!(pictures[286].crc32(), TITLE_FRAME286_CRC);
}

#[test]
fn mode_cycle_order() {
    let mut harness = Harness::new();
    harness.frame();
    let order = [
        (GameMode::TwoPlayers, "TWO", 220, " PLAYERS", 184),
        (GameMode::TwoDiggers, "TWO PLAYER", 180, "SIMULTANEOUS", 170),
        (GameMode::Gauntlet, "GAUNTLET", 192, "MODE", 216),
        (
            GameMode::TwoDiggerGauntlet,
            "TWO PLAYER",
            180,
            "GAUNTLET",
            192,
        ),
        (GameMode::OnePlayer, "ONE", 220, " PLAYER ", 192),
    ];
    for (mode, line1, x1, line2, x2) in order {
        harness.tap("KeyN");
        harness.frame();
        assert!(harness.on_title(), "no game starts");
        assert_eq!(harness.shell.mode(), mode);
        let picture = harness.shell.picture();
        assert!(shows(picture, line1, (x1, 25), 3), "{mode:?} line 1");
        assert!(shows(picture, line2, (x2, 39), 3), "{mode:?} line 2");
        // The change is kept for saving at exit.
        assert_eq!(harness.shell.runtime_changes().mode, Some(mode));
    }
    // Two presses in one frame advance twice.
    harness.tap("KeyN");
    harness.tap("KeyN");
    harness.frame();
    assert_eq!(harness.shell.mode(), GameMode::TwoDiggers);
    // A game started now is in the selected mode.
    harness.tap("Space");
    harness.frame();
    assert_eq!(
        harness.game().state().mode(),
        digger_core::GameMode::TwoDiggers
    );
}

#[test]
fn mode_cycle_wraps_and_swaps_table() {
    let scores = scores_with(&[
        (GameMode::OnePlayer, b"ONE", 11_111),
        (GameMode::TwoDiggers, b"TWO", 22_222),
        (GameMode::TwoDiggerGauntlet, b"GTW", 33_333),
    ]);
    let mut harness = title_harness(GameMode::TwoDiggerGauntlet, scores);
    harness.frame();
    let picture = harness.shell.picture();
    assert!(shows(picture, "TWO PLAYER", (180, 25), 3));
    assert!(shows(picture, "GAUNTLET", (192, 39), 3));
    assert!(shows(picture, "GTW   33333", (16, 44), 2));

    harness.tap("KeyN");
    harness.frame();
    assert!(harness.on_title());
    assert_eq!(harness.shell.mode(), GameMode::OnePlayer);
    let picture = harness.shell.picture();
    assert!(shows(picture, "ONE", (220, 25), 3));
    assert!(shows(picture, " PLAYER ", (192, 39), 3));
    assert!(
        shows(picture, "ONE   11111", (16, 44), 2),
        "one-digger table"
    );
    assert!(shows(picture, "...       0", (16, 57), 1));
    // The longer label of the previous mode is gone entirely.
    let mut cleared = picture.clone();
    cleared.erase_text(10, Px::new(180, 25));
    cleared.erase_text(12, Px::new(170, 39));
    cleared.draw_text("ONE", Px::new(220, 25), TextColor::Three);
    cleared.draw_text(" PLAYER ", Px::new(192, 39), TextColor::Three);
    assert_eq!(cleared.crc32(), picture.crc32());

    // Two players alternating share the one-Digger table.
    harness.tap("KeyN");
    harness.frame();
    assert!(shows(harness.shell.picture(), "ONE   11111", (16, 44), 2));
    harness.tap("KeyN");
    harness.frame();
    assert!(shows(harness.shell.picture(), "TWO   22222", (16, 44), 2));
}

#[test]
fn title_space_starts() {
    let mut harness = Harness::new();
    harness.run(10);
    harness.tap("Space");
    harness.frame();
    assert!(matches!(harness.shell.scene(), Scene::Playing(_)));
    assert_eq!(
        harness.game().state().mode(),
        digger_core::GameMode::OnePlayer
    );
    let from = harness.frames.len();
    harness.until_first_tick();
    assert!(
        harness
            .sound_since(from)
            .iter()
            .any(|command| matches!(command, SoundCommand::Music { .. })),
        "the main tune plays"
    );

    // Any other key that is no excluded function key starts as well, and
    // so do a pad's fire and pause buttons.
    for start in ["KeyX", "ArrowLeft", "Enter", "F1"] {
        let mut harness = Harness::new();
        harness.frame();
        harness.tap(start);
        harness.frame();
        assert!(!harness.on_title(), "{start} starts");
    }
    for button in [PadButton::South, PadButton::West, PadButton::Start] {
        let mut harness = Harness::new();
        harness.pad(PadSignal::Connected(0));
        harness.frame();
        harness.pad(PadSignal::Input {
            slot: 0,
            input: PadInput::Button(button),
            pressed: true,
        });
        harness.frame();
        assert!(!harness.on_title(), "{button:?} starts");
    }
}

#[test]
fn title_f7_does_not_start() {
    let mut harness = Harness::new();
    harness.frame();
    harness.tap("F7");
    let frame = harness.frame().clone();
    assert!(harness.on_title());
    assert_eq!(frame.controls, [SoundControl::ToggleMusic]);
    assert!(!harness.shell.music_on());
    // Neither do the other excluded function keys, nor modifiers.
    for key in [
        "F9",
        "NumpadAdd",
        "NumpadSubtract",
        "F11",
        "KeyN",
        "KeyK",
        "F8",
        "ShiftLeft",
        "ControlRight",
        "AltLeft",
        "MetaLeft",
    ] {
        harness.tap(key);
        harness.frame();
        assert!(harness.on_title(), "{key} does not start");
    }
    // The pad's mode-change button does not start either.
    harness.pad(PadSignal::Connected(0));
    harness.pad(PadSignal::Input {
        slot: 0,
        input: PadInput::Button(PadButton::Select),
        pressed: true,
    });
    harness.frame();
    assert!(harness.on_title());
}

#[test]
fn title_held_key_does_not_start() {
    let mut harness = Harness::new();
    harness.play();
    // The arrow key is held down from the game into the title screen.
    harness.press("ArrowRight");
    harness.run(3);
    harness.tap("F10");
    harness.frame();
    assert!(harness.on_title());
    harness.run(50);
    assert!(harness.on_title(), "a held key does not start");
    // Its key repeats do not either.
    harness.input.key_down(KeyPress {
        key: Some(key("ArrowRight")),
        repeat: true,
        text: None,
    });
    harness.frame();
    assert!(harness.on_title(), "a repeat does not start");
    // Released and pressed again, it does.
    harness.release("ArrowRight");
    harness.frame();
    harness.press("ArrowRight");
    harness.frame();
    assert!(!harness.on_title());
}

#[test]
fn title_quit_exits_zero() {
    let mut harness = Harness::new();
    harness.run(5);
    harness.tap("F10");
    let frame = harness.frame().clone();
    assert_eq!(frame.requests, [Request::Exit(0)]);
}
