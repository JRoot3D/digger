//! Shell tests of what the browser build asks of the shell
//! (add-web-build-and-releases, task group 3): the pause when the page is
//! hidden, the exit policy, settings saved as they change, damaged stored
//! settings, and picked recordings that are rejected.

#![allow(
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a failed expectation in a test is a test failure"
)]

use digger_core::FrameKind;

use super::harness::Harness;
use super::{ExitPolicy, Request, Scene, Setup};
use crate::config::{GameMode, Settings};
use crate::event::AppEvent;
use crate::page::Phase;
use crate::store::{MemoryStore, Storage, StoreKey};
use crate::transfer::{MAX_IMPORT_BYTES, RecordingTransfer, TransferError};

/// A recording of a live game quit after `frames` frames.
fn recorded(frames: usize) -> Vec<u8> {
    let mut source = Harness::new();
    source.play();
    source.press("ArrowLeft");
    source.run(frames);
    source.tap("F10");
    source.frame();
    source.shell.last_game().unwrap().text.clone().into_bytes()
}

/// The notices raised outside frames, as text.
fn taken(h: &mut Harness) -> Vec<String> {
    h.shell
        .take_notices()
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn hidden_during_play_enters_pause_phase() {
    let settings = Settings {
        mode: GameMode::Gauntlet,
        ..Settings::default()
    };
    let mut h = Harness::with_settings(settings);
    h.play();
    h.press("ArrowRight");
    h.run(20);
    let ticks = h.game().state().ticks();
    // The page is hidden: the platform runs one more frame at once, which
    // carries the pause, and then stops stepping.
    h.shell.event(AppEvent::Visibility(false));
    h.frame();
    assert!(h.game().is_paused(), "the game's own pause phase");
    // The frame that honoured the request was the tick of the moment the
    // page was hidden, as with the pause key.
    assert_eq!(h.game().state().ticks(), ticks + 1);
    // Visible again: the game stays paused, however many frames run, and
    // the gauntlet clock (counted in ticks) stands still.
    h.shell.event(AppEvent::Visibility(true));
    h.release("ArrowRight");
    h.run(50);
    assert!(h.game().is_paused());
    assert_eq!(h.last_frame(), Some(FrameKind::Paused));
    assert_eq!(h.game().state().ticks(), ticks + 1);
    // Play continues only when the player resumes.
    h.tap("KeyX");
    h.frame();
    assert!(!h.game().is_paused());
    h.run(3);
    assert!(h.game().state().ticks() > ticks + 1);

    // Hidden while already paused: it stays paused, and the injected pause
    // does not resume it.
    h.tap("Space");
    h.run(2);
    assert!(h.game().is_paused());
    h.shell.event(AppEvent::Visibility(false));
    h.frame();
    h.shell.event(AppEvent::Visibility(true));
    h.run(5);
    assert!(h.game().is_paused());

    // During a level draw, the request is kept until the first tick.
    let mut h = Harness::new();
    h.shell.start_game();
    h.frame();
    assert_eq!(h.last_frame(), Some(FrameKind::Wait));
    h.shell.event(AppEvent::Visibility(false));
    h.frame();
    h.shell.event(AppEvent::Visibility(true));
    h.until_first_tick();
    assert!(h.game().is_paused());
    assert_eq!(h.game().state().ticks(), 1);
}

#[test]
fn hidden_during_playback_resumes_alone() {
    let bytes = recorded(150);
    let expected =
        digger_core::replay::verify(&bytes, digger_core::replay::ReplayOptions::default()).unwrap();
    let mut h = Harness::new();
    h.shell.event(AppEvent::RecordingPicked {
        name: String::from("ABC0.drf"),
        bytes,
    });
    h.until_first_tick();
    h.run(10);
    let ticks = h.game().state().ticks();
    // Hidden: no frames run while it is; visible again, it goes on by
    // itself from the same tick.
    h.shell.event(AppEvent::Visibility(false));
    h.shell.event(AppEvent::Visibility(true));
    h.run(5);
    assert!(!h.game().is_paused());
    assert_eq!(h.game().state().ticks(), ticks + 5);
    // A frame run while hidden is not a pause either.
    h.shell.event(AppEvent::Visibility(false));
    h.frame();
    assert!(!h.game().is_paused());
    h.shell.event(AppEvent::Visibility(true));
    let mut last = None;
    for _ in 0..5000 {
        if h.on_title() {
            break;
        }
        h.frame();
        if let Scene::Playback(playback) = h.shell.scene()
            && let Some(game) = playback.game()
        {
            last = Some((
                game.state().ticks(),
                game.state().scores().visible(digger_core::ScoreId::ALL[0]),
            ));
        }
    }
    assert!(h.on_title(), "the playback ended and the title returned");
    assert_eq!(last, Some((expected.ticks, expected.scores[0])));
    // On the title screen a hidden page changes nothing either.
    h.shell.event(AppEvent::Visibility(false));
    h.frame();
    h.shell.event(AppEvent::Visibility(true));
    h.frame();
    assert!(h.on_title());
}

#[test]
fn web_exit_stays_on_title() {
    let mut h = Harness::new();
    h.shell.set_exit_policy(ExitPolicy::StayOnTitle);
    h.run(3);
    // The exit key on the title screen: no exit request, the title runs on.
    h.tap("F10");
    h.frame();
    assert!(h.on_title());
    assert_eq!(h.requests(), [] as [Request; 0]);
    let before = h.frames.len();
    h.run(10);
    assert!(h.on_title());
    assert_eq!(h.frames.len(), before + 10);
    // A close request (none comes in a browser) does not end it either.
    h.close();
    h.frame();
    assert!(h.on_title());
    // During a game the return-to-title key ends the game, as on the
    // desktop, and the title screen is shown.
    h.play();
    h.run(5);
    h.tap("F10");
    h.frame();
    assert!(h.on_title());
    assert_eq!(h.requests(), [] as [Request; 0]);
    // A close request during a game ends it on the title screen.
    h.play();
    h.close();
    h.frame();
    assert!(h.on_title());
    assert_eq!(h.requests(), [] as [Request; 0]);
    // The desktop policy still ends the program.
    let mut desktop = Harness::new();
    desktop.tap("F10");
    desktop.frame();
    assert_eq!(desktop.requests(), [Request::Exit(0)]);
}

#[test]
fn web_settings_saved_as_they_change() {
    let mut h = Harness::with_storage(Settings::default(), Setup::default(), Storage::memory());
    h.shell.save_changed_settings();
    assert_eq!(
        h.shell.storage.store().load(StoreKey::Settings).unwrap(),
        None,
        "nothing changed, nothing written"
    );
    // Music off: written at once, not at exit.
    h.tap("F7");
    h.frame();
    h.shell.save_changed_settings();
    let text = h.shell.storage.store().load(StoreKey::Settings).unwrap();
    let text = text.unwrap();
    let file = crate::config::file::parse(&text, std::path::Path::new("digger:settings")).unwrap();
    assert_eq!(file.layer.music, Some(false));
    // Read at the next start: music is off.
    let settings = Settings::resolve(&file.layer, &crate::config::Layer::default());
    assert!(!settings.music);
}

#[test]
fn web_damaged_settings_are_replaced_by_the_next_change() {
    let store = MemoryStore::new().with(StoreKey::Settings, "this is [not valid");
    let mut h = Harness::with_storage(Settings::default(), Setup::default(), Storage::new(store));
    h.shell.replace_damaged_settings();
    // Nothing changed yet: the damaged value stays.
    h.shell.save_changed_settings();
    assert_eq!(
        h.shell
            .storage
            .store()
            .load(StoreKey::Settings)
            .unwrap()
            .as_deref(),
        Some("this is [not valid")
    );
    h.tap("F7");
    h.frame();
    h.shell.save_changed_settings();
    let text = h
        .shell
        .storage
        .store()
        .load(StoreKey::Settings)
        .unwrap()
        .unwrap();
    let file = crate::config::file::parse(&text, std::path::Path::new("digger:settings")).unwrap();
    assert_eq!(file.layer.music, Some(false));
    assert_eq!(taken(&mut h), [] as [String; 0]);

    // Without the browser's rule the damaged value is left alone, with a
    // notice (the desktop).
    let store = MemoryStore::new().with(StoreKey::Settings, "this is [not valid");
    let mut h = Harness::with_storage(Settings::default(), Setup::default(), Storage::new(store));
    h.tap("F7");
    h.frame();
    h.shell.save_settings();
    let notices = taken(&mut h);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(
        notices[0].starts_with("settings not saved: "),
        "{notices:?}"
    );
}

#[test]
fn import_oversized_is_rejected() {
    assert_eq!(MAX_IMPORT_BYTES, 16_777_216);
    assert!(crate::transfer::check_import_size("a.drf", MAX_IMPORT_BYTES).is_ok());
    // A 20 MiB video picked in the browser is not read; the picker reports
    // it, and the title screen keeps running.
    let size = 20 * 1024 * 1024;
    let error = crate::transfer::check_import_size("film.mp4", size).unwrap_err();
    assert!(matches!(&error, TransferError::TooLarge { size: s, .. } if *s == size));
    let mut h = Harness::new();
    h.frame();
    h.shell.event(AppEvent::TransferFailed(error));
    assert_eq!(
        taken(&mut h),
        [
            "cannot play back film.mp4: the file is too large (20971520 bytes; recordings are at \
          most 16 MiB)"
        ]
    );
    h.run(3);
    assert!(h.on_title());
    // Bytes that reach the shell are checked too, before parsing.
    let mut big = recorded(10);
    big.resize(usize::try_from(MAX_IMPORT_BYTES).unwrap() + 1, b'\n');
    h.shell.event(AppEvent::RecordingPicked {
        name: String::from("big.drf"),
        bytes: big,
    });
    let notices = taken(&mut h);
    assert_eq!(notices.len(), 1);
    assert!(
        notices[0].starts_with("cannot play back big.drf: the file is too large (16777217 bytes"),
        "{notices:?}"
    );
    h.frame();
    assert!(h.on_title());
}

#[test]
fn import_malformed_shows_reason() {
    let mut h = Harness::new();
    h.frame();
    let mut reject = |name: &str, bytes: &[u8]| {
        h.shell.event(AppEvent::RecordingPicked {
            name: name.to_owned(),
            bytes: bytes.to_vec(),
        });
        let notices = taken(&mut h);
        assert_eq!(notices.len(), 1, "{notices:?}");
        h.frame();
        assert!(h.on_title(), "the title keeps running");
        notices[0].clone()
    };
    // Not a recording at all.
    assert_eq!(
        reject("notes.txt", b"just some text\n"),
        "cannot play back notes.txt: the file is not a recording (it does not begin with a DRF \
         or eDRF header)"
    );
    // A recording of a kind the game does not play.
    let old = reject("old.drf", b"DRF\nAJ DOS 19981125\n");
    assert!(
        old.starts_with("cannot play back old.drf: unsupported recording: "),
        "{old}"
    );
    // A damaged recording: a header line is missing.
    let damaged = reject("cut.drf", b"DRF\n");
    assert!(
        damaged.starts_with(
            "cannot play back cut.drf: the recording is damaged: bad recording header"
        ),
        "{damaged}"
    );
    // The three reasons read differently.
    let unreadable = {
        h.shell
            .event(AppEvent::TransferFailed(TransferError::Unreadable {
                name: String::from("gone.drf"),
                reason: String::from("NotReadableError"),
            }));
        taken(&mut h)
    };
    assert_eq!(
        unreadable,
        ["cannot play back gone.drf: the file cannot be read: NotReadableError"]
    );
    // Picked away from the title screen.
    let bytes = recorded(10);
    h.play();
    h.shell.event(AppEvent::RecordingPicked {
        name: String::from("ABC0.drf"),
        bytes,
    });
    assert_eq!(
        taken(&mut h),
        ["cannot play back ABC0.drf: recordings are played from the title screen"]
    );
    assert!(matches!(h.shell.scene(), Scene::Playing(_)));
}

/// An upstream recording of the conformance suite and its expected
/// result line.
fn upstream(name: &str) -> (Vec<u8>, String) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/recordings");
    let bytes = std::fs::read(root.join("data").join(name)).unwrap();
    let stem = name.rsplit_once('.').unwrap().0;
    let expected = std::fs::read_to_string(root.join("results").join(format!("{stem}.out")))
        .unwrap()
        .trim_end()
        .to_owned();
    (bytes, expected)
}

/// Runs frames until the title screen returns (at most `limit`).
fn until_title(h: &mut Harness, limit: usize) {
    for _ in 0..limit {
        if h.on_title() {
            return;
        }
        h.frame();
    }
    assert!(h.on_title(), "the title screen did not return");
}

#[test]
fn page_status_follows_a_game() {
    let mut h = Harness::new();
    assert_eq!(h.shell.phase(), Phase::Title);
    assert!(!h.shell.can_save(), "nothing to save on a fresh page");
    assert_eq!(h.shell.playback_result(), None);
    h.play();
    assert_eq!(h.shell.phase(), Phase::Playing);
    h.until_first_tick();
    h.tap("Space");
    h.run(3);
    assert_eq!(h.shell.phase(), Phase::Paused);
    h.tap("Space");
    h.run(3);
    assert_eq!(h.shell.phase(), Phase::Playing);
}

#[test]
fn save_button_saves_like_the_key() {
    let mut h = Harness::new();
    h.shell.set_exit_policy(ExitPolicy::StayOnTitle);
    h.play();
    h.run(30);
    h.tap("F10");
    h.frame();
    assert!(h.on_title());
    assert!(h.shell.last_game().is_some());
    // Nowhere to save: the button stays disabled and does nothing.
    assert!(!h.shell.can_save());
    h.shell.event(AppEvent::SaveRecording);
    assert!(h.shell.last_game().is_some());
    let transfer = RecordingTransfer::default();
    h.shell.set_transfer(transfer.clone());
    assert!(h.shell.can_save());
    let text = h.shell.last_game().unwrap().text.clone();
    let name = h.shell.last_game().unwrap().auto_name();
    h.shell.event(AppEvent::SaveRecording);
    assert_eq!(
        *transfer.exported.borrow(),
        [(name, text.into_bytes())],
        "the name and bytes of the save-recording key"
    );
    // Once only: the button is disabled again.
    assert!(!h.shell.can_save());
    h.shell.event(AppEvent::SaveRecording);
    assert_eq!(transfer.exported.borrow().len(), 1);
    assert_eq!(taken(&mut h), Vec::<String>::new());
}

#[test]
fn save_button_is_disabled_away_from_the_title() {
    let mut h = Harness::new();
    let transfer = RecordingTransfer::default();
    h.shell.set_transfer(transfer.clone());
    h.play();
    h.run(30);
    h.tap("F10");
    h.frame();
    assert!(h.shell.can_save());
    h.play();
    assert!(!h.shell.can_save());
    h.shell.event(AppEvent::SaveRecording);
    assert!(transfer.exported.borrow().is_empty());
}

#[test]
fn picked_recording_shows_its_result() {
    let (bytes, expected) = upstream("2riptest01.edrf");
    let mut h = Harness::new();
    h.shell.event(AppEvent::RecordingPicked {
        name: String::from("2riptest01.edrf"),
        bytes,
    });
    assert_eq!(h.shell.phase(), Phase::Playback);
    until_title(&mut h, 5000);
    assert_eq!(h.shell.playback_result(), Some(expected.as_str()));
    assert_eq!(taken(&mut h), Vec::<String>::new());
    assert_eq!(h.notices(), Vec::<String>::new());
}

#[test]
fn picked_recording_that_diverges_is_reported() {
    let (bytes, _) = upstream("2riptest01.edrf");
    let text = String::from_utf8(bytes).unwrap();
    // The first state checkpoint no longer matches.
    let broken = text.replacen("C 966B1F36", "C 00000000", 1);
    assert_ne!(broken, text);
    let mut h = Harness::new();
    h.shell.event(AppEvent::RecordingPicked {
        name: String::from("broken.edrf"),
        bytes: broken.into_bytes(),
    });
    until_title(&mut h, 5000);
    let result = h.shell.playback_result().unwrap().to_owned();
    assert!(result.contains("tick"), "{result}");
    let notices = h.notices();
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(
        notices[0].starts_with("playback of broken.edrf did not match its recording: ")
            && notices[0].ends_with(&result),
        "{notices:?}"
    );
}
