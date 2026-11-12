//! The playback sources and [`ReplayObserver`], the one [`Observer`] that
//! playback, recording, verification and tracing are composed in (design
//! D4 and D8 of `add-recording-and-conformance`).

use crate::{Controls, LevelLayout, LevelNumber, Movement, Observer, PlayerId, SlotId, StateView};

use super::recorder::Recorder;
use super::trace::TraceSink;
use super::verifier::Verifier;

/// Where the values of a played-back game come from: a DRF or an eDRF
/// recording. It replaces the values the simulation would otherwise use.
///
/// No source exists yet; task groups 6 and 9 add the DRF and eDRF sources
/// and take-over.
#[derive(Debug)]
pub(crate) enum PlaybackSource {}

impl Observer for PlaybackSource {}

/// The observer a replay or a recorded live game steps the simulation
/// with (design D4).
///
/// It holds an optional playback source, recorder, checkpoint verifier and
/// trace sink as plain fields and handles each hook in a fixed order: the
/// playback source, when there is one, replaces the value; then the
/// recorder, the verifier and the trace sink, in that order, see the value
/// actually used. A component that only observes is told the used value
/// and cannot change it. With every component absent it changes nothing,
/// so stepping a game with it is the same as stepping it plainly.
///
/// The hook-by-hook mapping onto upstream's recorder and player is the
/// "Simulation seam" section of `docs/conformance.md`.
#[derive(Debug, Default)]
pub struct ReplayObserver {
    source: Option<PlaybackSource>,
    recorder: Option<Recorder>,
    verifier: Option<Verifier>,
    trace: Option<TraceSink>,
}

impl ReplayObserver {
    /// An observer with no playback source, recorder, verifier or trace
    /// sink: it observes nothing and replaces nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// The components that only observe, in the order they are told.
    fn observers(&mut self) -> [Option<&mut dyn Observer>; 3] {
        [
            self.recorder.as_mut().map(|r| r as &mut dyn Observer),
            self.verifier.as_mut().map(|v| v as &mut dyn Observer),
            self.trace.as_mut().map(|t| t as &mut dyn Observer),
        ]
    }
}

impl Observer for ReplayObserver {
    fn round_seed(&mut self, round: u32, derived: i32) -> i32 {
        let seed = match &mut self.source {
            Some(source) => source.round_seed(round, derived),
            None => derived,
        };
        for obs in self.observers().into_iter().flatten() {
            obs.round_seed(round, seed);
        }
        seed
    }

    fn level_plan(
        &mut self,
        player: PlayerId,
        level: LevelNumber,
        layout: &LevelLayout,
    ) -> Option<LevelLayout> {
        let replaced = self
            .source
            .as_mut()
            .and_then(|source| source.level_plan(player, level, layout));
        let used = replaced.as_ref().unwrap_or(layout);
        for obs in self.observers().into_iter().flatten() {
            obs.level_plan(player, level, used);
        }
        replaced
    }

    fn controls(&mut self, tick: u32, slot: SlotId, raw: Controls) -> Controls {
        let used = match &mut self.source {
            Some(source) => source.controls(tick, slot, raw),
            None => raw,
        };
        for obs in self.observers().into_iter().flatten() {
            obs.controls(tick, slot, used);
        }
        used
    }

    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement {
        let used = match &mut self.source {
            Some(source) => source.movement(slot, sample),
            None => sample,
        };
        for obs in self.observers().into_iter().flatten() {
            obs.movement(slot, used);
        }
        used
    }

    fn tick_end(&mut self, tick: u32, state: StateView<'_>) {
        if let Some(source) = &mut self.source {
            source.tick_end(tick, state);
        }
        for obs in self.observers().into_iter().flatten() {
            obs.tick_end(tick, state);
        }
    }

    fn round_end(&mut self, state: StateView<'_>) {
        if let Some(source) = &mut self.source {
            source.round_end(state);
        }
        for obs in self.observers().into_iter().flatten() {
            obs.round_end(state);
        }
    }

    fn game_end(&mut self, state: StateView<'_>) {
        if let Some(source) = &mut self.source {
            source.game_end(state);
        }
        for obs in self.observers().into_iter().flatten() {
            obs.game_end(state);
        }
    }
}
