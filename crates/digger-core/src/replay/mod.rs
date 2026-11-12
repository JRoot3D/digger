//! Playing back, verifying and recording games (game-recording and
//! replay-verification specs; design D4 and D6–D8 of
//! `add-recording-and-conformance`).
//!
//! Everything here is built on the simulation's one seam,
//! [`Game::step_observed`] and its [`Observer`] hooks: [`ReplayObserver`]
//! composes a playback source, a recorder, a checkpoint verifier and a
//! trace sink, each optional, and forwards every hook to them in a fixed
//! order (see `docs/conformance.md`, "Simulation seam"). The driver never
//! sleeps and never reads a clock; pacing is the caller's.
//!
//! [`Replay`] plays a [`Recording`] one frame per
//! [`step`](Replay::step):
//!
//! ```
//! use digger_core::recording::parse;
//! use digger_core::replay::{Replay, ReplayOptions, ReplayStep};
//! use digger_core::{FrameInput, NoopObserver};
//!
//! // Two rounds' worth of nothing: the digger stands still for 40 ticks.
//! let recording = parse(b"eDRF 2\nRS DIG 20261111\n1\n20000\nR 00000000\nI @32\nI @8\n")?;
//! let mut replay = Replay::new(&recording, ReplayOptions::default())?;
//! let summary = loop {
//!     // A frontend would present `replay.game().framebuffer()` here.
//!     if let ReplayStep::Finished(summary) = replay.step(&FrameInput::NONE, &mut NoopObserver) {
//!         break summary;
//!     }
//! };
//! assert_eq!(summary.ticks, 40);
//! # Ok::<(), digger_core::recording::RecordingError>(())
//! ```
//!
//! The module holds, or will hold as later task groups add them:
//!
//! - [`Replay`] and [`ReplayOptions`]; later `ReplayReport`, `Outcome` and
//!   `verify()`;
//! - `playback`: the playback sources (DRF and eDRF), [`ReplayObserver`],
//!   [`playback_config`], and later take-over;
//! - `verifier`: checkpoint comparison and divergence reports;
//! - `recorder`: the DRF and eDRF recorders;
//! - `trace`: per-tick trace lines and the decoded state dump.

mod playback;
mod recorder;
mod trace;
mod verifier;

pub use playback::{PlaybackFailure, ReplayObserver, playback_config};

use crate::recording::{Recording, RecordingError};
use crate::{
    Controls, FrameInput, FrameKind, Game, GameSummary, LevelLayout, LevelNumber, Movement,
    Observer, PlayerId, SlotId, StateView,
};

/// The default of [`ReplayOptions::max_ticks`].
pub const DEFAULT_MAX_TICKS: u32 = 10_000_000;

/// How a recording is played (design D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReplayOptions {
    /// Quit the game on the frame after the first failure is found,
    /// instead of playing on with the recorded values.
    pub stop_at_first_failure: bool,
    /// A safety net: the game is quit on the frame after its tick count
    /// reaches this many ticks, so that no recording can play for ever.
    pub max_ticks: u32,
}

impl Default for ReplayOptions {
    fn default() -> Self {
        Self {
            stop_at_first_failure: true,
            max_ticks: DEFAULT_MAX_TICKS,
        }
    }
}

/// What one [`Replay::step`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplayStep {
    /// The game goes on; the frame was of this kind.
    Running(FrameKind),
    /// The game has finished, on this frame or before.
    Finished(GameSummary),
}

/// A recording being played back, one frame per [`step`](Self::step)
/// (design D6).
///
/// The game is configured from the recording alone
/// ([`playback_config`]), and takes its round seeds, level maps and
/// controls from it; the caller's controls are ignored. The game is quit
/// at the start of the frame after the recording runs out, after the
/// first failure (unless [`ReplayOptions::stop_at_first_failure`] is
/// off), or after [`ReplayOptions::max_ticks`] ticks.
#[derive(Debug)]
pub struct Replay {
    game: Box<Game>,
    observer: ReplayObserver,
    options: ReplayOptions,
    /// Whether the game was quit because it reached the tick limit.
    tick_limit_reached: bool,
}

impl Replay {
    /// A playback of `recording` that has not run a frame yet.
    ///
    /// # Errors
    ///
    /// None at present: a recording that parsed can be played. The result
    /// leaves room for checks a later revision adds.
    pub fn new(recording: &Recording, options: ReplayOptions) -> Result<Self, RecordingError> {
        let config = playback_config(recording);
        // The base seed is never used: every round's seed comes from the
        // recording.
        let game = Game::new(config, 0);
        Ok(Self {
            game,
            observer: ReplayObserver::playing(recording),
            options,
            tick_limit_reached: false,
        })
    }

    /// Advances the playback by exactly one frame. It never waits.
    ///
    /// Of `input` only the pause and quit requests have an effect: a quit
    /// stops the playback there. Its controls are replaced by the
    /// recording's on every tick (raw controls from an eDRF, none at all
    /// during DRF playback, where each control read takes a record).
    /// `extra` is told every hook after the replay's own observer, with
    /// the values actually used; what it returns is ignored. Once the game has finished every step reports the same
    /// summary.
    pub fn step(&mut self, input: &FrameInput, extra: &mut dyn Observer) -> ReplayStep {
        if let Some(summary) = self.game.summary() {
            return ReplayStep::Finished(summary);
        }
        // The caller's controls go in as they are: the playback source
        // replaces every slot's raw controls on every tick (quirks P3, P4).
        let mut frame = *input;
        let ticks = self.game.state().ticks();
        let options = self.options;
        if let Some(source) = self.observer.source_mut() {
            let failed = source.failure().is_some();
            if source.exhausted() {
                frame.quit = true;
            } else if input.quit || (failed && options.stop_at_first_failure) {
                frame.quit = true;
                source.stop();
            } else if ticks >= options.max_ticks {
                frame.quit = true;
                self.tick_limit_reached = true;
                source.stop();
            }
        }
        let mut chain = Chain {
            replay: &mut self.observer,
            extra,
        };
        let outcome = self.game.step_observed(&frame, &mut chain);
        match outcome.finished {
            Some(summary) => ReplayStep::Finished(summary),
            None => ReplayStep::Running(outcome.frame),
        }
    }

    /// The game being played: its picture, sound commands and state.
    pub fn game(&self) -> &Game {
        &self.game
    }

    /// Removes and yields the sound commands produced since the last
    /// drain (see [`Game::drain_sound`]).
    pub fn drain_sound(&mut self) -> impl Iterator<Item = crate::SoundCommand> + '_ {
        self.game.drain_sound()
    }

    /// The first failure the playback found, if any.
    pub fn failure(&self) -> Option<&PlaybackFailure> {
        self.observer.source().and_then(|source| source.failure())
    }

    /// Whether the recording has run out: every eDRF value used up, or a
    /// DRF control read found its end.
    pub fn recording_exhausted(&self) -> bool {
        self.observer
            .source()
            .is_some_and(playback::PlaybackSource::exhausted)
    }

    /// Whether the game was quit because it reached
    /// [`ReplayOptions::max_ticks`].
    pub const fn tick_limit_reached(&self) -> bool {
        self.tick_limit_reached
    }
}

/// The replay's own observer followed by the caller's `extra` one, which
/// sees the values actually used and replaces nothing.
struct Chain<'a> {
    replay: &'a mut ReplayObserver,
    extra: &'a mut dyn Observer,
}

impl Observer for Chain<'_> {
    fn round_seed(&mut self, round: u32, derived: i32) -> i32 {
        let seed = self.replay.round_seed(round, derived);
        self.extra.round_seed(round, seed);
        seed
    }

    fn level_plan(
        &mut self,
        player: PlayerId,
        level: LevelNumber,
        layout: &LevelLayout,
    ) -> Option<LevelLayout> {
        let replaced = self.replay.level_plan(player, level, layout);
        self.extra
            .level_plan(player, level, replaced.as_ref().unwrap_or(layout));
        replaced
    }

    fn controls(&mut self, tick: u32, slot: SlotId, raw: Controls) -> Controls {
        let used = self.replay.controls(tick, slot, raw);
        self.extra.controls(tick, slot, used);
        used
    }

    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement {
        let used = self.replay.movement(slot, sample);
        self.extra.movement(slot, used);
        used
    }

    fn tick_end(&mut self, tick: u32, state: StateView<'_>) {
        self.replay.tick_end(tick, state);
        self.extra.tick_end(tick, state);
    }

    fn round_end(&mut self, state: StateView<'_>) {
        self.replay.round_end(state);
        self.extra.round_end(state);
    }

    fn game_end(&mut self, state: StateView<'_>) {
        self.replay.game_end(state);
        self.extra.game_end(state);
    }
}
