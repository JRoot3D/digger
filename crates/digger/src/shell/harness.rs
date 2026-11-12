//! The shell test harness: scripted frames in, pictures, sound and
//! requests out (task 7.1).
//!
//! The harness owns a [`Shell`] and the [`InputState`] the platform would
//! feed. Tests press and release keys by their configuration names, then
//! run frames; every frame's output is captured as a [`Captured`] record
//! (the picture's CRC, its sound commands and controls, the frame time and
//! the requests), so that tests can look at any frame afterwards.

#![allow(clippy::panic, reason = "a misused harness is a failed test")]

use digger_core::recording::RecordingKind;
use digger_core::{FrameKind, Game, SoundCommand};
use digger_sound::SoundControl;

use super::{FrameOutput, Request, Scene, Setup, Shell, ShellInput};
use crate::config::{FrameTime, Settings};
use crate::input::{Bindings, InputState, KeyName, KeyPress, PadSignal};

/// One frame's output, as the platform would receive it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Captured {
    /// The CRC-32 of the presented picture's indices.
    pub(super) picture_crc: u32,
    /// The sound commands of the frame.
    pub(super) sound: Vec<SoundCommand>,
    /// The sound controls of the frame.
    pub(super) controls: Vec<SoundControl>,
    /// The frame time for the next frame.
    pub(super) frame_time: FrameTime,
    /// The requests to the platform.
    pub(super) requests: Vec<Request>,
}

impl Captured {
    fn of(output: &FrameOutput<'_>) -> Self {
        Self {
            picture_crc: output.picture.crc32(),
            sound: output.sound.clone(),
            controls: output.controls.clone(),
            frame_time: output.frame_time,
            requests: output.requests.clone(),
        }
    }
}

/// A shell driven by scripted input.
#[derive(Debug)]
pub(super) struct Harness {
    /// The shell under test.
    pub(super) shell: Shell,
    /// The input state the platform would feed.
    pub(super) input: InputState,
    /// A close request for the next frame.
    close: bool,
    /// Every frame's output, in order.
    pub(super) frames: Vec<Captured>,
}

/// The key named `name`.
pub(super) fn key(name: &str) -> KeyName {
    KeyName::parse(name).unwrap_or_else(|| panic!("{name} is a key name"))
}

impl Harness {
    /// A harness over a shell with default settings.
    pub(super) fn new() -> Self {
        Self::with_settings(Settings::default())
    }

    /// A harness over a shell with `settings` and the default set-up.
    pub(super) fn with_settings(settings: Settings) -> Self {
        Self::with(settings, Setup::default())
    }

    /// A harness over a shell with `settings` and `setup`.
    pub(super) fn with(settings: Settings, setup: Setup) -> Self {
        let input = InputState::new(Bindings::from_settings(&settings));
        Self {
            shell: Shell::new(settings, setup),
            input,
            close: false,
            frames: Vec::new(),
        }
    }

    /// A harness whose live games are recorded as `kind`.
    pub(super) fn recording(kind: RecordingKind) -> Self {
        Self::with(
            Settings::default(),
            Setup {
                record_kind: kind,
                ..Setup::default()
            },
        )
    }

    /// Presses the key named `name`.
    pub(super) fn press(&mut self, name: &str) {
        self.input.key_down(KeyPress {
            key: Some(key(name)),
            repeat: false,
            text: None,
        });
    }

    /// Releases the key named `name`.
    pub(super) fn release(&mut self, name: &str) {
        self.input.key_up(key(name));
    }

    /// Presses and releases the key named `name` within one frame.
    pub(super) fn tap(&mut self, name: &str) {
        self.press(name);
        self.release(name);
    }

    /// The window loses focus.
    pub(super) fn lose_focus(&mut self) {
        self.input.focus_lost();
    }

    /// A pad signal arrives.
    pub(super) fn pad(&mut self, signal: PadSignal) {
        self.input.pad_signal(signal);
    }

    /// The window's close button is used before the next frame.
    pub(super) fn close(&mut self) {
        self.close = true;
    }

    /// Runs one frame and returns its output.
    pub(super) fn frame(&mut self) -> &Captured {
        let input = ShellInput {
            input: self.input.take_frame(),
            close_requested: std::mem::take(&mut self.close),
        };
        let output = self.shell.frame(&input);
        let captured = Captured::of(&output);
        self.frames.push(captured);
        self.frames.last().unwrap_or_else(|| unreachable!())
    }

    /// Runs `n` frames.
    pub(super) fn run(&mut self, n: usize) {
        for _ in 0..n {
            self.frame();
        }
    }

    /// The game of the current play or playback scene.
    pub(super) fn game(&self) -> &Game {
        match self.shell.scene() {
            Scene::Playing(play) => play.game(),
            Scene::Playback(playback) => playback.game(),
            Scene::Title(_) => panic!("no game on the title screen"),
        }
    }

    /// Whether the title screen is shown.
    pub(super) fn on_title(&self) -> bool {
        matches!(self.shell.scene(), Scene::Title(_))
    }

    /// The kind of the last frame the current game or playback stepped.
    pub(super) fn last_frame(&self) -> Option<FrameKind> {
        match self.shell.scene() {
            Scene::Playing(play) => play.last_outcome().map(|outcome| outcome.frame),
            Scene::Playback(playback) => playback.last_frame(),
            Scene::Title(_) => None,
        }
    }

    /// Runs frames until the game's main phase has run its first tick,
    /// and returns the number of frames that took.
    pub(super) fn until_first_tick(&mut self) -> usize {
        let mut frames = 0;
        loop {
            self.frame();
            frames += 1;
            if self.last_frame() == Some(FrameKind::Tick) {
                return frames;
            }
            assert!(frames < 1000, "no tick within 1000 frames");
        }
    }

    /// Starts a live game and runs it to its first main-phase tick.
    pub(super) fn play(&mut self) {
        self.shell.start_game();
        self.until_first_tick();
    }

    /// All sound commands captured from frame `from` on.
    pub(super) fn sound_since(&self, from: usize) -> Vec<SoundCommand> {
        self.frames
            .iter()
            .skip(from)
            .flat_map(|frame| frame.sound.iter().copied())
            .collect()
    }

    /// All requests captured so far.
    pub(super) fn requests(&self) -> Vec<Request> {
        self.frames
            .iter()
            .flat_map(|frame| frame.requests.iter().copied())
            .collect()
    }
}
