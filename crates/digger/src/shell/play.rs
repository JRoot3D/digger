//! The live game and playback scenes, with pause and resume requests
//! (design D2).
//!
//! **Pause goes through the core.** The pause action sets the core's pause
//! request, and the core holds the game in its own pause phase: no tick
//! elapses, the gauntlet clock stands still, the core shows `PRESS ANY KEY`
//! at (80, 0) and emits the pause and resume sound commands. The core
//! honours a request on a main-phase tick only, so the scene keeps asking
//! on every frame until the game reports that it is paused (a level draw or
//! a wind-down ignores the request). Any new key or button press while
//! paused asks again, which resumes; that press is consumed: the controls
//! it holds stay off until they are released, so it never acts as a game
//! control.
//!
//! A live game also pauses when the window loses focus or a pad is removed
//! (`desktop-app` spec, losing focus; `input-controls` spec, gamepad
//! hot-plug); a playback keeps running.
//!
//! Quitting, or closing the window, ends the game at once through the
//! core's quit request, so the recording gets its end-of-game marker; quit
//! returns to the title without `GAME OVER`, close ends the program.
//!
//! When a step reports that a player is out, the scene hands over to the
//! game-over sequence ([`GameOver`]): it suspends the game while a
//! new-high-score screen is shown for each qualifying digger, then resumes
//! stepping on the frame the last entry is complete, so the core's
//! 50-frame `GAME OVER` or `TIME UP` notice follows. Quit and close work
//! there too and add no entry.

use std::path::{Path, PathBuf};

use digger_core::recording::{Recording, RecordingError, RecordingKind};
use digger_core::replay::{Recorder, Replay, ReplayOptions, ReplayReport, ReplayStep};
use digger_core::video::Framebuffer;
use digger_core::{
    Controls, FrameInput, FrameKind, Game, GameConfig, GameSummary, SoundCommand, StepOutcome,
};

use super::game_over::{GameOver, GameOverStep};
use super::initials::InitialsScene;
use super::recording::{FinishedRecording, LiveRecording};
use super::{Context, ShellInput, Transition};
use crate::config::GameMode;
use crate::input::{Action, PadChange};
use crate::notice::Notice;
use crate::scores::TableKey;

/// A frame that only asks the core to quit the game.
const QUIT: FrameInput = FrameInput {
    quit: true,
    ..FrameInput::NONE
};

/// Where the scene stands with the core's pause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PauseState {
    /// The game runs.
    Running,
    /// A pause was asked for and the core has not honoured it yet.
    Requested,
    /// The core holds the game in its pause.
    Paused,
}

/// The pause requests of one scene and the controls consumed by the
/// resume press.
#[derive(Debug, Clone)]
struct Pause {
    state: PauseState,
    /// Controls held by the press that resumed, kept off until released.
    consumed: [Controls; 2],
}

/// What the pause decided for one frame.
#[derive(Debug, Clone, Copy)]
struct PauseFrame {
    /// The core's pause request for this frame.
    request: bool,
    /// The controls to pass on.
    slots: [Controls; 2],
}

impl Pause {
    const fn new() -> Self {
        Self {
            state: PauseState::Running,
            consumed: [Controls::NONE; 2],
        }
    }

    /// The pause request and controls of a frame with `input`; `live` when
    /// focus loss and pad removal pause too.
    fn before_step(&mut self, input: &ShellInput, live: bool) -> PauseFrame {
        let frame = &input.input;
        let mut request = false;
        match self.state {
            PauseState::Paused => {
                if frame.any_key || frame.any_button {
                    // The resume press is consumed: what it holds stays
                    // off until released.
                    for (consumed, held) in self.consumed.iter_mut().zip(frame.slots) {
                        *consumed = union(*consumed, held);
                    }
                    request = true;
                }
            }
            PauseState::Requested => request = true,
            PauseState::Running => {
                let lost = live
                    && (frame.focus_lost
                        || frame
                            .pad_changes
                            .iter()
                            .any(|change| matches!(change, PadChange::Removed(_))));
                if lost || input.has(Action::Pause) {
                    self.state = PauseState::Requested;
                    request = true;
                }
            }
        }
        let mut slots = frame.slots;
        for (slot, consumed) in slots.iter_mut().zip(self.consumed.iter_mut()) {
            *slot = consume(*slot, consumed);
        }
        PauseFrame { request, slots }
    }

    /// Notes whether the game is paused after the step.
    const fn after_step(&mut self, paused: bool) {
        self.state = match (self.state, paused) {
            (_, true) => PauseState::Paused,
            (PauseState::Requested, false) => PauseState::Requested,
            (_, false) => PauseState::Running,
        };
    }
}

/// The controls held in `a` or `b`.
const fn union(a: Controls, b: Controls) -> Controls {
    Controls {
        up: a.up || b.up,
        down: a.down || b.down,
        left: a.left || b.left,
        right: a.right || b.right,
        fire: a.fire || b.fire,
    }
}

/// `held` without the controls in `consumed`; a consumed control that is
/// no longer held is released from `consumed`.
fn consume(held: Controls, consumed: &mut Controls) -> Controls {
    let pass = |held: bool, consumed: &mut bool| {
        if !held {
            *consumed = false;
        }
        held && !*consumed
    };
    Controls {
        up: pass(held.up, &mut consumed.up),
        down: pass(held.down, &mut consumed.down),
        left: pass(held.left, &mut consumed.left),
        right: pass(held.right, &mut consumed.right),
        fire: pass(held.fire, &mut consumed.fire),
    }
}

/// A game played by a person (design D2, D11).
#[derive(Debug)]
pub(crate) struct PlayScene {
    /// The game.
    game: Box<Game>,
    /// The game's recording, from its first frame.
    recording: Option<LiveRecording>,
    /// The pause requests.
    pause: Pause,
    /// How many frames were stepped.
    frames: u64,
    /// What the last step reported.
    last_outcome: Option<StepOutcome>,
    /// The game's mode.
    mode: GameMode,
    /// The score table the game's entries go to.
    table: TableKey,
    /// The game-over sequence while it shows a new-high-score screen.
    game_over: Option<GameOver>,
}

impl PlayScene {
    /// A new game under `config` with base seed `seed`, recorded as
    /// `kind`; it is played in `mode` and its high scores go to the table
    /// named by `table`.
    pub(crate) fn new(
        config: GameConfig,
        seed: u64,
        kind: RecordingKind,
        mode: GameMode,
        table: TableKey,
    ) -> Self {
        let recorder = Recorder::new(kind, &config, false);
        Self {
            game: Game::new(config, seed),
            recording: Some(LiveRecording::new(recorder)),
            pause: Pause::new(),
            frames: 0,
            last_outcome: None,
            mode,
            table,
            game_over: None,
        }
    }

    /// Runs one frame: exactly one step of the game, unless a
    /// new-high-score screen holds it.
    pub(crate) fn frame(&mut self, input: &ShellInput, cx: &mut Context<'_>) -> Transition {
        if input.close_requested {
            self.end(&QUIT, cx);
            return Transition::Exit(0);
        }
        if input.has(Action::Quit) {
            self.end(&QUIT, cx);
            return to_title(cx);
        }
        if let Some(over) = self.game_over.as_mut() {
            match over.frame(&input.input, cx) {
                GameOverStep::Entering => return Transition::Stay,
                GameOverStep::Entered(entry) => {
                    if let Some(recording) = self.recording.as_mut() {
                        recording.add_initials(entry);
                    }
                    if !over.is_done() {
                        return Transition::Stay;
                    }
                    // The last entry is in: the game resumes on this
                    // frame with the core's notice.
                    self.game_over = None;
                }
            }
        }
        let pause = self.pause.before_step(input, true);
        let frame = FrameInput {
            slots: pause.slots,
            pause: pause.request,
            quit: false,
        };
        let outcome = self.step(&frame, cx);
        self.pause.after_step(self.game.is_paused());
        if let Some(summary) = outcome.finished {
            self.finish(summary, cx);
            return to_title(cx);
        }
        if let Some(out) = outcome.player_out {
            self.game_over = GameOver::begin(&self.game, out, self.mode, self.table.clone(), cx);
            if self.game_over.is_some() {
                // All sound stops while the new-high-score screen is
                // shown (`high-scores` spec, new high score screen).
                cx.sound([SoundCommand::StopAll]);
            }
        }
        Transition::Stay
    }

    /// Steps the game once with `input` and forwards its sound.
    fn step(&mut self, input: &FrameInput, cx: &mut Context<'_>) -> StepOutcome {
        let outcome = match self.recording.as_mut() {
            Some(recording) => {
                let outcome = self.game.step_observed(input, recording.recorder_mut());
                recording.collect();
                outcome
            }
            None => self.game.step(input),
        };
        self.frames += 1;
        self.last_outcome = Some(outcome);
        cx.sound(self.game.drain_sound());
        outcome
    }

    /// Ends the game with one more step of `input` (a quit request) and
    /// keeps its recording. An entry being typed is dropped.
    fn end(&mut self, input: &FrameInput, cx: &mut Context<'_>) {
        self.game_over = None;
        let outcome = self.step(input, cx);
        if let Some(summary) = outcome.finished {
            self.finish(summary, cx);
        }
    }

    /// Hands the finished recording to the shell.
    fn finish(&mut self, summary: GameSummary, cx: &mut Context<'_>) {
        if let Some(recording) = self.recording.take() {
            cx.finished(recording.finish(summary));
        }
    }

    /// The picture to present: the new-high-score screen while one is
    /// shown, otherwise the game's own.
    pub(crate) fn picture(&self) -> &Framebuffer {
        match self.initials() {
            Some(screen) => screen.picture(),
            None => self.game.framebuffer(),
        }
    }

    /// The new-high-score screen being shown, if any.
    pub(crate) fn initials(&self) -> Option<&InitialsScene> {
        self.game_over.as_ref().and_then(GameOver::screen)
    }

    /// The game.
    pub(crate) fn game(&self) -> &Game {
        &self.game
    }

    /// How many frames were stepped.
    pub(crate) const fn frames(&self) -> u64 {
        self.frames
    }

    /// What the last step reported.
    pub(crate) const fn last_outcome(&self) -> Option<StepOutcome> {
        self.last_outcome
    }
}

/// Back to the title screen; the sound of the game stops.
fn to_title(cx: &mut Context<'_>) -> Transition {
    cx.sound([SoundCommand::StopAll]);
    Transition::To(cx.title())
}

/// What a playback is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Purpose {
    /// Shown for watching (`--play`, or a test): when it ends the program
    /// goes on with the next recording of the list or the title screen.
    Watch,
    /// `--verify`: when it ends the program ends with its status.
    Verify,
}

/// How a playback is left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Leave {
    /// The recorded game ended.
    End,
    /// The quit action.
    Quit,
    /// The window was closed.
    Close,
}

/// The playback itself, until it ends; then the last picture it showed.
#[derive(Debug)]
enum Feed {
    /// The playback runs.
    Playing(Box<Replay>),
    /// The playback has ended and was reported.
    Ended(Box<Framebuffer>),
}

/// A recording being played back (design D2, D11): `--play`, `--verify`
/// and `--take-over-at`.
///
/// Player movement and fire controls are ignored and focus loss does not
/// pause; pause and quit work, and the shell applies speed, sound, music
/// and fullscreen as in every scene. The game reports no player result, so
/// it never reaches high-score entry; the core's own closing notice is
/// shown when the recorded game ends.
///
/// With a take-over tick, the game is handed to the player once that many
/// ticks have been played: from the next tick the player's controls drive
/// it and losing focus pauses it. Such a playback is recorded from its
/// first frame, so the recording of a taken-over game covers both parts;
/// it is handed to the shell like a live game's (written to the `--record`
/// file, or kept for the save-recording action).
///
/// A verification run ends the program with the replay's status, after
/// writing its re-recording to the `--record` file if one was given.
#[derive(Debug)]
pub(crate) struct PlaybackScene {
    /// The playback.
    feed: Feed,
    /// What it is for.
    purpose: Purpose,
    /// The tick after which the player takes over.
    take_over_at: Option<u32>,
    /// The recording file, for a playback of the `--play` list.
    source: Option<PathBuf>,
    /// The pause requests.
    pause: Pause,
    /// What the last step reported.
    last_step: Option<ReplayStep>,
}

impl PlaybackScene {
    /// A playback of `recording` from its first frame, with `options`
    /// (a take-over tick, for example), that records nothing.
    pub(crate) fn with_options(
        recording: &Recording,
        options: ReplayOptions,
    ) -> Result<Self, RecordingError> {
        let replay = Replay::new(recording, options)?;
        Ok(Self::of(
            Ok(replay),
            Purpose::Watch,
            options.take_over_after,
        ))
    }

    /// The playback of `recording` from the `--play` file `source`,
    /// handed to the player after tick `take_over_at` if given; a playback
    /// that can be taken over is recorded as `kind`.
    pub(crate) fn watch(
        recording: &Recording,
        source: &Path,
        take_over_at: Option<u32>,
        kind: RecordingKind,
    ) -> Self {
        let options = ReplayOptions {
            take_over_after: take_over_at,
            ..ReplayOptions::default()
        };
        let replay = match take_over_at {
            Some(_) => Replay::recording_as(recording, options, kind),
            None => Replay::new(recording, options),
        };
        let mut scene = Self::of(replay, Purpose::Watch, take_over_at);
        scene.source = Some(source.to_path_buf());
        scene
    }

    /// The `--verify` playback of `recording`, re-recorded as `record` if
    /// given.
    pub(crate) fn verify(recording: &Recording, record: Option<RecordingKind>) -> Self {
        let options = ReplayOptions::default();
        let replay = match record {
            Some(kind) => Replay::recording_as(recording, options, kind),
            None => Replay::new(recording, options),
        };
        Self::of(replay, Purpose::Verify, None)
    }

    /// The scene over `replay`, which the core builds from any parsed
    /// recording.
    fn of(
        replay: Result<Replay, RecordingError>,
        purpose: Purpose,
        take_over_at: Option<u32>,
    ) -> Self {
        // `Replay::new` refuses no parsed recording; should a later core
        // refuse one, the scene shows a blank screen and ends at once.
        let feed = match replay {
            Ok(replay) => Feed::Playing(Box::new(replay)),
            Err(error) => {
                crate::report::notice(format!("cannot play back the recording: {error}"));
                Feed::Ended(Box::new(Framebuffer::new()))
            }
        };
        Self {
            feed,
            purpose,
            take_over_at,
            source: None,
            pause: Pause::new(),
            last_step: None,
        }
    }

    /// Runs one frame: exactly one step of the playback.
    pub(crate) fn frame(&mut self, input: &ShellInput, cx: &mut Context<'_>) -> Transition {
        let Feed::Playing(replay) = &mut self.feed else {
            // Only a verification run stays on an ended playback, for the
            // frame that asks the platform to exit.
            return match self.purpose {
                Purpose::Verify => Transition::Stay,
                Purpose::Watch => cx.after_playback(),
            };
        };
        if input.close_requested {
            step(replay, &QUIT, &mut self.last_step, cx);
            return self.end(Leave::Close, cx);
        }
        if input.has(Action::Quit) {
            step(replay, &QUIT, &mut self.last_step, cx);
            return self.end(Leave::Quit, cx);
        }
        // The hand-over takes effect on the frame whose tick is the first
        // after the take-over tick, so that this frame's controls already
        // drive it (`configuration` spec, taking over a playback).
        if let Some(after) = self.take_over_at
            && !replay.taken_over()
            && replay.game().state().ticks() >= after
        {
            replay.request_take_over();
        }
        // Once taken over, the game is a live one: the player's controls
        // drive it and losing focus pauses it. It still never enters the
        // score tables, as a playback reports no player result.
        let taken_over = replay.taken_over();
        let pause = self.pause.before_step(input, taken_over);
        // Before that the recording supplies every control; only the
        // pause request goes in.
        let frame = FrameInput {
            slots: if taken_over {
                pause.slots
            } else {
                [Controls::NONE; 2]
            },
            pause: pause.request,
            quit: false,
        };
        let outcome = step(replay, &frame, &mut self.last_step, cx);
        self.pause.after_step(replay.game().is_paused());
        match outcome {
            ReplayStep::Finished(_) => self.end(Leave::End, cx),
            ReplayStep::Running(_) => Transition::Stay,
        }
    }

    /// Ends the playback, which has finished or was just quit, and decides
    /// what follows.
    fn end(&mut self, leave: Leave, cx: &mut Context<'_>) -> Transition {
        let picture = Box::new(self.picture().clone());
        let Feed::Playing(mut replay) = std::mem::replace(&mut self.feed, Feed::Ended(picture))
        else {
            return Transition::Stay;
        };
        // The game has finished: the step before ended it, or was a quit
        // request, which the core honours at once. Should it not have,
        // quitting until it does keeps the report complete.
        let summary = loop {
            if let ReplayStep::Finished(summary) =
                replay.step(&QUIT, &mut digger_core::NoopObserver)
            {
                break summary;
            }
        };
        cx.sound(replay.drain_sound());
        let taken_over = replay.taken_over();
        let kind = replay.recorder_mut().map(|recorder| recorder.kind());
        let (report, text) = replay.finish_with_recording();
        let recording = kind.zip(text).map(|(kind, text)| FinishedRecording {
            kind,
            text,
            summary,
            entry: None,
        });
        match self.purpose {
            Purpose::Verify => {
                // The re-recording goes to the `--record` file before the
                // program ends (`configuration` spec, play-and-verify).
                if let Some(recording) = recording {
                    cx.finished(recording);
                }
                let status = report.exit_status();
                *cx.verified = Some(report);
                cx.sound([SoundCommand::StopAll]);
                Transition::Exit(status)
            }
            Purpose::Watch => {
                // A game the player took over is a game played by a
                // person: its recording is kept like a live game's.
                if taken_over && let Some(recording) = recording {
                    cx.finished(recording);
                }
                match leave {
                    Leave::Close => Transition::Exit(0),
                    // Quitting a playback ends the `--play` list, the loop
                    // included, and shows the title screen (compat quirk
                    // SH11).
                    Leave::Quit => {
                        cx.playlist.stop();
                        to_title(cx)
                    }
                    Leave::End => {
                        if !taken_over {
                            self.report_end(&report, cx);
                        }
                        cx.after_playback()
                    }
                }
            }
        }
    }

    /// Keeps the result of a watched playback that ran to its end for the
    /// page, and tells the player when it did not match its recording
    /// (`web-build` spec, loading a recording through a file picker): the
    /// first line of the divergence report names the tick.
    fn report_end(&self, report: &ReplayReport, cx: &mut Context<'_>) {
        let name = self
            .source
            .as_deref()
            .map_or_else(String::new, |path| path.display().to_string());
        let result = if report.exit_status() == 0 {
            report.result_line().trim_end().to_owned()
        } else {
            let diagnostics = report.diagnostics(&name);
            let first = diagnostics
                .lines()
                .next()
                .unwrap_or("playback failed")
                .to_owned();
            cx.notices.push(Notice::PlaybackFailed {
                name,
                reason: first.clone(),
            });
            first
        };
        *cx.played = Some(result);
    }

    /// The picture to present: the game's own.
    pub(crate) fn picture(&self) -> &Framebuffer {
        match &self.feed {
            Feed::Playing(replay) => replay.game().framebuffer(),
            Feed::Ended(picture) => picture,
        }
    }

    /// The game being played back, until the playback has ended.
    pub(crate) fn game(&self) -> Option<&Game> {
        match &self.feed {
            Feed::Playing(replay) => Some(replay.game()),
            Feed::Ended(_) => None,
        }
    }

    /// Whether the player has taken the game over.
    pub(crate) fn taken_over(&self) -> bool {
        matches!(&self.feed, Feed::Playing(replay) if replay.taken_over())
    }

    /// The `--play` file being played, if the playback is one of the list.
    pub(crate) fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    /// The kind of the last frame, if it ran.
    pub(crate) const fn last_frame(&self) -> Option<FrameKind> {
        match self.last_step {
            Some(ReplayStep::Running(kind)) => Some(kind),
            _ => None,
        }
    }
}

/// Steps `replay` once with `input`, notes the step and forwards its
/// sound.
fn step(
    replay: &mut Replay,
    input: &FrameInput,
    last_step: &mut Option<ReplayStep>,
    cx: &mut Context<'_>,
) -> ReplayStep {
    let outcome = replay.step(input, &mut digger_core::NoopObserver);
    *last_step = Some(outcome);
    cx.sound(replay.drain_sound());
    outcome
}
