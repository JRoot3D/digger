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

use digger_core::recording::{Recording, RecordingError, RecordingKind};
use digger_core::replay::{Recorder, Replay, ReplayOptions, ReplayStep};
use digger_core::video::Framebuffer;
use digger_core::{
    Controls, FrameInput, FrameKind, Game, GameConfig, GameSummary, SoundCommand, StepOutcome,
};

use super::recording::LiveRecording;
use super::{Context, Scene, ShellInput, TitleScene, Transition};
use crate::input::{Action, PadChange};

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
}

impl PlayScene {
    /// A new game under `config` with base seed `seed`, recorded as
    /// `kind`.
    pub(crate) fn new(config: GameConfig, seed: u64, kind: RecordingKind) -> Self {
        let recorder = Recorder::new(kind, &config, false);
        Self {
            game: Game::new(config, seed),
            recording: Some(LiveRecording::new(recorder)),
            pause: Pause::new(),
            frames: 0,
            last_outcome: None,
        }
    }

    /// Runs one frame: exactly one step of the game.
    pub(crate) fn frame(&mut self, input: &ShellInput, cx: &mut Context<'_>) -> Transition {
        if input.close_requested {
            self.end(&QUIT, cx);
            return Transition::Exit(0);
        }
        if input.has(Action::Quit) {
            self.end(&QUIT, cx);
            return to_title(cx);
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
    /// keeps its recording.
    fn end(&mut self, input: &FrameInput, cx: &mut Context<'_>) {
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

    /// The picture to present: the game's own.
    pub(crate) fn picture(&self) -> &Framebuffer {
        self.game.framebuffer()
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
    Transition::To(Scene::Title(TitleScene::new()))
}

/// A recording being played back (design D2). Player controls are ignored;
/// pause and quit work, focus loss does not pause.
///
/// Task group 7 needs playback only to show that focus loss leaves it
/// running; the `--play` list, `--loop`, verification and taking over
/// follow in task group 11.
#[derive(Debug)]
pub(crate) struct PlaybackScene {
    /// The playback.
    replay: Replay,
    /// The pause requests.
    pause: Pause,
    /// What the last step reported.
    last_step: Option<ReplayStep>,
}

impl PlaybackScene {
    /// A playback of `recording` from its first frame.
    pub(crate) fn new(recording: &Recording) -> Result<Self, RecordingError> {
        Ok(Self {
            replay: Replay::new(recording, ReplayOptions::default())?,
            pause: Pause::new(),
            last_step: None,
        })
    }

    /// Runs one frame: exactly one step of the playback.
    pub(crate) fn frame(&mut self, input: &ShellInput, cx: &mut Context<'_>) -> Transition {
        if input.close_requested {
            self.step(&QUIT, cx);
            return Transition::Exit(0);
        }
        if input.has(Action::Quit) {
            self.step(&QUIT, cx);
            return to_title(cx);
        }
        let pause = self.pause.before_step(input, false);
        // The recording supplies every control; only the pause request
        // goes in.
        let frame = FrameInput {
            pause: pause.request,
            ..FrameInput::NONE
        };
        let step = self.step(&frame, cx);
        self.pause.after_step(self.replay.game().is_paused());
        match step {
            ReplayStep::Finished(_) => to_title(cx),
            ReplayStep::Running(_) => Transition::Stay,
        }
    }

    /// Steps the playback once and forwards its sound.
    fn step(&mut self, input: &FrameInput, cx: &mut Context<'_>) -> ReplayStep {
        let step = self.replay.step(input, &mut digger_core::NoopObserver);
        self.last_step = Some(step);
        cx.sound(self.replay.drain_sound());
        step
    }

    /// The picture to present: the game's own.
    pub(crate) fn picture(&self) -> &Framebuffer {
        self.replay.game().framebuffer()
    }

    /// The game being played back.
    pub(crate) fn game(&self) -> &Game {
        self.replay.game()
    }

    /// The kind of the last frame, if it ran.
    pub(crate) const fn last_frame(&self) -> Option<FrameKind> {
        match self.last_step {
            Some(ReplayStep::Running(kind)) => Some(kind),
            _ => None,
        }
    }
}
