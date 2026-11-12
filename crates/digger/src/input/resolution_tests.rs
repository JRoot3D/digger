//! End-to-end resolution tests (task 5.5): scripted press and release
//! timelines go through [`InputState`] into the core's `Game`, and the
//! direction and fire each digger uses are read back through the
//! observer's movement hook.
//!
//! The direction resolver and the fire latch are the core's (design D3.1);
//! these tests prove that the frontend feeds them the raw states the
//! `input-controls` requirements assume. Each frame of a timeline is: the
//! events that happened since the previous frame, then `take_frame()`, then
//! one `Game` step, as the shell will do it.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]

use digger_core::testkit::{Run, Scenario};
use digger_core::{
    DiggerId, Direction, FrameInput, FrameKind, GameMode, Movement, Observer, PlayerId, SlotId,
    SoundCommand,
};

use super::{Bindings, InputState, KeyName, KeyPress};
use crate::config::Settings;

/// Records every movement sample.
#[derive(Debug, Default)]
struct Samples(Vec<(SlotId, Movement)>);

impl Observer for Samples {
    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement {
        self.0.push((slot, sample));
        sample
    }
}

/// One event between two frames.
#[derive(Debug, Clone, Copy)]
enum Ev {
    /// Key down.
    D(&'static str),
    /// Key up.
    U(&'static str),
}

/// Solid earth with one emerald, so nothing gets in the way of a few
/// ticks of movement.
const EARTH: [&str; 10] = [
    "              C",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
];

/// A game in `mode`, stepped to the first tick of its first round, and
/// the input state with the default bindings.
fn started(mode: GameMode, earth: bool) -> (Run<Samples>, InputState) {
    let scenario = Scenario::new(mode);
    let scenario = if earth {
        scenario.rows(EARTH)
    } else {
        scenario
    };
    let mut run = scenario.build_observed(Samples::default());
    run.start();
    let input = InputState::new(Bindings::from_settings(&Settings::default()));
    (run, input)
}

fn key(name: &str) -> KeyName {
    KeyName::parse(name).unwrap_or_else(|| panic!("{name} is a key"))
}

/// Applies `events`, takes the frame and steps the game once; returns the
/// movement samples of that step.
fn frame(run: &mut Run<Samples>, input: &mut InputState, events: &[Ev]) -> Vec<(SlotId, Movement)> {
    for event in events {
        match *event {
            Ev::D(name) => input.key_down(KeyPress {
                key: Some(key(name)),
                ..KeyPress::default()
            }),
            Ev::U(name) => input.key_up(key(name)),
        }
    }
    let taken = input.take_frame();
    let before = run.observer().0.len();
    let outcome = run.step(&FrameInput {
        slots: taken.slots,
        pause: false,
        quit: false,
    });
    assert_eq!(
        outcome.frame,
        FrameKind::Tick,
        "every frame of a round is a tick"
    );
    run.observer().0[before..].to_vec()
}

/// The first slot's direction on each of the frames of `timeline`.
fn directions(
    run: &mut Run<Samples>,
    input: &mut InputState,
    timeline: &[&[Ev]],
) -> Vec<Option<Direction>> {
    timeline
        .iter()
        .map(|events| {
            let samples = frame(run, input, events);
            let [(slot, sample)] = samples[..] else {
                panic!("one sample per tick, got {samples:?}");
            };
            assert_eq!(slot, SlotId::ALL[0]);
            sample.direction
        })
        .collect()
}

const R: Option<Direction> = Some(Direction::Right);
const UP: Option<Direction> = Some(Direction::Up);
const L: Option<Direction> = Some(Direction::Left);
const DN: Option<Direction> = Some(Direction::Down);

#[test]
fn input_resolution_last_pressed() {
    let (mut run, mut input) = started(GameMode::OnePlayer, true);
    // Hold right; three frames later also press and hold up.
    let got = directions(
        &mut run,
        &mut input,
        &[
            &[Ev::D("ArrowRight")],
            &[],
            &[],
            &[Ev::D("ArrowUp")],
            &[],
            &[],
        ],
    );
    assert_eq!(got, [R, R, R, UP, UP, UP]);
    // Releasing up falls back to the still-held right.
    let got = directions(&mut run, &mut input, &[&[Ev::U("ArrowUp")], &[]]);
    assert_eq!(got, [R, R]);
}

#[test]
fn input_resolution_same_frame() {
    let (mut run, mut input) = started(GameMode::OnePlayer, true);
    // Up and right go down between the same two frames: right.
    let got = directions(
        &mut run,
        &mut input,
        &[&[Ev::D("ArrowUp"), Ev::D("ArrowRight")], &[]],
    );
    assert_eq!(got, [R, R]);
    // All four at once: right beats left beats down beats up.
    let (mut run, mut input) = started(GameMode::OnePlayer, true);
    let got = directions(
        &mut run,
        &mut input,
        &[
            &[
                Ev::D("ArrowUp"),
                Ev::D("ArrowDown"),
                Ev::D("ArrowLeft"),
                Ev::D("ArrowRight"),
            ],
            &[Ev::U("ArrowRight")],
            &[Ev::U("ArrowLeft")],
            &[Ev::U("ArrowDown")],
        ],
    );
    assert_eq!(got, [R, L, DN, UP]);
    let (mut run, mut input) = started(GameMode::OnePlayer, true);
    let got = directions(
        &mut run,
        &mut input,
        &[&[Ev::D("ArrowUp"), Ev::D("ArrowDown")], &[]],
    );
    assert_eq!(got, [DN, DN]);
}

#[test]
fn input_resolution_release_fallback() {
    let (mut run, mut input) = started(GameMode::OnePlayer, true);
    // Down, left, up pressed in that order; releasing up gives left.
    let got = directions(
        &mut run,
        &mut input,
        &[
            &[Ev::D("ArrowDown")],
            &[Ev::D("ArrowLeft")],
            &[Ev::D("ArrowUp")],
            &[Ev::U("ArrowUp")],
            &[],
        ],
    );
    assert_eq!(got, [DN, L, UP, L, L]);
    // Releasing a key that is not the active one changes nothing.
    let (mut run, mut input) = started(GameMode::OnePlayer, true);
    let got = directions(
        &mut run,
        &mut input,
        &[
            &[Ev::D("ArrowLeft")],
            &[Ev::D("ArrowUp")],
            &[Ev::U("ArrowLeft")],
            &[],
            &[Ev::U("ArrowUp")],
            &[],
        ],
    );
    assert_eq!(got, [L, UP, UP, UP, None, None]);
}

#[test]
fn input_tap_one_tick() {
    // A 20 ms tap with nothing held: down for exactly one tick.
    let (mut run, mut input) = started(GameMode::OnePlayer, true);
    let got = directions(
        &mut run,
        &mut input,
        &[&[], &[Ev::D("ArrowDown"), Ev::U("ArrowDown")], &[], &[]],
    );
    assert_eq!(got, [None, DN, None, None]);
    // A tap of up while right is held: up for one tick, then right again.
    let (mut run, mut input) = started(GameMode::OnePlayer, true);
    let got = directions(
        &mut run,
        &mut input,
        &[
            &[Ev::D("ArrowRight")],
            &[],
            &[Ev::D("ArrowUp"), Ev::U("ArrowUp")],
            &[],
            &[],
        ],
    );
    assert_eq!(got, [R, R, UP, R, R]);
}

#[test]
fn fire_takes_effect_next_tick() {
    let (mut run, mut input) = started(GameMode::OnePlayer, false);
    let first = DiggerId::ALL[0];
    run.take_sounds();
    // A 10 ms tap on F1 between two frames.
    let samples = frame(&mut run, &mut input, &[Ev::D("F1"), Ev::U("F1")]);
    assert!(samples[0].1.fire, "the tap is sampled on this tick");
    assert!(
        run.state().digger(first).not_firing(),
        "but does not fire on it"
    );
    let samples = frame(&mut run, &mut input, &[]);
    assert!(!samples[0].1.fire, "seen once");
    assert!(
        !run.state().digger(first).not_firing(),
        "launched on the next tick"
    );
    for _ in 0..30 {
        frame(&mut run, &mut input, &[]);
    }
    let fires = run
        .take_sounds()
        .into_iter()
        .filter(|sound| matches!(sound, SoundCommand::Fire(_)))
        .count();
    assert_eq!(fires, 1, "exactly one fireball");
    // Fire held with auto-repeat still launches once per recharge, and a
    // repeat event is no new press.
    input.key_down(KeyPress {
        key: Some(key("F1")),
        ..KeyPress::default()
    });
    for _ in 0..5 {
        input.key_down(KeyPress {
            key: Some(key("F1")),
            repeat: true,
            text: None,
        });
        let samples = frame(&mut run, &mut input, &[]);
        assert!(samples[0].1.fire);
    }
}

#[test]
fn alternating_uses_p1_controls() {
    let (mut run, mut input) = started(GameMode::TwoPlayers, true);
    // Player 1 plays with the arrow keys; player 2's keys do nothing.
    let got = directions(
        &mut run,
        &mut input,
        &[&[Ev::D("KeyS")], &[Ev::D("ArrowUp")]],
    );
    assert_eq!(got, [None, UP]);
    for event in [Ev::U("KeyS"), Ev::U("ArrowUp")] {
        if let Ev::U(name) = event {
            input.key_up(key(name));
        }
    }
    // Player 1 loses a life: the turn passes to player 2.
    run.kill_digger(DiggerId::ALL[0], digger_core::testkit::KillCause::Monster)
        .expect("in play");
    let mut frames = 0;
    while run.state().current_player() != PlayerId::ALL[1] {
        let taken = input.take_frame();
        run.step(&FrameInput {
            slots: taken.slots,
            ..FrameInput::NONE
        });
        frames += 1;
        assert!(frames < 10_000, "player 2's turn never came");
    }
    run.start();
    assert_eq!(run.state().current_player(), PlayerId::ALL[1]);
    // Player 2's turn: the arrow keys and F1 drive the digger, through
    // slot 1; player 2's own keys still do nothing.
    let got = directions(
        &mut run,
        &mut input,
        &[
            &[Ev::D("KeyA"), Ev::D("KeyW")],
            &[Ev::D("ArrowLeft")],
            &[Ev::U("ArrowLeft"), Ev::D("ArrowDown")],
        ],
    );
    assert_eq!(got, [None, L, DN]);
    let samples = frame(&mut run, &mut input, &[Ev::D("Tab")]);
    assert!(!samples[0].1.fire, "player 2's fire key does nothing");
    let samples = frame(&mut run, &mut input, &[Ev::D("F1")]);
    assert!(samples[0].1.fire, "F1 fires for player 2");
}
