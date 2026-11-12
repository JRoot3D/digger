//! Taking a game over from a playback (task 9.1 of
//! `add-recording-and-conformance`; game-recording spec, "Taking over
//! from a playback"; design D8).
//!
//! Upstream recordings of the corpus are played back and handed over to
//! generated live controls after a chosen tick. The tests check where the
//! controls come from on each side of the hand-over, that a game recorded
//! across the hand-over replays to the same end, and that a taken-over
//! game is marked as such and never eligible for the high-score table.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

#[path = "common/inputs.rs"]
mod inputs;

use std::fs;
use std::path::Path;

use digger_core::recording::{Recording, RecordingKind, parse};
use digger_core::replay::{Outcome, Replay, ReplayOptions, ReplayReport, ReplayStep, verify};
use digger_core::{Controls, DiggerId, FrameInput, NoopObserver, Observer, SlotId, StateView};

use inputs::InputGen;

/// The tick after which the tests hand over.
const HAND_OVER: u32 = 500;

/// The most live frames a test plays before it quits the game.
const LIVE_FRAMES: u32 = 4000;

fn corpus(name: &str) -> Recording {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/recordings/data")
        .join(name);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn options() -> ReplayOptions {
    ReplayOptions {
        take_over_after: Some(HAND_OVER),
        ..ReplayOptions::default()
    }
}

/// The controls each slot was given on each tick, numbered from 1.
#[derive(Default)]
struct Spy {
    ticks_done: u32,
    used: Vec<(u32, SlotId, Controls)>,
}

impl Observer for Spy {
    fn controls(&mut self, _tick: u32, slot: SlotId, raw: Controls) -> Controls {
        self.used.push((self.ticks_done + 1, slot, raw));
        raw
    }

    fn tick_end(&mut self, tick: u32, _state: StateView<'_>) {
        self.ticks_done = tick;
    }
}

/// Whether the digger of control slot `slot` takes a movement sample on
/// the next tick. A fire pressed on another tick takes effect without
/// being part of any DRF record (quirk Q15), so the live controls press
/// fire only on such ticks when the game is recorded as DRF.
fn will_sample(state: StateView<'_>, slot: usize) -> bool {
    let index = if state.mode().alternating() {
        usize::from(state.current_player().number())
    } else {
        slot
    };
    let Some(&digger) = DiggerId::ALL.get(index) else {
        return false;
    };
    let digger = state.digger(digger);
    state.diggers_visible() && digger.alive() && digger.bag_time() == 0
}

/// Plays `replay` to its end with generated live controls (fire masked
/// as [`will_sample`] says when `mask_fire`), told to `spy`, quitting
/// after [`LIVE_FRAMES`] frames. The inputs given, by frame.
fn play(replay: &mut Replay, spy: &mut Spy, seed: u64, mask_fire: bool) -> Vec<FrameInput> {
    let mut generator = InputGen::new(seed);
    let mut inputs = Vec::new();
    for frame in 0.. {
        let mut input = generator.controls();
        if mask_fire {
            let state = replay.game().state();
            for (slot, controls) in input.slots.iter_mut().enumerate() {
                controls.fire &= will_sample(state, slot);
            }
        }
        input.quit = frame >= LIVE_FRAMES + HAND_OVER * 2;
        inputs.push(input);
        if let ReplayStep::Finished(_) = replay.step(&input, spy) {
            break;
        }
    }
    inputs
}

/// The controls each tick of a plain playback of `recording` used.
fn plain(recording: &Recording) -> Vec<(u32, SlotId, Controls)> {
    let mut replay = Replay::new(recording, ReplayOptions::default()).expect("playable");
    let mut spy = Spy::default();
    while let ReplayStep::Running(_) = replay.step(&FrameInput::NONE, &mut spy) {}
    spy.used
}

#[test]
fn takeover_switches_after_tick() {
    for name in ["sobodig.edrf", "sobodig.drf"] {
        let recording = corpus(name);
        let recorded = plain(&recording);
        let mut replay = Replay::new(&recording, options()).expect("playable");
        let mut spy = Spy::default();
        let mut generator = InputGen::new(7);
        let mut live_ticks = 0_u32;
        let mut frame = 0_u32;
        let report = loop {
            let input = FrameInput {
                quit: frame >= LIVE_FRAMES,
                ..generator.controls()
            };
            frame += 1;
            let before = spy.used.len();
            let ticks = replay.game().state().ticks();
            let step = replay.step(&input, &mut spy);
            for &(tick, slot, used) in spy.used.get(before..).unwrap_or_default() {
                if tick <= HAND_OVER {
                    // Ticks 1 to 500 use the recorded controls.
                    let expected = recorded
                        .iter()
                        .find(|&&(t, s, _)| t == tick && s == slot)
                        .map(|&(_, _, c)| c);
                    assert_eq!(Some(used), expected, "{name}: tick {tick} slot {slot:?}");
                    assert!(ticks < HAND_OVER, "{name}: tick {tick} after the hand-over");
                } else {
                    // Tick 501 onward use the live controls.
                    assert_eq!(
                        used,
                        input.slots.get(slot.index()).copied().unwrap_or_default(),
                        "{name}: tick {tick} slot {slot:?}"
                    );
                    live_ticks += 1;
                }
            }
            if let ReplayStep::Finished(_) = step {
                break replay.finish();
            }
        };
        assert!(live_ticks > 100, "{name}: only {live_ticks} live ticks");
        assert_eq!(report.outcome, Outcome::TakenOver, "{name}");
        assert!(report.ticks > HAND_OVER, "{name}: {report:?}");
        assert_eq!(report.exit_status(), 0, "{name}");
        assert!(report.diagnostics(name).is_empty(), "{name}");
    }
}

#[test]
fn takeover_request_hands_over_between_frames() {
    let recording = corpus("sobodig.edrf");
    let mut replay = Replay::new(&recording, ReplayOptions::default()).expect("playable");
    for _ in 0..50 {
        replay.step(&FrameInput::NONE, &mut NoopObserver);
    }
    assert!(!replay.taken_over());
    assert!(replay.request_take_over());
    assert!(replay.taken_over());
    assert!(replay.request_take_over(), "a second request is no error");
    let quit = FrameInput {
        quit: true,
        ..FrameInput::NONE
    };
    replay.step(&quit, &mut NoopObserver);
    let report = replay.finish();
    assert_eq!(report.outcome, Outcome::TakenOver);

    // A finished playback has nothing left to take over.
    let mut replay = Replay::new(&recording, ReplayOptions::default()).expect("playable");
    while let ReplayStep::Running(_) = replay.step(&FrameInput::NONE, &mut NoopObserver) {}
    assert!(!replay.request_take_over());
    assert_eq!(replay.finish().outcome, Outcome::Verified);
}

/// Whether `again` ends as `taken` did.
fn assert_same_end(again: &ReplayReport, taken: &ReplayReport, what: &str) {
    assert_eq!(again.scores, taken.scores, "{what}: scores");
    assert_eq!(again.level, taken.level, "{what}: level");
    assert_eq!(again.ticks, taken.ticks, "{what}: ticks");
}

#[test]
fn takeover_rerecorded_game_replays() {
    let cases = [
        ("sobodig.edrf", RecordingKind::Edrf, Outcome::Verified),
        ("sobodig.drf", RecordingKind::Edrf, Outcome::Verified),
        ("sobodig.drf", RecordingKind::Drf, Outcome::Completed),
        ("2test01.edrf", RecordingKind::Edrf, Outcome::Verified),
        ("2test01.drf", RecordingKind::Drf, Outcome::Completed),
    ];
    for (seed, (name, kind, replayed)) in (1_u64..).zip(cases) {
        let what = format!("{name} as {kind:?}");
        let recording = corpus(name);
        let mut replay = Replay::recording_as(&recording, options(), kind).expect("playable");
        let mut spy = Spy::default();
        play(&mut replay, &mut spy, seed, kind == RecordingKind::Drf);
        let (taken, text) = replay.finish_with_recording();
        let text = text.expect("a recorder");
        assert_eq!(taken.outcome, Outcome::TakenOver, "{what}");
        assert!(taken.ticks > HAND_OVER + 100, "{what}: {taken:?}");

        // The new recording reproduces the whole game: the played-back
        // part followed by the live part.
        let again = verify(text.as_bytes(), ReplayOptions::default()).expect("playable");
        assert_eq!(again.outcome, replayed, "{what}: {again:?}");
        assert_same_end(&again, &taken, &what);

        // Its first 500 ticks are the original recording's.
        let original = plain(&recording);
        let copy = plain(&parse(text.as_bytes()).expect("a recording"));
        let head = |used: &[(u32, SlotId, Controls)]| {
            used.iter()
                .filter(|&&(tick, _, _)| tick <= HAND_OVER)
                .copied()
                .collect::<Vec<_>>()
        };
        if kind == recording.kind() {
            assert_eq!(head(&copy), head(&original), "{what}");
        }

        // An eDRF holds the live raw controls from tick 501 on, also one
        // converted from a DRF playback (every case has one digger, in
        // slot 0).
        if kind == RecordingKind::Edrf {
            let slot0 = SlotId::new(0).expect("slot 0");
            let tail = |used: &[(u32, SlotId, Controls)]| {
                used.iter()
                    .filter(|&&(tick, slot, _)| tick > HAND_OVER && slot == slot0)
                    .copied()
                    .collect::<Vec<_>>()
            };
            assert_eq!(tail(&copy), tail(&spy.used), "{what}");
        }
    }
}

#[test]
fn takeover_not_eligible_for_high_score() {
    let recording = corpus("sobodig.edrf");
    let mut replay = Replay::new(&recording, options()).expect("playable");
    play(&mut replay, &mut Spy::default(), 3, false);
    let report = replay.finish();
    assert!(report.scores[0] > 0, "{report:?}");
    assert_eq!(report.outcome, Outcome::TakenOver);
    assert!(!report.high_score_eligible());

    // Nor is a playback that was not taken over.
    let report = verify(
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/recordings/data/sobodig.edrf"),
        )
        .expect("the corpus")
        .as_slice(),
        ReplayOptions::default(),
    )
    .expect("playable");
    assert_eq!(report.outcome, Outcome::Verified);
    assert!(!report.high_score_eligible());
}
