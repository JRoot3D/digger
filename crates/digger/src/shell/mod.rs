//! The device-free program logic, driven one frame at a time: `Shell`,
//! `Scene`, `ShellInput` and `FrameOutput` (design D2). Nothing here touches a
//! window, an audio device or a gamepad, so the browser build can reuse it.
//!
//! The platform calls [`Shell::frame`] once per pacing deadline with what
//! the input layer collected since the previous frame. The shell first
//! applies the function actions every scene shares (speed, sound and music,
//! fullscreen), then lets the current [`Scene`] run its frame. The scene
//! answers with a [`Transition`]: stay, change to another scene, or end the
//! program. The [`FrameOutput`] carries the picture to present, the sound
//! commands and controls of the frame, the frame time to pace the next
//! frame with and the requests to the platform.
//!
//! The shell never draws over a running game: while a game or a playback is
//! shown, the picture is the core's own framebuffer, and every text of play
//! (`PRESS ANY KEY`, `GAME OVER`, `TIME UP`) comes from the core.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the shell is driven by the platform event loop of a later task group"
    )
)]

use std::path::PathBuf;

use digger_core::recording::{Recording, RecordingError, RecordingKind};
use digger_core::replay::ReplayOptions;
use digger_core::video::Framebuffer;
use digger_core::{GameConfig, GauntletSeconds, LevelSet, SoundCommand, StartLevel};
use digger_sound::SoundControl;

use crate::config::{FrameTime, GameMode, Layer, Settings};
use crate::input::{Action, InputFrame};
use crate::scores::store::ScoreStore;
use crate::scores::{self, ScoreTable, Scores, TableKey};

mod game_over;
mod initials;
mod play;
mod recording;
mod redefine;
mod title;

#[cfg(test)]
mod game_over_tests;
#[cfg(test)]
mod harness;
#[cfg(test)]
mod tests;

pub(crate) use play::{PlayScene, PlaybackScene};
pub(crate) use recording::FinishedRecording;
pub(crate) use title::TitleScene;

/// The speed actions' step: 10000 µs (`desktop-app` spec, runtime speed
/// control; research 05 §1.1).
const SPEED_STEP_MICROS: u32 = 10_000;
/// The longest frame time the slow-down action reaches: 1000000 µs (a
/// port decision of the same requirement).
const SPEED_CAP_MICROS: u32 = 1_000_000;

/// What the platform hands the shell for one frame (design D2). Named so
/// as not to clash with the core's `FrameInput`, which the play scene
/// builds from it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ShellInput {
    /// Everything the input layer collected since the previous frame.
    pub(crate) input: InputFrame,
    /// The window's close button was used (or the system asked the
    /// program to end).
    pub(crate) close_requested: bool,
}

impl From<InputFrame> for ShellInput {
    fn from(input: InputFrame) -> Self {
        Self {
            input,
            close_requested: false,
        }
    }
}

impl ShellInput {
    /// Whether the frame triggered `action`.
    fn has(&self, action: Action) -> bool {
        self.input.actions.contains(&action)
    }
}

/// A request from the shell to the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Request {
    /// Switch between window and fullscreen.
    ToggleFullscreen,
    /// Sound was switched on: open the audio device if it is closed.
    RetryAudio,
    /// End the program with this exit status, through the single shutdown
    /// path (design D12, D13).
    Exit(u8),
}

/// What one frame produced (design D2).
#[derive(Debug)]
pub(crate) struct FrameOutput<'a> {
    /// The picture to present: 640×400 palette indices with their palette.
    pub(crate) picture: &'a Framebuffer,
    /// The simulation's sound commands of this frame, in order, followed by
    /// the scene's own (a stop on leaving a game).
    pub(crate) sound: Vec<SoundCommand>,
    /// The frontend's sound controls of this frame (the toggles).
    pub(crate) controls: Vec<SoundControl>,
    /// The frame time to pace the next frame with.
    pub(crate) frame_time: FrameTime,
    /// Requests to the platform, in order.
    pub(crate) requests: Vec<Request>,
}

/// The output of a frame being collected.
#[derive(Debug, Default)]
struct Pending {
    sound: Vec<SoundCommand>,
    controls: Vec<SoundControl>,
    requests: Vec<Request>,
}

/// One screen of the program (design D2). Every scene owns its state; the
/// shell owns the current scene.
#[derive(Debug)]
pub(crate) enum Scene {
    /// The title screen.
    Title(TitleScene),
    /// A game played by a person.
    Playing(Box<PlayScene>),
    /// A recording being played back.
    Playback(Box<PlaybackScene>),
}

/// What a scene's frame decided (design D2): no scene reaches into
/// another, and no flag outlives the frame that set it.
#[derive(Debug)]
pub(crate) enum Transition {
    /// Keep the current scene.
    Stay,
    /// Change to this scene from the next frame on.
    To(Scene),
    /// End the program with this exit status.
    Exit(u8),
}

/// How the shell is set up, besides the resolved settings.
#[derive(Debug, Clone)]
pub(crate) struct Setup {
    /// The level set games are played on: the built-in one or the level
    /// file's.
    pub(crate) levels: LevelSet,
    /// The format live games are recorded in: the `--record` file's (by
    /// its extension), otherwise DRF, the format of the automatic names.
    pub(crate) record_kind: RecordingKind,
    /// The base seed of the first live game; each later game derives its
    /// own from it. The platform draws it from the clock, tests fix it.
    pub(crate) base_seed: u64,
    /// The level set the score tables are kept for: the built-in levels
    /// or the level file's name.
    pub(crate) score_levels: scores::LevelSet,
    /// The `--record` file: every game played by a person is written to
    /// it when it ends (design D11).
    pub(crate) record_path: Option<PathBuf>,
    /// The `recordings` folder of the data directory, where the
    /// save-recording action writes the last game; `None` when there is
    /// nowhere to save.
    pub(crate) recordings_dir: Option<PathBuf>,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            levels: LevelSet::builtin(),
            record_kind: RecordingKind::Drf,
            base_seed: 0,
            score_levels: scores::LevelSet::Builtin,
            record_path: None,
            recordings_dir: None,
        }
    }
}

/// What a scene's frame may use and change in the shell besides itself.
#[derive(Debug)]
pub(crate) struct Context<'a> {
    /// The resolved settings.
    settings: &'a Settings,
    /// The selected game mode.
    mode: &'a mut GameMode,
    /// The settings the player changed at run time.
    runtime: &'a mut Layer,
    /// The high-score tables and their file.
    scores: &'a mut ScoreStore,
    /// The set-up of the program.
    setup: &'a Setup,
    /// How many live games were started so far.
    games_started: &'a mut u64,
    /// The recording of the last finished game.
    last_game: &'a mut Option<FinishedRecording>,
    /// The frame's output.
    out: &'a mut Pending,
}

impl Context<'_> {
    /// Adds the simulation's or the scene's sound commands to the frame.
    fn sound(&mut self, commands: impl IntoIterator<Item = SoundCommand>) {
        self.out.sound.extend(commands);
    }

    /// The configuration of a new live game in the selected mode.
    fn game_config(&self) -> GameConfig {
        game_config(self.settings, *self.mode, &self.setup.levels)
    }

    /// The score table of the selected mode.
    fn table(&self) -> &ScoreTable {
        self.scores.scores().table(&self.table_key(*self.mode))
    }

    /// The name of the table games in `mode` are entered in.
    fn table_key(&self, mode: GameMode) -> TableKey {
        TableKey::from_mode(mode, self.setup.score_levels.clone())
    }

    /// Selects the next game mode and enters it in the runtime changes,
    /// to be saved at exit (`desktop-app` spec, mode selection).
    fn change_mode(&mut self) {
        *self.mode = title::next_mode(*self.mode);
        self.runtime.mode = Some(*self.mode);
    }

    /// The title screen for the selected mode.
    fn title(&self) -> Scene {
        Scene::Title(TitleScene::new(*self.mode, self.table()))
    }

    /// A new live game in the selected mode, with its own seed.
    fn new_game(&mut self) -> Scene {
        // Each game gets a different base seed (a Weyl sequence over the
        // set-up's seed), so two games of one session differ.
        let seed = self
            .setup
            .base_seed
            .wrapping_add(self.games_started.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        *self.games_started = self.games_started.wrapping_add(1);
        let mode = *self.mode;
        Scene::Playing(Box::new(PlayScene::new(
            self.game_config(),
            seed,
            self.setup.record_kind,
            mode,
            self.table_key(mode),
        )))
    }

    /// Takes the recording of a game that has ended (design D11): with
    /// `--record` it is written to that file at once; otherwise it is kept
    /// as the last game for the save-recording action.
    fn finished(&mut self, recording: FinishedRecording) {
        match &self.setup.record_path {
            Some(path) => {
                recording::save_record_file(&recording, path);
                *self.last_game = None;
            }
            None => *self.last_game = Some(recording),
        }
    }

    /// The save-recording action: writes the last game into the
    /// recordings folder under its automatic name, once; nothing happens
    /// when there is no unsaved game (`desktop-app` spec, other title
    /// screen actions).
    fn save_last_game(&mut self) {
        let Some(dir) = &self.setup.recordings_dir else {
            return;
        };
        if let Some(recording) = self.last_game.take() {
            recording::save_to_recordings(&recording, dir);
        }
    }
}

/// The core's configuration of a game in `mode` under `settings`.
fn game_config(settings: &Settings, mode: GameMode, levels: &LevelSet) -> GameConfig {
    let time = GauntletSeconds::new(u32::from(settings.gauntlet_time));
    let mode = match mode {
        GameMode::OnePlayer => digger_core::GameMode::OnePlayer,
        GameMode::TwoPlayers => digger_core::GameMode::TwoPlayers,
        GameMode::TwoDiggers => digger_core::GameMode::TwoDiggers,
        GameMode::Gauntlet => digger_core::GameMode::Gauntlet { time },
        GameMode::TwoDiggerGauntlet => digger_core::GameMode::GauntletTwoDiggers { time },
    };
    GameConfig {
        mode,
        // The settings hold a start level in 1..=1000, which the core
        // accepts; the default is only a fallback that cannot be reached.
        start_level: StartLevel::new(i64::from(settings.start_level)).unwrap_or_default(),
        unlimited_lives: settings.unlimited_lives,
        levels: *levels,
    }
}

/// The program between the platform's frames (design D2, D13): the current
/// scene, the settings and what the player changed at run time, the frame
/// time and the sound switches, and the last game's recording.
#[derive(Debug)]
pub(crate) struct Shell {
    /// The resolved settings the program started with.
    settings: Settings,
    /// The settings the player changed with in-game actions, for saving at
    /// exit (design D9). Speed changes are never entered here.
    runtime: Layer,
    /// The set-up of the program.
    setup: Setup,
    /// The selected game mode.
    mode: GameMode,
    /// The current frame time.
    frame_time: FrameTime,
    /// Whether sound is switched on, as the sound engine has it.
    sound_on: bool,
    /// Whether music is switched on, as the sound engine has it.
    music_on: bool,
    /// How many live games were started so far.
    games_started: u64,
    /// The recording of the last finished game, until it is saved.
    last_game: Option<FinishedRecording>,
    /// The high-score tables and their file.
    scores: ScoreStore,
    /// The current scene.
    scene: Scene,
}

impl Shell {
    /// A shell on the title screen, with fresh score tables.
    pub(crate) fn new(settings: Settings, setup: Setup) -> Self {
        Self::with_scores(settings, setup, Scores::default())
    }

    /// A shell on the title screen, showing the tables of `scores`, which
    /// are kept in memory only.
    pub(crate) fn with_scores(settings: Settings, setup: Setup, scores: Scores) -> Self {
        Self::with_store(settings, setup, ScoreStore::in_memory(scores))
    }

    /// A shell on the title screen, showing and saving the tables of
    /// `scores`.
    pub(crate) fn with_store(settings: Settings, setup: Setup, scores: ScoreStore) -> Self {
        let mode = settings.mode;
        let table = scores
            .scores()
            .table(&TableKey::from_mode(mode, setup.score_levels.clone()));
        Self {
            runtime: Layer::default(),
            mode,
            frame_time: settings.speed.frame_time(),
            sound_on: settings.sound_enabled,
            music_on: settings.music,
            games_started: 0,
            last_game: None,
            scene: Scene::Title(TitleScene::new(mode, table)),
            scores,
            settings,
            setup,
        }
    }

    /// Runs one frame.
    pub(crate) fn frame(&mut self, input: &ShellInput) -> FrameOutput<'_> {
        let mut out = Pending::default();
        self.function_actions(input, &mut out);
        let mut cx = Context {
            settings: &self.settings,
            mode: &mut self.mode,
            runtime: &mut self.runtime,
            scores: &mut self.scores,
            setup: &self.setup,
            games_started: &mut self.games_started,
            last_game: &mut self.last_game,
            out: &mut out,
        };
        let transition = match &mut self.scene {
            Scene::Title(title) => title.frame(input, &mut cx),
            Scene::Playing(play) => play.frame(input, &mut cx),
            Scene::Playback(playback) => playback.frame(input, &mut cx),
        };
        match transition {
            Transition::Stay => {}
            Transition::To(scene) => self.scene = scene,
            Transition::Exit(code) => out.requests.push(Request::Exit(code)),
        }
        FrameOutput {
            picture: self.picture(),
            sound: out.sound,
            controls: out.controls,
            frame_time: self.frame_time,
            requests: out.requests,
        }
    }

    /// The function actions every current scene shares: speed, sound,
    /// music and fullscreen (`desktop-app` spec; `input-controls` spec,
    /// function actions fire once per press).
    fn function_actions(&mut self, input: &ShellInput, out: &mut Pending) {
        // During initials entry every key but quit and the fullscreen
        // chord is ignored (`input-controls` spec, typing initials).
        let entering = self.entering_initials();
        for &action in &input.input.actions {
            if entering && action != Action::Fullscreen {
                continue;
            }
            match action {
                Action::SpeedUp => self.speed_up(),
                Action::SpeedDown => self.speed_down(),
                Action::ToggleSound => {
                    self.sound_on = !self.sound_on;
                    // The engine switches music on with sound (sound
                    // quirk SQ10); the shell mirrors it.
                    if self.sound_on && !self.music_on {
                        self.music_on = true;
                        self.runtime.music = Some(true);
                    }
                    self.runtime.sound_enabled = Some(self.sound_on);
                    out.controls.push(SoundControl::ToggleSound);
                    if self.sound_on {
                        out.requests.push(Request::RetryAudio);
                    }
                }
                Action::ToggleMusic => {
                    self.music_on = !self.music_on;
                    self.runtime.music = Some(self.music_on);
                    out.controls.push(SoundControl::ToggleMusic);
                }
                Action::Fullscreen => out.requests.push(Request::ToggleFullscreen),
                _ => {}
            }
        }
    }

    /// The speed-up action: 10000 µs shorter, only while the frame time is
    /// above 10000 µs (compat quirk: research 05 §1.1).
    fn speed_up(&mut self) {
        let micros = self.frame_micros();
        if micros > SPEED_STEP_MICROS {
            self.frame_time = FrameTime::from_micros(micros - SPEED_STEP_MICROS);
        }
    }

    /// The slow-down action: 10000 µs longer, up to 1000000 µs.
    fn speed_down(&mut self) {
        let micros = self
            .frame_micros()
            .saturating_add(SPEED_STEP_MICROS)
            .min(SPEED_CAP_MICROS);
        self.frame_time = FrameTime::from_micros(micros);
    }

    /// The current frame time in whole microseconds.
    fn frame_micros(&self) -> u32 {
        u32::try_from(self.frame_time.as_micros()).unwrap_or(SPEED_CAP_MICROS)
    }

    /// The picture of the current scene.
    pub(crate) fn picture(&self) -> &Framebuffer {
        match &self.scene {
            Scene::Title(title) => title.picture(),
            Scene::Playing(play) => play.picture(),
            Scene::Playback(playback) => playback.picture(),
        }
    }

    /// Starts a live game in the selected mode, as the title screen's
    /// start does.
    pub(crate) fn start_game(&mut self) {
        let mut out = Pending::default();
        let mut cx = Context {
            settings: &self.settings,
            mode: &mut self.mode,
            runtime: &mut self.runtime,
            scores: &mut self.scores,
            setup: &self.setup,
            games_started: &mut self.games_started,
            last_game: &mut self.last_game,
            out: &mut out,
        };
        self.scene = cx.new_game();
    }

    /// Starts playing `recording` back.
    pub(crate) fn start_playback(&mut self, recording: &Recording) -> Result<(), RecordingError> {
        self.start_playback_with(recording, ReplayOptions::default())
    }

    /// Starts playing `recording` back with `options` (a take-over tick,
    /// for example).
    pub(crate) fn start_playback_with(
        &mut self,
        recording: &Recording,
        options: ReplayOptions,
    ) -> Result<(), RecordingError> {
        self.scene = Scene::Playback(Box::new(PlaybackScene::with_options(recording, options)?));
        Ok(())
    }

    /// Whether a new-high-score screen is shown.
    pub(crate) fn entering_initials(&self) -> bool {
        matches!(&self.scene, Scene::Playing(play) if play.initials().is_some())
    }

    /// The high-score tables.
    pub(crate) fn scores(&self) -> &Scores {
        self.scores.scores()
    }

    /// The current scene.
    pub(crate) const fn scene(&self) -> &Scene {
        &self.scene
    }

    /// The selected game mode.
    pub(crate) const fn mode(&self) -> GameMode {
        self.mode
    }

    /// The settings the player changed with in-game actions.
    pub(crate) const fn runtime_changes(&self) -> &Layer {
        &self.runtime
    }

    /// The current frame time.
    pub(crate) const fn frame_time(&self) -> FrameTime {
        self.frame_time
    }

    /// Whether sound is switched on.
    pub(crate) const fn sound_on(&self) -> bool {
        self.sound_on
    }

    /// Whether music is switched on.
    pub(crate) const fn music_on(&self) -> bool {
        self.music_on
    }

    /// The recording of the last finished game, if it was not saved yet.
    pub(crate) const fn last_game(&self) -> Option<&FinishedRecording> {
        self.last_game.as_ref()
    }
}
