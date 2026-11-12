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
//! - [`Replay`], [`ReplayOptions`], [`ReplayReport`], [`Outcome`] and
//!   [`verify()`], the headless verify run;
//! - `playback`: the playback sources (DRF and eDRF), [`ReplayObserver`],
//!   [`playback_config`], and later take-over;
//! - `verifier`: checkpoint comparison and divergence reports;
//! - `recorder`: the DRF and eDRF recorders;
//! - `trace`: per-tick trace lines and the decoded state dump.

mod playback;
mod recorder;
mod trace;
mod verifier;

pub use playback::{Failure, PlaybackFailure, ReplayObserver, playback_config};
pub use recorder::{
    DRF_MAX_RUN, DRF_WRAP_COLUMNS, DrfRecorder, EDRF_CHUNK, EdrfRecorder, Recorder, SlotSample,
};
pub use verifier::{CHECKPOINT_EVERY, Divergence};

use core::fmt::Write as _;

use crate::recording::{Recording, RecordingError, RecordingKind, parse};
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
    kind: RecordingKind,
    /// Whether the game was quit because it reached the tick limit.
    tick_limit_reached: bool,
    /// Whether the player took the game over from the recording. Nothing
    /// sets it yet: take-over is added by task group 9.
    taken_over: bool,
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
            observer: ReplayObserver::verifying(recording),
            options,
            kind: recording.kind(),
            tick_limit_reached: false,
            taken_over: false,
        })
    }

    /// A playback of `recording` that also records the game as it is
    /// played, in the format `kind` (game-recording spec, "Re-recording
    /// and conversion between formats"). Recording a DRF playback as eDRF
    /// converts it: each read slot's value is the direction and fire the
    /// DRF supplied at the slot's control read, 0 on a tick without one.
    ///
    /// # Errors
    ///
    /// Those of [`Replay::new`].
    pub fn recording_as(
        recording: &Recording,
        options: ReplayOptions,
        kind: RecordingKind,
    ) -> Result<Self, RecordingError> {
        let mut replay = Self::new(recording, options)?;
        let config = playback_config(recording);
        let drf_playback = recording.kind() == RecordingKind::Drf;
        replay
            .observer
            .set_recorder(Recorder::new(kind, &config, drf_playback));
        Ok(replay)
    }

    /// The recorder of a playback made with
    /// [`recording_as`](Self::recording_as), for draining its text as the
    /// game goes.
    pub const fn recorder_mut(&mut self) -> Option<&mut Recorder> {
        self.observer.recorder_mut()
    }

    /// Ends the playback as [`finish`](Self::finish) does, and returns the
    /// text its recorder has not yet handed out (for a DRF the whole
    /// recording), if it has one.
    pub fn finish_with_recording(mut self) -> (ReplayReport, Option<String>) {
        self.run_to_end();
        let text = self
            .observer
            .take_recorder()
            .map(|mut recorder| recorder.take_pending());
        (self.report(), text)
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
        let failed = self.observer.failure().is_some();
        if let Some(source) = self.observer.source_mut() {
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

    /// The first failure the playback found, if any: a checkpoint
    /// mismatch or a failure of the playback itself, whichever came first.
    pub fn failure(&self) -> Option<&Failure> {
        self.observer.failure()
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

    /// Ends the playback and reports how it went (replay-verification
    /// spec, "Playback outcome classification").
    ///
    /// A game still running is quit first, which makes the playback
    /// stopped unless it had already failed or run out of recording.
    pub fn finish(mut self) -> ReplayReport {
        self.run_to_end();
        self.report()
    }

    /// Quits the game if it still runs.
    fn run_to_end(&mut self) {
        let quit = FrameInput {
            quit: true,
            ..FrameInput::NONE
        };
        while let ReplayStep::Running(_) = self.step(&quit, &mut crate::NoopObserver) {}
    }

    /// The report of the finished playback.
    fn report(&self) -> ReplayReport {
        let Some(summary) = self.game.summary() else {
            unreachable!("the report is made once the game has finished")
        };
        let outcome = self.outcome();
        let last_good_checkpoint = self
            .observer
            .verifier()
            .and_then(verifier::Verifier::last_match);
        ReplayReport {
            kind: self.kind,
            outcome,
            scores: summary.scores.map(|score| score.total),
            two_scores: two_scores(self.game.state().mode()),
            level: summary.level.get(),
            ticks: summary.ticks,
            last_good_checkpoint,
        }
    }

    /// The outcome of the finished playback, by the spec's precedence:
    /// failed, stopped, truncated, taken over, then verified (eDRF) or
    /// completed (DRF).
    fn outcome(&self) -> Outcome {
        if let Some(failure) = self.observer.failure() {
            return Outcome::Failed(failure.clone());
        }
        let source = self.observer.source();
        if source.is_some_and(playback::PlaybackSource::stopped_early) {
            return Outcome::Stopped;
        }
        let has_end = self
            .observer
            .verifier()
            .is_some_and(verifier::Verifier::has_end);
        match self.kind {
            RecordingKind::Edrf if !has_end => Outcome::Truncated,
            _ if self.taken_over => Outcome::TakenOver,
            RecordingKind::Edrf => Outcome::Verified,
            RecordingKind::Drf => Outcome::Completed,
        }
    }
}

/// Whether a game in `mode` has two scores: two alternating players or
/// two simultaneous diggers.
const fn two_scores(mode: crate::GameMode) -> bool {
    mode.players() > 1 || mode.two_diggers()
}

/// How a playback ended (replay-verification spec, "Playback outcome
/// classification"). Every playback ends in exactly one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// An eDRF whose every checkpoint and end record matched.
    Verified,
    /// A DRF played to its end (it has no checkpoints).
    Completed,
    /// A checkpoint mismatch, a level-number mismatch, a bad record or a
    /// DRF playback out of step; the first one found.
    Failed(Failure),
    /// The game ended while the recording still held unused controls or
    /// rounds.
    Stopped,
    /// An eDRF whose data ran out with no `Z` record.
    Truncated,
    /// The player took the game over from the recording.
    TakenOver,
}

/// The exit status of a verify run that ended as verified or completed.
pub const STATUS_OK: u8 = 0;
/// The exit status when the file cannot be opened or read as a recording.
pub const STATUS_UNREADABLE: u8 = 1;
/// The exit status of a failed playback.
pub const STATUS_FAILED: u8 = 2;
/// The exit status of a stopped playback.
pub const STATUS_STOPPED: u8 = 3;
/// The exit status of a truncated eDRF.
pub const STATUS_TRUNCATED: u8 = 4;

impl RecordingError {
    /// The verify run's exit status for a recording that could not be
    /// played (replay-verification spec, "Verify exit statuses"; design
    /// D6): a bad eDRF record or DRF body is a failed playback (2),
    /// anything else an unreadable recording (1).
    pub const fn exit_status(&self) -> u8 {
        match self {
            Self::BadRecord { .. } | Self::BadDrfBody { .. } => STATUS_FAILED,
            _ => STATUS_UNREADABLE,
        }
    }
}

/// The result of a finished playback (design D6).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReplayReport {
    /// The recording's format.
    pub kind: RecordingKind,
    /// How the playback ended.
    pub outcome: Outcome,
    /// The total scores of player/digger 1 and 2.
    pub scores: [i32; 2],
    /// Whether the game had two alternating players or two diggers, so
    /// that the result line shows both scores.
    pub two_scores: bool,
    /// The level of the player playing when the game ended.
    pub level: u16,
    /// The tick total.
    pub ticks: u32,
    /// The tick count of the last checkpoint that matched (eDRF only).
    pub last_good_checkpoint: Option<u32>,
}

impl ReplayReport {
    /// The result line of a successful verify run, ending in one LF
    /// (replay-verification spec, "Result line"): `score=S level=L
    /// ticks=T`, or `score=S score2=S2 level=L ticks=T` with two scores.
    pub fn result_line(&self) -> String {
        let [first, second] = self.scores;
        let mut line = format!("score={first}");
        if self.two_scores {
            let _ = write!(line, " score2={second}");
        }
        let _ = writeln!(line, " level={} ticks={}", self.level, self.ticks);
        line
    }

    /// The verify run's exit status (replay-verification spec, "Verify
    /// exit statuses"): 0 verified or completed, 2 failed, 3 stopped,
    /// 4 truncated.
    pub const fn exit_status(&self) -> u8 {
        match self.outcome {
            Outcome::Verified | Outcome::Completed | Outcome::TakenOver => STATUS_OK,
            Outcome::Failed(_) => STATUS_FAILED,
            Outcome::Stopped => STATUS_STOPPED,
            Outcome::Truncated => STATUS_TRUNCATED,
        }
    }

    /// The explanation for the error stream of a run that did not succeed,
    /// one LF-terminated line each; empty when the exit status is 0
    /// (replay-verification spec, "Divergence report"). `file` names the
    /// recording.
    pub fn diagnostics(&self, file: &str) -> String {
        let format = match self.kind {
            RecordingKind::Drf => "DRF",
            RecordingKind::Edrf => "eDRF",
        };
        let mut text = String::new();
        match &self.outcome {
            Outcome::Verified | Outcome::Completed | Outcome::TakenOver => {}
            Outcome::Failed(failure) => {
                let _ = writeln!(text, "{failure}");
                if self.kind == RecordingKind::Edrf {
                    match self.last_good_checkpoint {
                        Some(tick) => {
                            let _ = writeln!(text, "last matching checkpoint: tick {tick}");
                        }
                        None => text.push_str("last matching checkpoint: none\n"),
                    }
                }
                let _ = writeln!(text, "{format}: playback of {file} FAILED");
            }
            Outcome::Stopped => {
                let _ = writeln!(
                    text,
                    "{format}: playback stopped on tick {}, before the end of the recording",
                    self.ticks
                );
            }
            Outcome::Truncated => {
                let _ = writeln!(
                    text,
                    "eDRF: the recording ends on tick {}, before its game does",
                    self.ticks
                );
            }
        }
        text
    }
}

/// The headless verify run (replay-verification spec, "Headless verify
/// run"): reads `bytes` as a recording and plays it to its end as fast as
/// possible, with no devices and no pacing.
///
/// # Errors
///
/// Those of [`parse`] and [`Replay::new`]; see
/// [`RecordingError::exit_status`] for the exit status of each.
pub fn verify(bytes: &[u8], options: ReplayOptions) -> Result<ReplayReport, RecordingError> {
    let recording = parse(bytes)?;
    let mut replay = Replay::new(&recording, options)?;
    while let ReplayStep::Running(_) = replay.step(&FrameInput::NONE, &mut crate::NoopObserver) {}
    Ok(replay.finish())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::{DrfProblem, EndRecord, OutOfStep};

    fn report(kind: RecordingKind, outcome: Outcome) -> ReplayReport {
        ReplayReport {
            kind,
            outcome,
            scores: [3100, 0],
            two_scores: false,
            level: 2,
            ticks: 1378,
            last_good_checkpoint: Some(512),
        }
    }

    fn tick_divergence() -> Outcome {
        Outcome::Failed(Failure::Divergence(Divergence::Tick {
            tick: 768,
            expected: 0x9988_7766,
            actual: 0x0A1B_2C3D,
        }))
    }

    #[test]
    fn result_line_one_player() {
        let report = report(RecordingKind::Edrf, Outcome::Verified);
        assert_eq!(report.result_line(), "score=3100 level=2 ticks=1378\n");
        let zero = ReplayReport {
            scores: [0, 0],
            ticks: 2947,
            ..report
        };
        assert_eq!(zero.result_line(), "score=0 level=2 ticks=2947\n");
    }

    #[test]
    fn result_line_two_scores() {
        let report = ReplayReport {
            scores: [18100, 1100],
            two_scores: true,
            level: 4,
            ticks: 4489,
            ..report(RecordingKind::Drf, Outcome::Completed)
        };
        assert_eq!(
            report.result_line(),
            "score=18100 score2=1100 level=4 ticks=4489\n"
        );
    }

    #[test]
    fn exit_status_table() {
        use RecordingKind::{Drf, Edrf};
        let cases = [
            (Edrf, Outcome::Verified, 0),
            (Drf, Outcome::Completed, 0),
            (Edrf, tick_divergence(), 2),
            (Edrf, Outcome::Stopped, 3),
            (Drf, Outcome::Stopped, 3),
            (Edrf, Outcome::Truncated, 4),
        ];
        for (kind, outcome, status) in cases {
            let report = report(kind, outcome.clone());
            assert_eq!(report.exit_status(), status, "{outcome:?}");
            assert_eq!(
                report.diagnostics("x").is_empty(),
                status == 0,
                "{outcome:?}: diagnostics exactly when the status is not 0"
            );
        }
        let failed_drf = Outcome::Failed(Failure::Playback(PlaybackFailure::OutOfStep {
            tick: 99,
            problem: OutOfStep::RepeatsPending { remaining: 5 },
        }));
        assert_eq!(report(Drf, failed_drf).exit_status(), 2);

        // Errors before play: a bad record or body is a failure (2),
        // anything else an unreadable recording (1).
        let errors = [
            (RecordingError::NotARecording, 1),
            (
                RecordingError::UnsupportedVersion {
                    version_line: "AJ DOS 19981125".to_owned(),
                },
                1,
            ),
            (
                RecordingError::BadHeader {
                    line: 3,
                    reason: crate::recording::HeaderProblem::MissingModeLine,
                },
                1,
            ),
            (
                RecordingError::BadRecord {
                    line: 7,
                    text: "X 1".to_owned(),
                },
                2,
            ),
            (
                RecordingError::BadDrfBody {
                    offset: 900,
                    reason: DrfProblem::CountTooLarge,
                },
                2,
            ),
        ];
        for (error, status) in errors {
            assert_eq!(error.exit_status(), status, "{error:?}");
        }
    }

    #[test]
    fn divergence_messages() {
        let text = report(RecordingKind::Edrf, tick_divergence()).diagnostics("game.edrf");
        assert_eq!(
            text,
            "eDRF: the game diverges from the recording before tick 768 \
             (tick state 0A1B2C3D, recorded 99887766)\n\
             last matching checkpoint: tick 512\n\
             eDRF: playback of game.edrf FAILED\n"
        );

        let round_end = Outcome::Failed(Failure::Divergence(Divergence::RoundEnd {
            tick: 1378,
            expected: 0x4D35_B2CC,
            actual: 0xB34D_5617,
        }));
        let report_round_end = ReplayReport {
            last_good_checkpoint: None,
            ..report(RecordingKind::Edrf, round_end)
        };
        let text = report_round_end.diagnostics("r.edrf");
        assert!(
            text.contains("before tick 1378 (end of round state B34D5617, recorded 4D35B2CC)"),
            "{text}"
        );
        assert!(text.contains("last matching checkpoint: none\n"), "{text}");

        let end = Outcome::Failed(Failure::Divergence(Divergence::End {
            expected: EndRecord {
                ticks: 4489,
                hash: 0x1122_3344,
            },
            actual: EndRecord {
                ticks: 4490,
                hash: 0x0000_00AB,
            },
        }));
        let text = report(RecordingKind::Edrf, end).diagnostics("e.edrf");
        assert!(
            text.starts_with(
                "eDRF: the game ends differently from the recording \
                 (expected 4489/11223344, got 4490/000000AB)\n"
            ),
            "{text}"
        );

        let level = Outcome::Failed(Failure::Playback(PlaybackFailure::LevelMismatch {
            started: crate::LevelNumber::FIRST,
            recorded: 4,
        }));
        let text = report(RecordingKind::Edrf, level).diagnostics("l.edrf");
        assert!(
            text.starts_with("eDRF: level 1 starts, recorded level 4\n"),
            "{text}"
        );

        let out_of_step = Outcome::Failed(Failure::Playback(PlaybackFailure::OutOfStep {
            tick: 99,
            problem: OutOfStep::NoEol { offset: 1234 },
        }));
        let text = report(RecordingKind::Drf, out_of_step).diagnostics("o.drf");
        assert_eq!(
            text,
            "DRF: the playback is out of step with the recording at the round end \
             on tick 99: no EOL at offset 1234\n\
             DRF: playback of o.drf FAILED\n"
        );

        assert_eq!(
            report(RecordingKind::Edrf, Outcome::Stopped).diagnostics("s.edrf"),
            "eDRF: playback stopped on tick 1378, before the end of the recording\n"
        );
        assert_eq!(
            report(RecordingKind::Edrf, Outcome::Truncated).diagnostics("t.edrf"),
            "eDRF: the recording ends on tick 1378, before its game does\n"
        );
    }
}
