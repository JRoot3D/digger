//! The observer hook through which a caller observes or replaces round
//! seeds, level layouts, controls and movement samples (design D8).
//!
//! Recording, playback and checkpoint verification are built on these hooks
//! without the simulation knowing about files (game-flow spec, "Externally
//! supplied seeds and samples"; research 01 §7.2, research 05 §1.6, §4.3,
//! §5.3).

use super::ids::{PlayerId, SlotId};
use super::input::{Controls, Movement};
use super::level::{LevelLayout, LevelNumber};
use super::state::StateView;

/// Hooks called by the game at the exact points where the original
/// consumed or produced the values a recording holds.
///
/// Every method has a default that observes nothing and replaces nothing,
/// so an implementation overrides only what it needs. Hooks that return a
/// value may replace the value they were given; returning the argument
/// unchanged leaves the game running exactly as without an observer.
pub trait Observer {
    /// Called once at every round start, with the round counter (0 for the
    /// first round of the game) and the seed derived from the base seed
    /// for that round. The returned value becomes the random-number
    /// generator's state for the round.
    fn round_seed(&mut self, round: u32, derived: i32) -> i32 {
        let _ = round;
        derived
    }

    /// Called once each time a level is set up for a participating player:
    /// at game start for every player who plays, and whenever a player
    /// advances to a new level. Never called for the placeholder level 0
    /// state of the second player in a non-alternating game (Q06).
    ///
    /// Returning `Some` replaces the level's layout; a returned layout is
    /// used verbatim, without the two-digger start-cell substitution (Q07).
    fn level_plan(
        &mut self,
        player: PlayerId,
        level: LevelNumber,
        layout: &LevelLayout,
    ) -> Option<LevelLayout> {
        let _ = (player, level, layout);
        None
    }

    /// Called once per control slot on every tick, before the slot's
    /// direction is resolved, whether or not that slot's digger reads its
    /// controls on the tick. `tick` is the number of the tick being run.
    /// The returned value is resolved instead of `raw`.
    fn controls(&mut self, tick: u32, slot: SlotId, raw: Controls) -> Controls {
        let _ = (tick, slot);
        raw
    }

    /// Called exactly once per movement sample, that is on each tick in
    /// which a digger is alive, visible and not in a push pause (Q15), with
    /// the slot's resolved direction and pending fire state. The returned
    /// direction is the one the digger uses on this tick; the returned fire
    /// state replaces the pending one, which takes effect on the next tick.
    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement {
        let _ = slot;
        sample
    }

    /// Called at the end of every tick of the main phase and the
    /// wind-down, after everything of the tick has happened, with the tick
    /// count including that tick.
    fn tick_end(&mut self, tick: u32, state: StateView<'_>) {
        let _ = (tick, state);
    }

    /// Called once at every round end, after the round-end transition has
    /// tidied and saved the bags, saved the field and removed the
    /// monsters, and before the outcome moves to the next phase.
    fn round_end(&mut self, state: StateView<'_>) {
        let _ = state;
    }

    /// Called once when the game finishes, for any reason, with the final
    /// state.
    fn game_end(&mut self, state: StateView<'_>) {
        let _ = state;
    }
}

/// The observer that observes nothing and replaces nothing; the game's
/// plain step uses it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct NoopObserver;

impl Observer for NoopObserver {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Direction, LevelSet};

    #[test]
    fn noop_observer_returns_inputs_unchanged() {
        let mut observer = NoopObserver;
        let obs: &mut dyn Observer = &mut observer;
        assert_eq!(obs.round_seed(0, 0x7B1D_CDAF), 0x7B1D_CDAF);
        assert_eq!(obs.round_seed(7, -5), -5);

        let levels = LevelSet::builtin();
        let level = LevelNumber::new(1).unwrap_or_else(|| unreachable!());
        let layout = levels.layout(level.plan());
        assert_eq!(obs.level_plan(PlayerId::ALL[1], level, layout), None);

        let raw = Controls {
            left: true,
            fire: true,
            ..Controls::NONE
        };
        for slot in SlotId::ALL {
            assert_eq!(obs.controls(42, slot, raw), raw);
        }
        let sample = Movement {
            direction: Some(Direction::Down),
            fire: true,
        };
        assert_eq!(obs.movement(SlotId::ALL[0], sample), sample);
        assert_eq!(
            obs.movement(SlotId::ALL[1], Movement::default()),
            Movement::default()
        );

        let game = crate::sim::Game::new(crate::sim::GameConfig::default(), 0);
        obs.tick_end(1, game.state());
        obs.round_end(game.state());
        obs.game_end(game.state());
    }
}
