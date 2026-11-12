//! Shell tests: scripted frames against the scenes (design D15).

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a failed expectation in a test is a test failure"
)]

use digger_core::recording::RecordingKind;
use digger_core::replay::ReplayOptions;
use digger_core::video::Framebuffer;
use digger_core::{DiggerId, Direction, FinishReason, FrameKind, SoundCommand};
use digger_sound::{ManualDriver, SoundConfig, SoundControl, SoundSink, TuneTables};

use super::harness::Harness;
use super::{Request, Scene};
use crate::config::{FrameTime, GameMode, Settings, Speed};
use crate::input::{PadInput, PadSignal};

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
    assert_eq!(frame.picture_crc, blank);
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
    assert_eq!(frame.picture_crc, Framebuffer::new().crc32());
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
