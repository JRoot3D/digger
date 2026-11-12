//! Shell tests of on-screen playback, the `--play` list, `--loop`,
//! play-and-verify in the window and taking over a playback (task
//! group 11).

#![allow(clippy::panic, reason = "a failed expectation is a failed test")]
#![allow(clippy::unwrap_used, reason = "a failed expectation is a failed test")]
#![allow(
    clippy::indexing_slicing,
    reason = "a failed expectation is a failed test"
)]

use std::fs;
use std::path::{Path, PathBuf};

use digger_core::recording::RecordingKind;
use digger_core::replay::{Outcome, ReplayOptions};
use digger_core::{DiggerId, state_hash};

use super::harness::Harness;
use super::{Request, Scene, Setup};
use crate::config::Settings;
use crate::config::file::tests::TempDir;

/// The text of a live game recorded as `kind`: Digger holds `keys` for
/// `hold` frames, then stands still until the game is quit after `total`
/// frames of play.
fn recorded(kind: RecordingKind, keys: &[&str], hold: usize, total: usize) -> String {
    let mut h = Harness::recording(kind);
    h.play();
    for key in keys {
        h.press(key);
    }
    h.run(hold);
    for key in keys {
        h.release(key);
    }
    h.run(total.saturating_sub(hold));
    h.tap("F10");
    h.frame();
    assert!(h.on_title());
    h.shell.last_game().unwrap().text.clone()
}

/// Writes `text` to `name` in `dir` and returns the file.
fn write(dir: &TempDir, name: &str, text: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, text).unwrap();
    path
}

/// A harness that starts on the `--play` list `files`.
fn playing(files: &[PathBuf], loop_playback: bool, take_over_at: Option<u32>) -> Harness {
    Harness::with(
        Settings::default(),
        Setup {
            play: files.to_vec(),
            loop_playback,
            take_over_at,
            ..Setup::default()
        },
    )
}

/// The `--play` file being played, if a playback is shown.
fn source(h: &Harness) -> Option<PathBuf> {
    match h.shell.scene() {
        Scene::Playback(playback) => playback.source().map(Path::to_path_buf),
        _ => None,
    }
}

/// The tick count and state hash of the game shown.
fn snapshot(h: &Harness) -> (u32, u32) {
    let state = h.game().state();
    (state.ticks(), state_hash::hash(state))
}

/// The first Digger's x position in the game shown.
fn digger_x(h: &Harness) -> i32 {
    h.game().state().digger(DiggerId::ALL[0]).position().x
}

#[test]
fn playback_ignores_player_keys() {
    let dir = TempDir::new("ignores-keys");
    let file = write(
        &dir,
        "game.drf",
        &recorded(RecordingKind::Drf, &["ArrowLeft"], 60, 200),
    );
    // Watched without touching a key, then again with every player
    // control held and fire tapped throughout.
    let mut quiet = playing(std::slice::from_ref(&file), false, None);
    let mut busy = playing(&[file], false, None);
    busy.press("ArrowRight");
    busy.press("ArrowUp");
    let mut frames = 0;
    while !quiet.on_title() {
        busy.tap("F1");
        quiet.frame();
        busy.frame();
        frames += 1;
        assert!(frames < 20_000);
        if quiet.on_title() {
            break;
        }
        assert_eq!(snapshot(&quiet), snapshot(&busy), "frame {frames}");
        assert_eq!(
            quiet.frames.last().unwrap().picture_crc,
            busy.frames.last().unwrap().picture_crc,
            "frame {frames}"
        );
    }
    assert!(busy.on_title(), "both end on the same frame");
    // The ended playback never asked for initials.
    assert!(quiet.shell.last_game().is_none(), "a playback is not kept");
}

#[test]
fn playback_list_then_title() {
    let dir = TempDir::new("list");
    let a = write(
        &dir,
        "a.drf",
        &recorded(RecordingKind::Drf, &["ArrowLeft"], 30, 80),
    );
    let b = write(
        &dir,
        "b.edrf",
        &recorded(RecordingKind::Edrf, &["ArrowRight"], 30, 60),
    );
    let mut h = playing(&[a.clone(), b.clone()], false, None);
    // The list plays before the title screen, in the order given.
    let mut order: Vec<PathBuf> = Vec::new();
    let mut frames = 0;
    while !h.on_title() {
        let shown = source(&h).unwrap();
        if order.last() != Some(&shown) {
            order.push(shown);
        }
        h.frame();
        frames += 1;
        assert!(frames < 20_000);
    }
    assert_eq!(order, [a, b]);
    // The sound stops between the two and at the end.
    let stops = h
        .frames
        .iter()
        .filter(|frame| frame.sound.contains(&digger_core::SoundCommand::StopAll))
        .count();
    assert!(stops >= 2, "{stops} stops");
    // And the title stays.
    h.run(20);
    assert!(h.on_title());
}

#[test]
fn playback_loop_until_quit() {
    let dir = TempDir::new("loop");
    let a = write(
        &dir,
        "a.drf",
        &recorded(RecordingKind::Drf, &["ArrowLeft"], 20, 40),
    );
    let mut h = playing(&[a], true, None);
    // The recording restarts each time it ends: the tick count drops back.
    let mut restarts = 0;
    let mut last = 0;
    let mut frames = 0;
    while restarts < 3 {
        h.frame();
        frames += 1;
        assert!(frames < 20_000);
        assert!(!h.on_title(), "the loop never shows the title on its own");
        let ticks = h.game().state().ticks();
        if ticks < last {
            restarts += 1;
        }
        last = ticks;
    }
    // The quit action ends the loop and shows the title screen (compat
    // quirk SH11), from where the next quit ends the program.
    h.run(10);
    h.tap("F10");
    h.frame();
    assert!(h.on_title(), "quit ends the loop");
    h.run(300);
    assert!(h.on_title(), "the list does not come back");
    h.tap("F10");
    assert_eq!(h.frame().requests, [Request::Exit(0)]);

    // Closing the window ends the program at once.
    let mut h = playing(
        &[write(
            &dir,
            "b.drf",
            &recorded(RecordingKind::Drf, &["ArrowLeft"], 20, 40),
        )],
        true,
        None,
    );
    h.run(30);
    h.close();
    assert_eq!(h.frame().requests, [Request::Exit(0)]);
}

#[test]
fn playback_bad_file_skipped_with_notice() {
    let dir = TempDir::new("bad-file");
    let text = write(&dir, "notes.txt", "shopping list\nmilk\n");
    let missing = dir.path().join("missing.drf");
    let good = write(
        &dir,
        "good.drf",
        &recorded(RecordingKind::Drf, &["ArrowLeft"], 20, 40),
    );
    let mut h = playing(&[text, missing, good.clone()], true, None);
    // The two bad files are reported and skipped; the good one plays.
    assert_eq!(source(&h), Some(good));
    let notices = h.shell.playback_notices().to_vec();
    assert_eq!(notices.len(), 2, "{notices:?}");
    assert!(notices[0].contains("notes.txt"), "{notices:?}");
    assert!(notices[0].contains("not a recording"), "{notices:?}");
    assert!(notices[1].contains("missing.drf"), "{notices:?}");
    // Further laps skip them without a second notice.
    let mut restarts = 0;
    let mut last = 0;
    let mut frames = 0;
    while restarts < 2 {
        h.frame();
        frames += 1;
        assert!(frames < 20_000);
        let ticks = h.game().state().ticks();
        if ticks < last {
            restarts += 1;
        }
        last = ticks;
    }
    assert_eq!(h.shell.playback_notices().len(), 2);

    // A list of nothing playable goes straight to the title, loop or not.
    let only_bad = playing(&[dir.path().join("nothing.edrf")], true, None);
    assert!(only_bad.on_title());
}

#[test]
fn take_over_switches_to_live_controls_after_tick() {
    const TAKE_OVER: u32 = 30;
    let dir = TempDir::new("take-over");
    let file = write(
        &dir,
        "game.drf",
        &recorded(RecordingKind::Drf, &["ArrowLeft"], 80, 120),
    );
    // The recording as it plays: Digger walks left.
    let mut reference = playing(std::slice::from_ref(&file), false, None);
    let mut recorded_states = Vec::new();
    while !reference.on_title() {
        reference.frame();
        if let Scene::Playback(playback) = reference.shell.scene()
            && playback.game().is_some()
        {
            recorded_states.push((snapshot(&reference), digger_x(&reference)));
        }
    }
    // Taken over after tick 30, with the right arrow held from the start.
    let mut h = playing(&[file], false, Some(TAKE_OVER));
    h.press("ArrowRight");
    let mut x_at_take_over = None;
    let mut frames = 0;
    loop {
        h.frame();
        let (ticks, hash) = snapshot(&h);
        if ticks <= TAKE_OVER {
            // Up to the take-over tick the recording drives the game, and
            // the held key changes nothing.
            assert!(
                recorded_states
                    .iter()
                    .any(|&(state, _)| state == (ticks, hash)),
                "tick {ticks} follows the recording"
            );
            if ticks == TAKE_OVER {
                x_at_take_over = Some(digger_x(&h));
            }
        } else {
            let Scene::Playback(playback) = h.shell.scene() else {
                panic!("still the playback");
            };
            assert!(playback.taken_over(), "tick {ticks} is the player's");
        }
        if ticks >= TAKE_OVER + 20 {
            break;
        }
        frames += 1;
        assert!(frames < 2_000);
    }
    // From tick 31 the arrow key moves Digger right, where the recording
    // walked it left.
    let x0 = x_at_take_over.unwrap();
    let recorded_x = recorded_states
        .iter()
        .find(|&&((ticks, _), _)| ticks == TAKE_OVER + 20)
        .map(|&(_, x)| x)
        .unwrap();
    assert!(recorded_x < x0, "the recording walks left");
    assert!(digger_x(&h) > x0, "the player walks right");
    // Losing focus now pauses the game, as in live play.
    h.lose_focus();
    h.run(3);
    assert!(h.game().is_paused());
}

#[test]
fn take_over_recording_replays_whole_game() {
    for kind in [RecordingKind::Drf, RecordingKind::Edrf] {
        let dir = TempDir::new("take-over-record");
        let file = write(
            &dir,
            "game.drf",
            &recorded(RecordingKind::Drf, &["ArrowLeft"], 80, 120),
        );
        let out = dir.path().join(match kind {
            RecordingKind::Drf => "out.drf",
            RecordingKind::Edrf => "out.edrf",
        });
        let setup = Setup {
            play: vec![file],
            take_over_at: Some(30),
            record_kind: kind,
            record_path: Some(out.clone()),
            ..Setup::default()
        };
        let mut h = Harness::with(Settings::default(), setup);
        // The player takes over and walks right, then quits.
        h.press("ArrowRight");
        let mut frames = 0;
        while h.game().state().ticks() < 60 {
            h.frame();
            frames += 1;
            assert!(frames < 2_000);
        }
        let ticks = h.game().state().ticks();
        let x = digger_x(&h);
        h.release("ArrowRight");
        h.tap("F10");
        h.frame();
        assert!(h.on_title(), "{kind:?}");
        // The `--record` file holds both parts and replays to the same
        // game: the recorded ticks first, then the player's.
        let text = fs::read(&out).unwrap();
        let report = digger_core::replay::verify(&text, ReplayOptions::default()).unwrap();
        assert_eq!(report.exit_status(), 0, "{kind:?}: {report:?}");
        let extra = u32::from(kind == RecordingKind::Drf);
        assert_eq!(report.ticks, ticks + extra, "{kind:?}");
        if kind == RecordingKind::Edrf {
            assert_eq!(report.outcome, Outcome::Verified);
        }
        // Watching the new recording shows the player's walk.
        let mut replay = playing(&[out], false, None);
        while replay.game().state().ticks() < ticks {
            replay.frame();
        }
        assert_eq!(digger_x(&replay), x, "{kind:?}");
        assert!(h.shell.last_game().is_none(), "written to --record");
    }
}

/// A harness that verifies `text` in the window, re-recording to `record`.
fn verifying(text: &str, record: Option<&Path>) -> Harness {
    let recording = digger_core::recording::parse(text.as_bytes()).unwrap();
    let setup = Setup {
        verify: Some(crate::headless::VerifyJob {
            file: PathBuf::from("game"),
            recording,
        }),
        record_kind: record.map_or(RecordingKind::Drf, super::recording::kind_for_path),
        record_path: record.map(Path::to_path_buf),
        ..Setup::default()
    };
    Harness::with(Settings::default(), setup)
}

/// Runs a verification harness until it asks to exit; returns the status.
fn exit_status(h: &mut Harness) -> u8 {
    for _ in 0..20_000 {
        let requests = h.frame().requests.clone();
        if let Some(Request::Exit(code)) = requests.first() {
            return *code;
        }
    }
    panic!("the verification does not end");
}

#[test]
fn verify_in_window_statuses() {
    let dir = TempDir::new("verify");
    let text = recorded(RecordingKind::Drf, &["ArrowLeft"], 30, 80);
    // Played to its end: status 0 and the result line.
    let mut h = verifying(&text, None);
    assert_eq!(exit_status(&mut h), 0);
    let report = h.shell.verify_report().unwrap();
    assert_eq!(report.outcome, Outcome::Completed);
    assert!(report.result_line().starts_with("score="));
    // The last picture stays on screen for the exiting frame.
    let _ = h.shell.picture();
    // Arrow keys are ignored there too: the same report.
    let mut keys = verifying(&text, None);
    keys.press("ArrowRight");
    assert_eq!(exit_status(&mut keys), 0);
    assert_eq!(keys.shell.verify_report(), h.shell.verify_report());

    // Stopped by F10 halfway: status 3.
    let mut stopped = verifying(&text, None);
    stopped.run(40);
    stopped.tap("F10");
    assert_eq!(exit_status(&mut stopped), 3);
    assert_eq!(stopped.shell.verify_report().unwrap().exit_status(), 3);

    // With `--record`, the replayed game is written before the exit: a
    // classic recording converted to the extended format.
    let out = dir.path().join("game.edrf");
    let mut convert = verifying(&text, Some(&out));
    assert_eq!(exit_status(&mut convert), 0);
    let converted = fs::read(&out).unwrap();
    assert!(converted.starts_with(b"eDRF 2"));
    let report = digger_core::replay::verify(&converted, ReplayOptions::default()).unwrap();
    assert_eq!(report.outcome, Outcome::Verified);
    let last = h.shell.verify_report().unwrap();
    assert_eq!(report.scores, last.scores);
    assert_eq!(
        convert.shell.verify_report().unwrap().ticks,
        last.ticks,
        "the same game"
    );
    assert!(convert.shell.last_game().is_none());
}
