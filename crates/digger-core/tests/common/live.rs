//! A scripted live game recorded as DRF and as eDRF at once, shared by
//! the codec round trips and the port-made fixtures.
//!
//! The script presses fire only on ticks on which the slot's digger takes
//! a movement sample: a fire pressed on another tick takes effect without
//! being part of any DRF record (quirk Q15), so no DRF can hold it.

use digger_core::replay::{DrfRecorder, EdrfRecorder};
use digger_core::{
    Controls, DiggerId, FrameInput, Game, GameConfig, GameSummary, LevelLayout, LevelNumber,
    Movement, Observer, PlayerId, SlotId, StateView,
};

use crate::inputs::InputGen;

/// The most frames a game may take.
const MAX_FRAMES: usize = 100_000;

/// Both recorders, told every hook.
struct Both {
    drf: DrfRecorder,
    edrf: EdrfRecorder,
}

impl Observer for Both {
    fn round_seed(&mut self, round: u32, derived: i32) -> i32 {
        self.drf.round_seed(round, derived);
        self.edrf.round_seed(round, derived)
    }

    fn level_plan(
        &mut self,
        player: PlayerId,
        level: LevelNumber,
        layout: &LevelLayout,
    ) -> Option<LevelLayout> {
        self.drf.level_plan(player, level, layout);
        self.edrf.level_plan(player, level, layout)
    }

    fn controls(&mut self, tick: u32, slot: SlotId, raw: Controls) -> Controls {
        self.drf.controls(tick, slot, raw);
        self.edrf.controls(tick, slot, raw)
    }

    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement {
        self.drf.movement(slot, sample);
        self.edrf.movement(slot, sample)
    }

    fn tick_end(&mut self, tick: u32, state: StateView<'_>) {
        self.drf.tick_end(tick, state);
        self.edrf.tick_end(tick, state);
    }

    fn round_end(&mut self, state: StateView<'_>) {
        self.drf.round_end(state);
        self.edrf.round_end(state);
    }

    fn game_end(&mut self, state: StateView<'_>) {
        self.drf.game_end(state);
        self.edrf.game_end(state);
    }
}

/// Whether the digger of control slot `slot` takes a movement sample on
/// the next tick.
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

/// A live game under `config` played to its end with generated controls;
/// its summary and its DRF and eDRF recordings.
#[allow(dead_code, reason = "not every test binary uses every helper")]
pub(crate) fn live(config: &GameConfig, seed: u64) -> (GameSummary, String, String) {
    let mut generator = InputGen::new(seed);
    live_with(config, seed, |_| generator.controls())
}

/// A live game under `config` played to its end with the controls
/// `script` gives for each frame, seen the state before the frame; its
/// summary and its DRF and eDRF recordings. Fire is dropped on frames on
/// which the slot's digger takes no movement sample.
pub(crate) fn live_with(
    config: &GameConfig,
    seed: u64,
    mut script: impl FnMut(StateView<'_>) -> FrameInput,
) -> (GameSummary, String, String) {
    let mut game = Game::new(config.clone(), seed);
    let mut both = Both {
        drf: DrfRecorder::new(config),
        edrf: EdrfRecorder::new(config),
    };
    let mut edrf = String::new();
    for _ in 0..MAX_FRAMES {
        let state = game.state();
        let mut input = script(state);
        for (slot, controls) in input.slots.iter_mut().enumerate() {
            controls.fire &= will_sample(state, slot);
        }
        let finished = game.step_observed(&input, &mut both).finished;
        // Streamed as a frontend would write it to disk.
        edrf.push_str(&both.edrf.take_pending());
        if let Some(summary) = finished {
            return (summary, both.drf.text(), edrf);
        }
    }
    panic!("the game did not finish in {MAX_FRAMES} frames");
}
