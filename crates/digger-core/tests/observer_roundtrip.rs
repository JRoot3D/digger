//! Observer round trip (game-flow spec, "Externally supplied seeds and
//! samples", scenario "Re-running from observed values"; design D8; task
//! 15.2).
//!
//! A live scripted game in every mode is run with a recording observer
//! that keeps each round seed and each movement sample as the game
//! consumes them. A second game with the same configuration and base seed
//! 0 is then fed those values through a playback observer, with empty
//! controls on every frame, and must end exactly as the first did.
//!
//! The live script holds fire only on ticks on which the slot's digger
//! takes a movement sample. A fire control held on a tick without a
//! sample (a push pause, a dead digger) is still read and takes effect on
//! the next tick, but it is part of no sample, so a replay fed only seeds
//! and samples cannot see it — as in the original, whose DRF playback
//! reads it from the live controls (quirk Q15,
//! `quirk_q15_fire_pressed_during_push_pause_fires_without_a_sample`).

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

#[path = "common/inputs.rs"]
mod inputs;

use digger_core::{
    DiggerId, FrameInput, Game, GameConfig, GameMode, GameSummary, GauntletSeconds, Movement,
    Observer, SlotId, StateView,
};

use inputs::InputGen;

/// The most frames a game may take; every scripted game finishes well
/// before.
const MAX_FRAMES: usize = 6000;

/// Records the values a recording file would hold.
#[derive(Debug, Default)]
struct Recorder {
    seeds: Vec<(u32, i32)>,
    samples: Vec<(SlotId, Movement)>,
    traces: Vec<String>,
}

impl Observer for Recorder {
    fn round_seed(&mut self, round: u32, derived: i32) -> i32 {
        self.seeds.push((round, derived));
        derived
    }

    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement {
        self.samples.push((slot, sample));
        sample
    }

    fn tick_end(&mut self, _tick: u32, state: StateView<'_>) {
        self.traces.push(trace_of(state));
    }
}

/// Feeds recorded values back in place of the derived seeds and the
/// resolved controls.
#[derive(Debug)]
struct Player<'a> {
    seeds: &'a [(u32, i32)],
    samples: std::slice::Iter<'a, (SlotId, Movement)>,
    seeds_used: usize,
    traces: Vec<String>,
}

impl Observer for Player<'_> {
    fn round_seed(&mut self, round: u32, derived: i32) -> i32 {
        let &(recorded_round, seed) = self
            .seeds
            .get(self.seeds_used)
            .expect("no more recorded rounds");
        assert_eq!(round, recorded_round, "rounds out of step");
        self.seeds_used += 1;
        // The seeds come from the recording; the playback game's own base
        // seed differs, so its derived seed must not be what is used.
        let _ = derived;
        seed
    }

    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement {
        let &(recorded_slot, recorded) = self.samples.next().expect("no more recorded samples");
        assert_eq!(slot, recorded_slot, "movement samples out of step");
        // With empty controls the live sample has no direction and no fire.
        assert_eq!(sample.direction, None);
        recorded
    }

    fn tick_end(&mut self, _tick: u32, state: StateView<'_>) {
        self.traces.push(trace_of(state));
    }
}

fn trace(game: &Game) -> String {
    trace_of(game.state())
}

fn trace_of(state: StateView<'_>) -> String {
    let mut line = String::new();
    state
        .trace_line(&mut line)
        .unwrap_or_else(|_| unreachable!("writing to a String cannot fail"));
    line
}

/// Whether the digger driven by control slot `slot` will take a movement
/// sample on the next tick: the diggers are shown and it is alive and not
/// in a push pause (quirk Q15).
fn will_sample(state: StateView<'_>, slot: usize) -> bool {
    let record = if state.mode().alternating() {
        usize::from(state.current_player().number())
    } else {
        slot
    };
    let digger = state.digger(*DiggerId::ALL.get(record).expect("a digger record"));
    state.diggers_visible() && digger.alive() && digger.bag_time() == 0
}

/// Steps `game` with `input` for each frame until it finishes; returns the
/// summary and the number of frames.
fn finish(
    game: &mut Game,
    obs: &mut dyn Observer,
    mut input: impl FnMut(StateView<'_>) -> FrameInput,
) -> (GameSummary, usize) {
    for frame in 1..=MAX_FRAMES {
        if let Some(summary) = game.step_observed(&input(game.state()), obs).finished {
            return (summary, frame);
        }
    }
    panic!("the game did not finish in {MAX_FRAMES} frames");
}

fn round_trip(mode: GameMode, seed: u64) {
    let config = GameConfig {
        mode,
        ..GameConfig::default()
    };

    let mut live = Game::new(config.clone(), seed);
    let mut recorder = Recorder::default();
    let mut generator = InputGen::new(seed);
    let (live_summary, live_frames) = finish(&mut live, &mut recorder, |state| {
        let mut input = generator.controls();
        for (slot, controls) in input.slots.iter_mut().enumerate() {
            controls.fire &= will_sample(state, slot);
        }
        input
    });
    assert!(live_summary.ticks > 500, "{mode:?}: a short game");
    assert!(recorder.seeds.len() > 1, "{mode:?}: only one round");
    assert!(
        recorder
            .samples
            .iter()
            .any(|(_, sample)| sample.direction.is_some() && sample.fire),
        "{mode:?}: no sample moves and fires"
    );

    // A different base seed: everything random must come from the
    // recording.
    let mut replay = Game::new(config, seed ^ 0xDEAD_BEEF);
    let mut player = Player {
        seeds: &recorder.seeds,
        samples: recorder.samples.iter(),
        seeds_used: 0,
        traces: Vec::new(),
    };
    let (replay_summary, replay_frames) = finish(&mut replay, &mut player, |_| FrameInput::NONE);

    assert_eq!(replay_summary, live_summary, "{mode:?}: summaries");
    assert_eq!(trace(&replay), trace(&live), "{mode:?}: final trace lines");
    assert_eq!(replay_frames, live_frames, "{mode:?}: frame counts");
    // Every tick ended in the same state, not just the last.
    assert_eq!(
        player.traces.len(),
        recorder.traces.len(),
        "{mode:?}: tick counts"
    );
    for (tick, (replayed, recorded)) in player.traces.iter().zip(&recorder.traces).enumerate() {
        assert_eq!(replayed, recorded, "{mode:?}: tick {} differs", tick + 1);
    }
    assert_eq!(
        player.seeds_used,
        recorder.seeds.len(),
        "{mode:?}: seeds left"
    );
    assert_eq!(player.samples.len(), 0, "{mode:?}: samples left");
}

#[test]
fn re_running_from_observed_values_one_player() {
    round_trip(GameMode::OnePlayer, 11);
}

#[test]
fn re_running_from_observed_values_two_players() {
    round_trip(GameMode::TwoPlayers, 12);
}

#[test]
fn re_running_from_observed_values_two_diggers() {
    round_trip(GameMode::TwoDiggers, 13);
}

#[test]
fn re_running_from_observed_values_gauntlet() {
    round_trip(
        GameMode::Gauntlet {
            time: GauntletSeconds::new(90),
        },
        14,
    );
}

#[test]
fn re_running_from_observed_values_gauntlet_two_diggers() {
    round_trip(
        GameMode::GauntletTwoDiggers {
            time: GauntletSeconds::new(90),
        },
        15,
    );
}
