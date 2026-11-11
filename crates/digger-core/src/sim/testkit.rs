//! Scenario builder and input scripts for tests, behind the `testkit`
//! feature (design D11).
//!
//! A [`Scenario`] describes a game — mode, an optional one-layout level
//! set, start level, seed — and builds a [`Run`], which steps the game
//! through its waiting frames transparently, feeds it a [`Script`] of
//! per-tick inputs, collects its sound commands and offers validated
//! setters to put the game into a situation directly.
//!
//! ```
//! use digger_core::GameMode;
//! use digger_core::testkit::{Scenario, Script};
//!
//! let mut run = Scenario::new(GameMode::OnePlayer)
//!     .rows([
//!         "S              ",
//!         "               ",
//!         "               ",
//!         "               ",
//!         "               ",
//!         "               ",
//!         "               ",
//!         "    C          ",
//!         "               ",
//!         "       S       ",
//!     ])
//!     .seed(7)
//!     .build()
//!     .script(Script::parse("R*5 . U*6 F").unwrap());
//! assert_eq!(run.run_ticks(13), 13);
//! assert_eq!(run.state().ticks(), 13);
//! ```
//!
//! Misusing the kit (rows of the wrong length, a start level out of range,
//! a game that never ticks again) is a bug in the test and panics with a
//! message.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "the test kit runs outside the simulation's no-allocation rule (design D9)"
)]

use std::fmt;

use crate::sound_cmd::SoundCommand;

use super::config::{GameConfig, GameMode, StartLevel};
use super::ids::DiggerId;
use super::input::{Controls, FrameInput};
use super::level::{LevelLayout, LevelSet};
use super::observer::{NoopObserver, Observer};
use super::score::ScoreId;
use super::state::StateView;
use super::{FrameKind, Game, GameSummary, StepOutcome};

/// Consecutive frames without a tick after which [`Run::run_ticks`] gives
/// up: far more than any waiting phase of the game lasts.
const MAX_IDLE_FRAMES: u32 = 100_000;

/// The largest visible score; a larger one would have rolled over.
const MAX_VISIBLE_SCORE: i32 = 999_999;

/// A game to be built for a test (design D11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    mode: GameMode,
    layout: Option<LevelLayout>,
    bonus_step: u16,
    start_level: StartLevel,
    unlimited_lives: bool,
    seed: u64,
}

impl Scenario {
    /// A game in `mode` on the built-in levels, starting on level 1 with
    /// base seed 0.
    pub fn new(mode: GameMode) -> Self {
        Self {
            mode,
            layout: None,
            bonus_step: LevelSet::builtin().bonus_step(),
            start_level: StartLevel::FIRST,
            unlimited_lives: false,
            seed: 0,
        }
    }

    /// Plays every level on the layout written as these ten rows of 15
    /// characters, top row first, in a custom (verbatim) level set.
    ///
    /// # Panics
    ///
    /// When a row is not 15 bytes long.
    #[expect(clippy::panic, reason = "invalid rows are a bug in the test")]
    #[must_use]
    pub fn rows(self, rows: [&str; 10]) -> Self {
        match LevelLayout::parse_rows(rows) {
            Ok(layout) => self.layout(layout),
            Err(error) => panic!("invalid scenario rows: {error}"),
        }
    }

    /// Plays every level on `layout`, in a custom (verbatim) level set.
    #[must_use]
    pub fn layout(mut self, layout: LevelLayout) -> Self {
        self.layout = Some(layout);
        self
    }

    /// Sets the bonus-life score step.
    #[must_use]
    pub fn bonus_step(mut self, step: u16) -> Self {
        self.bonus_step = step;
        self
    }

    /// Starts every player on `level`.
    ///
    /// # Panics
    ///
    /// When `level` is outside 1..=1000.
    #[expect(clippy::panic, reason = "an invalid start level is a bug in the test")]
    #[must_use]
    pub fn start_level(mut self, level: i64) -> Self {
        match StartLevel::new(level) {
            Ok(level) => self.start_level = level,
            Err(error) => panic!("invalid scenario start level: {error}"),
        }
        self
    }

    /// Lifts the cap on bonus lives.
    #[must_use]
    pub fn unlimited_lives(mut self) -> Self {
        self.unlimited_lives = true;
        self
    }

    /// Sets the base seed the round seeds are derived from.
    #[must_use]
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// The configuration the scenario's game is started with.
    pub fn config(&self) -> GameConfig {
        let levels = match self.layout {
            Some(layout) => LevelSet::from_plans([layout; 8], self.bonus_step, true),
            None => LevelSet::from_plans(*LevelSet::builtin().layouts(), self.bonus_step, false),
        };
        GameConfig {
            mode: self.mode,
            start_level: self.start_level,
            unlimited_lives: self.unlimited_lives,
            levels,
        }
    }

    /// The scenario's game, not yet stepped, with no observer.
    pub fn build(&self) -> Run {
        Run::new(Game::new(self.config(), self.seed))
    }

    /// The scenario's game, not yet stepped, observed by `obs` on every
    /// step.
    pub fn build_observed<O: Observer>(&self, obs: O) -> Run<O> {
        Run::with_observer(Game::new(self.config(), self.seed), obs)
    }
}

/// Inputs for consecutive ticks, written as text (design D11).
///
/// The text is a whitespace-separated list of entries, each optionally
/// followed by `*n` to repeat it `n` times. An entry is `.` for no input or
/// any combination of `U`, `D`, `L`, `R` (directions) and `F` (fire) for the
/// first control slot; `/` followed by another such part gives the second
/// slot's controls (`R/L`, `./U`). `P` (pause) and `Q` (quit) may appear in
/// either part and request that for the frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Script {
    inputs: Vec<FrameInput>,
}

impl Script {
    /// The script written as `text`.
    ///
    /// # Errors
    ///
    /// A [`ScriptError`] naming the first entry that cannot be read.
    pub fn parse(text: &str) -> Result<Self, ScriptError> {
        let mut inputs = Vec::new();
        for token in text.split_whitespace() {
            let error = |reason| ScriptError {
                token: token.to_owned(),
                reason,
            };
            let (entry, count) = match token.split_once('*') {
                Some((entry, count)) => {
                    let count: usize = count.parse().map_err(|_| error("bad repeat count"))?;
                    if count == 0 {
                        return Err(error("repeat count of 0"));
                    }
                    (entry, count)
                }
                None => (token, 1),
            };
            let input = parse_entry(entry).map_err(error)?;
            inputs.extend(std::iter::repeat_n(input, count));
        }
        Ok(Self { inputs })
    }

    /// The script made of these inputs, one per tick.
    pub fn from_inputs(inputs: Vec<FrameInput>) -> Self {
        Self { inputs }
    }

    /// The inputs, one per tick.
    pub fn inputs(&self) -> &[FrameInput] {
        &self.inputs
    }

    /// The input of the tick at `index`, if the script reaches it.
    pub fn get(&self, index: usize) -> Option<FrameInput> {
        self.inputs.get(index).copied()
    }

    /// The number of inputs.
    pub fn len(&self) -> usize {
        self.inputs.len()
    }

    /// Whether the script has no input.
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }
}

/// Reads one script entry without its repeat count.
fn parse_entry(entry: &str) -> Result<FrameInput, &'static str> {
    let mut input = FrameInput::NONE;
    let (first, second) = match entry.split_once('/') {
        Some((first, second)) => (first, Some(second)),
        None => (entry, None),
    };
    input.slots[0] = parse_part(first, &mut input)?;
    if let Some(second) = second {
        input.slots[1] = parse_part(second, &mut input)?;
    }
    Ok(input)
}

/// Reads the controls of one slot's part of an entry, recording pause and
/// quit requests in `input`.
fn parse_part(part: &str, input: &mut FrameInput) -> Result<Controls, &'static str> {
    if part == "." {
        return Ok(Controls::NONE);
    }
    if part.is_empty() {
        return Err("empty entry");
    }
    let mut controls = Controls::NONE;
    for c in part.chars() {
        match c {
            'U' => controls.up = true,
            'D' => controls.down = true,
            'L' => controls.left = true,
            'R' => controls.right = true,
            'F' => controls.fire = true,
            'P' => input.pause = true,
            'Q' => input.quit = true,
            _ => return Err("unknown control letter"),
        }
    }
    Ok(controls)
}

/// A script entry that cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError {
    /// The entry as written.
    pub token: String,
    /// What is wrong with it.
    pub reason: &'static str,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "script entry {:?}: {}", self.token, self.reason)
    }
}

impl std::error::Error for ScriptError {}

/// A set-up that would put the game into a state it cannot reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupError {
    /// The digger record is not in play in this mode.
    DiggerNotInPlay {
        /// The record asked for.
        digger: DiggerId,
    },
    /// The score is not in play in this mode.
    ScoreNotInPlay {
        /// The score asked for.
        score: ScoreId,
    },
    /// A visible score outside 0..=999,999.
    ScoreOutOfRange {
        /// The rejected value.
        value: i32,
    },
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DiggerNotInPlay { digger } => {
                write!(f, "digger record {} is not in play", digger.number())
            }
            Self::ScoreNotInPlay { score } => write!(f, "score {} is not in play", score.number()),
            Self::ScoreOutOfRange { value } => {
                write!(f, "visible score {value} is outside 0..=999999")
            }
        }
    }
}

impl std::error::Error for SetupError {}

/// A game being run by a test: steps through waiting frames transparently,
/// feeds a [`Script`] one entry per tick and collects the sound commands
/// (design D11).
#[derive(Debug)]
pub struct Run<O: Observer = NoopObserver> {
    game: Box<Game>,
    obs: O,
    script: Script,
    cursor: usize,
    sounds: Vec<SoundCommand>,
    last: Option<StepOutcome>,
    frames: u64,
}

impl Run {
    /// Runs `game` with no observer.
    pub fn new(game: Box<Game>) -> Self {
        Self::with_observer(game, NoopObserver)
    }
}

impl<O: Observer> Run<O> {
    /// Runs `game`, observed by `obs` on every step.
    pub fn with_observer(game: Box<Game>, obs: O) -> Self {
        Self {
            game,
            obs,
            script: Script::default(),
            cursor: 0,
            sounds: Vec::new(),
            last: None,
            frames: 0,
        }
    }

    /// Feeds `script` from the next frame on.
    #[must_use]
    pub fn script(mut self, script: Script) -> Self {
        self.set_script(script);
        self
    }

    /// Replaces the script; its first entry is used next.
    pub fn set_script(&mut self, script: Script) {
        self.script = script;
        self.cursor = 0;
    }

    /// The game.
    pub fn game(&self) -> &Game {
        &self.game
    }

    /// The observer.
    pub fn observer(&self) -> &O {
        &self.obs
    }

    /// The observer, mutably.
    pub fn observer_mut(&mut self) -> &mut O {
        &mut self.obs
    }

    /// The game's state.
    pub fn state(&self) -> StateView<'_> {
        self.game.state()
    }

    /// Ticks run so far.
    pub fn ticks(&self) -> u32 {
        self.game.state().ticks()
    }

    /// Frames stepped so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The outcome of the last frame, if any frame ran.
    pub fn last_outcome(&self) -> Option<StepOutcome> {
        self.last
    }

    /// The summary, once the game has finished.
    pub fn summary(&self) -> Option<GameSummary> {
        self.game.summary()
    }

    /// Every sound command produced since the run began or since the last
    /// [`take_sounds`](Self::take_sounds), in order.
    pub fn sounds(&self) -> &[SoundCommand] {
        &self.sounds
    }

    /// Removes and returns the collected sound commands.
    pub fn take_sounds(&mut self) -> Vec<SoundCommand> {
        std::mem::take(&mut self.sounds)
    }

    /// Steps one frame with `input`, leaving the script where it is.
    pub fn step(&mut self, input: &FrameInput) -> StepOutcome {
        let outcome = self.game.step_observed(input, &mut self.obs);
        self.sounds.extend(self.game.drain_sound());
        self.last = Some(outcome);
        self.frames += 1;
        outcome
    }

    /// Steps one frame with the script's next entry, or with no input once
    /// the script is used up. An entry is used up by a tick frame, by a
    /// paused frame when it requests a pause (the resume), and by any frame
    /// when it requests a quit; waiting frames see it without using it up.
    pub fn frame(&mut self) -> StepOutcome {
        let input = self.script.get(self.cursor).unwrap_or(FrameInput::NONE);
        let outcome = self.step(&input);
        let used = match outcome.frame {
            FrameKind::Tick => true,
            FrameKind::Paused => input.pause || input.quit,
            FrameKind::Wait => input.quit,
        };
        if used {
            self.cursor += 1;
        }
        outcome
    }

    /// Steps `n` frames with the script; returns the last outcome.
    pub fn run_frames(&mut self, n: u32) -> Option<StepOutcome> {
        for _ in 0..n {
            self.frame();
        }
        self.last
    }

    /// Steps until `n` more ticks have run or the game has finished, using
    /// the script; returns the number of ticks run.
    ///
    /// # Panics
    ///
    /// When 100,000 frames in a row pass without a tick (a pause that is
    /// never resumed, for example).
    pub fn run_ticks(&mut self, n: u32) -> u32 {
        let start = self.ticks();
        let mut idle = 0;
        while self.ticks().wrapping_sub(start) < n && self.summary().is_none() {
            if self.frame().frame == FrameKind::Tick {
                idle = 0;
            } else {
                idle += 1;
                assert!(
                    idle < MAX_IDLE_FRAMES,
                    "no tick for {MAX_IDLE_FRAMES} frames"
                );
            }
        }
        self.ticks().wrapping_sub(start)
    }

    /// Steps with the script until `done` holds for the state, at most
    /// `max_frames` frames; returns whether it holds. It is checked before
    /// the first frame and after every frame; a finished game stops the run.
    pub fn run_until(
        &mut self,
        mut done: impl FnMut(StateView<'_>) -> bool,
        max_frames: u64,
    ) -> bool {
        for _ in 0..max_frames {
            if done(self.state()) {
                return true;
            }
            if self.summary().is_some() {
                return false;
            }
            self.frame();
        }
        done(self.state())
    }

    /// Sets the lives of digger record `digger`, which must be in play: the
    /// first always, the second in alternating and two-digger games. The
    /// lives display is not redrawn.
    ///
    /// # Errors
    ///
    /// [`SetupError::DiggerNotInPlay`] for a record not in play.
    pub fn set_lives(&mut self, digger: DiggerId, lives: u32) -> Result<(), SetupError> {
        let mode = self.game.sim.cfg.mode;
        let in_play = usize::from(mode.diggers() + mode.players() - 1);
        if digger.index() >= in_play {
            return Err(SetupError::DiggerNotInPlay { digger });
        }
        digger.of_mut(&mut self.game.sim.diggers).lives = lives;
        Ok(())
    }

    /// Sets score `score`'s visible value, which must be in play (the
    /// second only in alternating and two-digger games) and within
    /// 0..=999,999. Nothing is redrawn and no bonus is checked.
    ///
    /// # Errors
    ///
    /// [`SetupError::ScoreNotInPlay`] or [`SetupError::ScoreOutOfRange`].
    pub fn set_score(&mut self, score: ScoreId, visible: i32) -> Result<(), SetupError> {
        let mode = self.game.sim.cfg.mode;
        let scores = if mode.alternating() || mode.two_diggers() {
            2
        } else {
            1
        };
        if score.index() >= scores {
            return Err(SetupError::ScoreNotInPlay { score });
        }
        if !(0..=MAX_VISIBLE_SCORE).contains(&visible) {
            return Err(SetupError::ScoreOutOfRange { value: visible });
        }
        self.game.sim.scores.set_visible(score, visible);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RIGHT: Controls = Controls {
        right: true,
        ..Controls::NONE
    };

    #[test]
    fn script_parses_repeat_and_idle() {
        let script = Script::parse("R*5 . U*6 F").unwrap_or_default();
        assert_eq!(script.len(), 13);
        let up = Controls {
            up: true,
            ..Controls::NONE
        };
        let fire = Controls {
            fire: true,
            ..Controls::NONE
        };
        let slot0: Vec<Controls> = script.inputs().iter().map(|i| i.slots[0]).collect();
        let mut expected = vec![RIGHT; 5];
        expected.push(Controls::NONE);
        expected.extend([up; 6]);
        expected.push(fire);
        assert_eq!(slot0, expected);
        assert!(
            script
                .inputs()
                .iter()
                .all(|i| i.slots[1] == Controls::NONE && !i.pause && !i.quit)
        );
    }

    #[test]
    fn script_parses_second_slot_and_requests() {
        let script = Script::parse("RF/L ./U P Q").unwrap_or_default();
        assert_eq!(script.len(), 4);
        let first = script.get(0).unwrap_or_default();
        assert!(first.slots[0].right && first.slots[0].fire && first.slots[1].left);
        let second = script.get(1).unwrap_or_default();
        assert_eq!(second.slots[0], Controls::NONE);
        assert!(second.slots[1].up);
        assert!(script.get(2).is_some_and(|i| i.pause && !i.quit));
        assert!(script.get(3).is_some_and(|i| i.quit && !i.pause));
        assert_eq!(script.get(4), None);
    }

    #[test]
    fn script_rejects_bad_entries() {
        for text in ["X", "R*0", "R*x", "R/", "/R", ""] {
            let parsed = Script::parse(text);
            if text.is_empty() {
                assert_eq!(parsed, Ok(Script::default()));
            } else {
                assert!(parsed.is_err(), "{text:?} accepted");
            }
        }
    }

    #[test]
    fn scenario_runs_to_first_tick() {
        let mut run = Scenario::new(GameMode::OnePlayer)
            .rows([
                "S              ",
                "               ",
                "               ",
                "  C            ",
                "               ",
                "               ",
                "               ",
                "               ",
                "               ",
                "       S       ",
            ])
            .seed(3)
            .build();
        assert_eq!(run.ticks(), 0);
        assert_eq!(run.run_ticks(1), 1);
        assert_eq!(run.ticks(), 1);
        // Two opened cells and one emerald: one draw frame, then the tick.
        assert_eq!(run.frames(), 2);
        assert_eq!(
            run.state().emeralds().count(run.state().current_player()),
            1
        );
        assert_eq!(run.state().rng_state(), crate::sim::round_seed(3, 0));
        assert_eq!(
            run.sounds(),
            [SoundCommand::Music {
                tune: crate::sound_cmd::Tune::Main,
                tempo: crate::sound_cmd::Tempo::Standard,
            }]
        );
    }

    #[test]
    fn run_until_stops_when_condition_holds() {
        let mut run = Scenario::new(GameMode::OnePlayer).build();
        assert!(run.run_until(|s| s.ticks() == 5, 1000));
        assert_eq!(run.ticks(), 5);
        assert!(!run.run_until(|s| s.ticks() == 1000, 10));
    }

    #[test]
    fn setters_validate_what_is_in_play() {
        let mut run = Scenario::new(GameMode::OnePlayer).build();
        let [d0, d1] = DiggerId::ALL;
        let [s0, s1] = ScoreId::ALL;
        assert_eq!(run.set_lives(d0, 1), Ok(()));
        assert_eq!(run.state().lives(d0), 1);
        assert_eq!(
            run.set_lives(d1, 2),
            Err(SetupError::DiggerNotInPlay { digger: d1 })
        );
        assert_eq!(run.set_score(s0, 19_975), Ok(()));
        assert_eq!(run.state().scores().visible(s0), 19_975);
        assert_eq!(
            run.set_score(s1, 5),
            Err(SetupError::ScoreNotInPlay { score: s1 })
        );
        assert_eq!(
            run.set_score(s0, 1_000_000),
            Err(SetupError::ScoreOutOfRange { value: 1_000_000 })
        );

        let mut run = Scenario::new(GameMode::TwoDiggers).build();
        assert_eq!(run.set_lives(d1, 2), Ok(()));
        assert_eq!(run.set_score(s1, 5), Ok(()));
    }
}
