//! The playback sources and [`ReplayObserver`], the one [`Observer`] that
//! playback, recording, verification and tracing are composed in (design
//! D4 and D8 of `add-recording-and-conformance`).

use core::fmt;

use crate::recording::{
    ControlRuns, ControlValue, DrfCursor, DrfProblem, DrfRecording, DrfStep, EDRF_SLOTS,
    EdrfRecording, OutOfStep, Recording, RecordingError,
};
use crate::{
    Controls, GameConfig, LevelLayout, LevelNumber, LevelSet, Movement, Observer, PlayerId, SlotId,
    StateView,
};

use super::recorder::Recorder;
use super::trace::TraceSink;
use super::verifier::Verifier;

/// The game configuration a recording is played back under, built from
/// the recording alone (game-recording spec, "Playback settings come only
/// from the recording"; design D7).
///
/// The mode, the gauntlet time and the start level come from the mode
/// line, the bonus-life interval from the bonus line, and for a DRF the
/// eight level plans from its header maps. Unlimited lives is always off
/// (quirk P9: a `U` in the mode line is accepted and ignored). There is
/// no parameter through which a caller's own settings could arrive.
///
/// Mode combinations the simulation refuses (two alternating players with
/// two diggers or with gauntlet) cannot be written in the mode line's
/// grammar and are refused when the header is read, so every recording
/// has a playable configuration.
///
/// A DRF's maps are not used verbatim: with two diggers the start cells
/// of plans 3 and 4 are opened as in live play (quirk P6). An eDRF's maps
/// arrive level by level through its `L` records instead, which are used
/// exactly as recorded; until one arrives the built-in plans stand in.
pub fn playback_config(recording: &Recording) -> GameConfig {
    let header = recording.header();
    let bonus = header.bonus_score();
    let plans = match recording {
        Recording::Drf(drf) => *drf.maps(),
        Recording::Edrf(_) => *LevelSet::builtin().layouts(),
    };
    GameConfig {
        mode: header.mode(),
        start_level: header.start_level(),
        // Quirk P9: playback never has unlimited lives.
        unlimited_lives: false,
        levels: LevelSet::from_plans(plans, bonus, false),
    }
}

/// Why a playback no longer follows its recording (replay-verification
/// spec, "Playback outcome classification"). Checkpoint mismatches are
/// the verifier's; these are the ones the playback source finds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PlaybackFailure {
    /// An eDRF level record names another level than the one starting
    /// (game-recording spec, "eDRF inline level maps").
    LevelMismatch {
        /// The level the game starts.
        started: LevelNumber,
        /// The level the recording has.
        recorded: u32,
    },
    /// A DRF round ended where the recording does not end it (game-recording
    /// spec, "DRF round boundaries and end of data").
    OutOfStep {
        /// The tick count when the round ended.
        tick: u32,
        /// How the recording disagrees.
        problem: OutOfStep,
    },
    /// The DRF body is malformed where the playback read it.
    BadDrfBody {
        /// The tick count when it was read.
        tick: u32,
        /// The reader's error.
        error: RecordingError,
    },
}

impl fmt::Display for PlaybackFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LevelMismatch { started, recorded } => {
                write!(
                    f,
                    "level {} starts, recorded level {recorded}",
                    started.get()
                )
            }
            Self::OutOfStep { tick, problem } => {
                write!(
                    f,
                    "the playback is out of step with the recording at the round end on tick {tick}: "
                )?;
                match problem {
                    OutOfStep::RepeatsPending { remaining } => {
                        write!(f, "{remaining} repeats of a run are still pending")
                    }
                    OutOfStep::NoEol { offset } => {
                        write!(f, "no EOL at offset {offset}")
                    }
                }
            }
            Self::BadDrfBody { tick, error } => write!(f, "{error} (on tick {tick})"),
        }
    }
}

impl std::error::Error for PlaybackFailure {}

/// A DRF being played: the recording and the reading position in its
/// body (design D3).
#[derive(Debug, Clone)]
struct DrfPlayback {
    recording: Box<DrfRecording>,
    cursor: DrfCursor,
    /// Set when a control read found the end marker or the end of the
    /// data, or the body could not be read: the game is quit on the next
    /// frame (quirk P2).
    ended: bool,
}

/// An eDRF being played: what is left of its queues (design D3).
#[derive(Debug, Clone)]
struct EdrfPlayback {
    recording: Box<EdrfRecording>,
    controls: [ControlRuns; EDRF_SLOTS],
    next_seed: usize,
    next_level: usize,
}

/// The format-specific part of a playback source.
#[derive(Debug, Clone)]
enum Feed {
    Drf(DrfPlayback),
    Edrf(EdrfPlayback),
}

/// Where the values of a played-back game come from: a DRF or an eDRF
/// recording. It replaces the values the simulation would otherwise use
/// (design D4).
#[derive(Debug, Clone)]
pub(crate) struct PlaybackSource {
    feed: Feed,
    /// The first failure found, if any.
    failure: Option<PlaybackFailure>,
    /// Set when the caller stops the playback before the recording ends:
    /// a round cut short then is not out of step.
    stopping: bool,
    /// The tick count of the last tick that ended.
    tick: u32,
}

impl PlaybackSource {
    /// A source playing `recording` from its start.
    pub(crate) fn new(recording: &Recording) -> Self {
        let feed = match recording {
            Recording::Drf(drf) => Feed::Drf(DrfPlayback {
                recording: drf.clone(),
                cursor: DrfCursor::new(),
                ended: false,
            }),
            Recording::Edrf(edrf) => Feed::Edrf(EdrfPlayback {
                controls: SlotId::ALL.map(|slot| edrf.controls(slot).clone()),
                recording: edrf.clone(),
                next_seed: 0,
                next_level: 0,
            }),
        };
        Self {
            feed,
            failure: None,
            stopping: false,
            tick: 0,
        }
    }

    /// Whether the recording has nothing more to play, so that the game is
    /// quit at the start of the next frame: for an eDRF when every slot's
    /// values are used up (game-recording spec, "End of an eDRF
    /// recording"), for a DRF when a control read found the end marker
    /// or the end of the data (quirk P2), or the body could not be read.
    ///
    /// An eDRF's other records are read into their queues before play
    /// starts, so "no record remains to be read" holds throughout and only
    /// the control queues decide; this is what upstream's lazy reader
    /// finds when its input queues are empty and the file has no line
    /// left.
    pub(crate) fn exhausted(&self) -> bool {
        match &self.feed {
            Feed::Drf(drf) => drf.ended,
            Feed::Edrf(edrf) => edrf.controls.iter().all(ControlRuns::is_empty),
        }
    }

    /// The first failure found, if any.
    pub(crate) const fn failure(&self) -> Option<&PlaybackFailure> {
        self.failure.as_ref()
    }

    /// Tells the source that the caller is ending the game before the
    /// recording does.
    pub(crate) const fn stop(&mut self) {
        self.stopping = true;
    }

    /// Records `failure` unless an earlier one was found.
    fn fail(&mut self, failure: PlaybackFailure) {
        if self.failure.is_none() {
            self.failure = Some(failure);
        }
    }
}

impl Observer for PlaybackSource {
    fn round_seed(&mut self, _round: u32, derived: i32) -> i32 {
        let tick = self.tick;
        match &mut self.feed {
            Feed::Drf(drf) => match drf.cursor.next_seed(&drf.recording) {
                Ok(seed) => seed.cast_signed(),
                Err(RecordingError::BadDrfBody {
                    reason: DrfProblem::SeedCutOff | DrfProblem::BadSeed { byte: b'E' | b'e' },
                    ..
                }) => {
                    // The game outlives the recording: it ended here (by
                    // Exit, `EOG`, or the file was cut). Nothing is read
                    // out of bounds; the game is quit on the next frame.
                    drf.ended = true;
                    derived
                }
                Err(error) => {
                    drf.ended = true;
                    self.fail(PlaybackFailure::BadDrfBody { tick, error });
                    derived
                }
            },
            Feed::Edrf(edrf) => {
                // Upstream plays a round the recording has no `R` for
                // with seed 0.
                let seed = edrf.recording.seeds().get(edrf.next_seed).copied();
                edrf.next_seed = edrf.next_seed.saturating_add(1);
                seed.unwrap_or(0).cast_signed()
            }
        }
    }

    fn level_plan(
        &mut self,
        _player: PlayerId,
        level: LevelNumber,
        _layout: &LevelLayout,
    ) -> Option<LevelLayout> {
        let Feed::Edrf(edrf) = &mut self.feed else {
            // A DRF's maps are the configuration's level set (quirk P6).
            return None;
        };
        let recorded = *edrf.recording.levels().get(edrf.next_level)?;
        edrf.next_level = edrf.next_level.saturating_add(1);
        if recorded.level != u32::from(level.get()) {
            self.fail(PlaybackFailure::LevelMismatch {
                started: level,
                recorded: recorded.level,
            });
        }
        // Quirk P6: the recorded map is used exactly as it is, also after
        // a mismatch, as upstream does.
        Some(recorded.layout)
    }

    fn controls(&mut self, _tick: u32, slot: SlotId, _raw: Controls) -> Controls {
        match &mut self.feed {
            // Quirk P3: during DRF playback the raw controls are all off,
            // so on a tick without a control read the next tick's fire is
            // "not pressed"; the record of a control read supplies the
            // direction and the fire.
            Feed::Drf(_) => Controls::NONE,
            // Quirks P4 and P5: the recorded raw value goes through the
            // direction resolver like a live one, and every slot uses up
            // one value on every tick, read or not.
            Feed::Edrf(edrf) => slot
                .of_mut(&mut edrf.controls)
                .next_value()
                .map_or(Controls::NONE, ControlValue::controls),
        }
    }

    fn movement(&mut self, _slot: SlotId, sample: Movement) -> Movement {
        let tick = self.tick.wrapping_add(1);
        let Feed::Drf(drf) = &mut self.feed else {
            return sample;
        };
        if drf.ended {
            return Movement::default();
        }
        // Quirk P1: one record per control read, the diggers of a tick in
        // order, from the one stream.
        match drf.cursor.next_record(&drf.recording) {
            Ok(DrfStep::Record(record)) => record.into(),
            Ok(DrfStep::End) => {
                // Quirk P2: no direction and no fire; the tick completes
                // and counts, and the game is quit on the next frame.
                drf.ended = true;
                Movement::default()
            }
            Err(error) => {
                drf.ended = true;
                self.fail(PlaybackFailure::BadDrfBody { tick, error });
                Movement::default()
            }
        }
    }

    fn tick_end(&mut self, tick: u32, _state: StateView<'_>) {
        self.tick = tick;
    }

    fn round_end(&mut self, state: StateView<'_>) {
        let skip = self.failure.is_some() || self.stopping;
        let Feed::Drf(drf) = &mut self.feed else {
            return;
        };
        if skip {
            return;
        }
        let in_step = drf.cursor.end_round(&drf.recording);
        if drf.ended {
            // The recording ended inside this round; its `EOL` (if any)
            // is consumed, and nothing is left to be in step with.
            return;
        }
        if let Err(problem) = in_step {
            drf.ended = true;
            self.fail(PlaybackFailure::OutOfStep {
                tick: state.ticks(),
                problem,
            });
        }
    }
}

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

    /// An observer playing `recording` back, with no recorder, verifier
    /// or trace sink.
    pub(crate) fn playing(recording: &Recording) -> Self {
        Self {
            source: Some(PlaybackSource::new(recording)),
            ..Self::default()
        }
    }

    /// The playback source, if there is one.
    pub(crate) const fn source(&self) -> Option<&PlaybackSource> {
        self.source.as_ref()
    }

    /// The playback source, if there is one, for changing it.
    pub(crate) const fn source_mut(&mut self) -> Option<&mut PlaybackSource> {
        self.source.as_mut()
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

#[cfg(test)]
#[allow(
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::*;
    use crate::recording::{HeaderProblem, parse};
    use crate::{GameMode, GauntletSeconds, StartLevel};

    /// An eDRF holding only a header with this mode line.
    fn edrf(mode: &str) -> Result<Recording, RecordingError> {
        parse(format!("eDRF 2\nRS DIG 20261111\n{mode}\n20000\n").as_bytes())
    }

    /// A DRF with this mode line and bonus score, plan 1 marked by an
    /// emerald in its top-left cell.
    fn drf(mode: &str, bonus: &str) -> Result<Recording, RecordingError> {
        let mut text = format!("DRF\nAJ DOS 19990506\n{mode}\n{bonus}\nC\n");
        text.push_str(&"\n".repeat(79));
        text.push_str("00000000\nEOL\n");
        parse(text.as_bytes())
    }

    fn start(level: i64) -> StartLevel {
        StartLevel::new(level).unwrap_or_else(|_| unreachable!("a valid start level"))
    }

    #[test]
    fn config_from_header_modes() {
        let time = |seconds| GauntletSeconds::new(seconds);
        let cases = [
            ("1", GameMode::OnePlayer, 1),
            ("2", GameMode::TwoPlayers, 1),
            ("M2", GameMode::TwoDiggers, 1),
            ("M1", GameMode::OnePlayer, 1),
            ("G120", GameMode::Gauntlet { time: time(120) }, 1),
            (
                "M2G300I5",
                GameMode::GauntletTwoDiggers { time: time(300) },
                5,
            ),
            ("2I3", GameMode::TwoPlayers, 3),
            ("I1000", GameMode::OnePlayer, 1000),
            ("", GameMode::OnePlayer, 1),
        ];
        for (line, mode, level) in cases {
            let recording = edrf(line).unwrap_or_else(|e| panic!("{line:?}: {e}"));
            let config = playback_config(&recording);
            assert_eq!(config.mode, mode, "{line:?}");
            assert_eq!(config.start_level, start(level), "{line:?}");
            assert!(!config.unlimited_lives, "{line:?}");
            assert_eq!(config.levels.bonus_step(), 20000, "{line:?}");
            assert_eq!(config.levels.layouts(), LevelSet::builtin().layouts());
            assert!(!config.levels.is_verbatim(), "{line:?}");
        }

        // The bonus interval and, for a DRF, the header maps.
        let recording = drf("M2G60", "7500").unwrap_or_else(|e| panic!("{e}"));
        let config = playback_config(&recording);
        assert_eq!(config.mode, GameMode::GauntletTwoDiggers { time: time(60) });
        assert_eq!(config.levels.bonus_step(), 7500);
        let Recording::Drf(inner) = &recording else {
            unreachable!("a DRF")
        };
        assert_eq!(config.levels.layouts(), inner.maps());
        assert_eq!(config.levels.layouts()[0].rows()[0][0], b'C');
        assert!(
            !config.levels.is_verbatim(),
            "the two-digger start cells are opened in DRF playback (P6)"
        );

        // Combinations the simulation refuses never reach a configuration:
        // the header refuses them, naming the mode line.
        for line in [
            "2M2", "2G120", "M2 ", "M3", "G0", "G4000", "I0", "I1001", "3",
        ] {
            let error = edrf(line).err();
            assert!(
                matches!(
                    error,
                    Some(RecordingError::BadHeader {
                        line: 3,
                        reason: HeaderProblem::BadModeLine { .. }
                    })
                ),
                "{line:?}: {error:?}"
            );
        }
    }

    #[test]
    fn mode_u_ignored() {
        for (with_u, without) in [
            ("1U", "1"),
            ("2UI3", "2I3"),
            ("M2G300UI5", "M2G300I5"),
            ("U", ""),
        ] {
            let a = edrf(with_u).unwrap_or_else(|e| panic!("{with_u:?}: {e}"));
            let b = edrf(without).unwrap_or_else(|e| panic!("{without:?}: {e}"));
            let config = playback_config(&a);
            assert_eq!(config, playback_config(&b), "{with_u:?}");
            assert!(!config.unlimited_lives, "{with_u:?}: never unlimited lives");
        }
        let config = playback_config(&drf("1U", "20000").unwrap_or_else(|e| panic!("{e}")));
        assert!(!config.unlimited_lives);
    }
}
