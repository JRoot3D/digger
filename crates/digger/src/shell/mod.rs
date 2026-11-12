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
use digger_core::replay::{ReplayOptions, ReplayReport};
use digger_core::video::Framebuffer;
use digger_core::{GameConfig, GauntletSeconds, LevelSet, SoundCommand, StartLevel};
use digger_sound::SoundControl;

use crate::config::file::SaveError;
use crate::config::{FrameTime, GameMode, Layer, Settings};
use crate::event::AppEvent;
use crate::headless::VerifyJob;
use crate::input::{Action, Bindings, InputFrame};
use crate::launch::Redefine;
use crate::notice::{Notice, Rejection};
use crate::page::Phase;
use crate::scores::store::ScoreStore;
use crate::scores::{self, ScoreTable, Scores, TableKey};
use crate::store::{Storage, StoreKey};
use crate::transfer::{FileTransfer, MAX_IMPORT_BYTES, TransferError};

mod game_over;
mod initials;
mod play;
mod playlist;
pub(crate) mod recording;
mod redefine;
mod title;

#[cfg(test)]
mod game_over_tests;
#[cfg(test)]
mod harness;
#[cfg(test)]
mod playback_tests;
#[cfg(test)]
mod redefine_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod web_tests;

pub(crate) use play::{PlayScene, PlaybackScene};
pub(crate) use playlist::PlayList;
pub(crate) use recording::FinishedRecording;
pub(crate) use redefine::{KeyTable, RedefineScene, Variant};
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
    /// A key redefinition completed: the input layer takes the bindings of
    /// [`Shell::bindings`] from the next event on.
    BindingsChanged,
}

/// What the shell does when a scene ends the program (add-web-build-and-
/// releases, design decision 17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ExitPolicy {
    /// The desktop: the program ends through [`Request::Exit`].
    #[default]
    Quit,
    /// The browser: a page is never closed, so the title screen runs on
    /// (`web-build` spec, no command line and no exit).
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        expect(dead_code, reason = "the browser's exit policy")
    )]
    StayOnTitle,
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
    /// The notices raised in this frame (a failed save, for example).
    pub(crate) notices: Vec<Notice>,
}

/// The output of a frame being collected.
#[derive(Debug, Default)]
struct Pending {
    sound: Vec<SoundCommand>,
    controls: Vec<SoundControl>,
    requests: Vec<Request>,
    /// The key table of a redefinition completed in this frame.
    keys: Option<Box<KeyTable>>,
}

/// One screen of the program (design D2). Every scene owns its state; the
/// shell owns the current scene.
#[derive(Debug)]
pub(crate) enum Scene {
    /// The title screen.
    Title(TitleScene),
    /// Key redefinition.
    Redefine(Box<RedefineScene>),
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
    /// `--redefine-keys[=all]`: the program opens on key redefinition
    /// instead of the title screen.
    pub(crate) redefine_keys: Option<Redefine>,
    /// `--play`: the recordings played before the title screen, in order.
    pub(crate) play: Vec<PathBuf>,
    /// `--loop`: the `--play` list repeats until quit.
    pub(crate) loop_playback: bool,
    /// `--take-over-at`: each `--play` recording is handed to the player
    /// once this many ticks have been played.
    pub(crate) take_over_at: Option<u32>,
    /// `--verify`: the program plays this recording and ends with its
    /// status.
    pub(crate) verify: Option<VerifyJob>,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            levels: LevelSet::builtin(),
            record_kind: RecordingKind::Drf,
            base_seed: 0,
            score_levels: scores::LevelSet::Builtin,
            record_path: None,
            redefine_keys: None,
            play: Vec::new(),
            loop_playback: false,
            take_over_at: None,
            verify: None,
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
    /// The high-score tables.
    scores: &'a mut ScoreStore,
    /// Where the settings and the tables are saved.
    storage: &'a mut Storage,
    /// Where the save-recording action hands the last game.
    transfer: &'a mut Option<Box<dyn FileTransfer>>,
    /// The notices raised in the frame besides the storage's.
    notices: &'a mut Vec<Notice>,
    /// The set-up of the program.
    setup: &'a Setup,
    /// How many live games were started so far.
    games_started: &'a mut u64,
    /// The recording of the last finished game.
    last_game: &'a mut Option<FinishedRecording>,
    /// The `--play` list.
    playlist: &'a mut PlayList,
    /// The report of a finished `--verify` run.
    verified: &'a mut Option<ReplayReport>,
    /// The result of the last watched playback that ran to its end.
    played: &'a mut Option<String>,
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

    /// Key redefinition over the keys in effect, for the selected mode.
    fn redefine(&self, variant: Variant) -> Scene {
        redefine_scene(self.settings, *self.mode, variant)
    }

    /// Hands the key table of a completed redefinition to the shell,
    /// which applies and saves it after the frame.
    fn redefined(&mut self, keys: Box<KeyTable>) {
        self.out.keys = Some(keys);
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

    /// What follows a playback of the `--play` list that ended: the next
    /// recording of the list, else the title screen. The playback's sound
    /// stops.
    fn after_playback(&mut self) -> Transition {
        self.sound([SoundCommand::StopAll]);
        Transition::To(next_playback(self.playlist, self.setup).unwrap_or_else(|| self.title()))
    }

    /// The save-recording action: hands the last game out under its
    /// automatic name, once, through the platform's file transfer (on the
    /// desktop, into the recordings folder); nothing happens when there is
    /// no unsaved game or nowhere to save (`desktop-app` spec, other title
    /// screen actions).
    fn save_last_game(&mut self) {
        let Some(transfer) = self.transfer.as_mut() else {
            return;
        };
        if let Some(recording) = self.last_game.take()
            && let Err(error) = transfer.export(&recording.auto_name(), recording.text.as_bytes())
        {
            self.notices.push(Notice::RecordingNotSaved(error));
        }
    }
}

/// The playback of the next recording of `playlist`, if the list goes
/// on (`configuration` spec, play options and taking over a playback).
fn next_playback(playlist: &mut PlayList, setup: &Setup) -> Option<Scene> {
    let (file, recording) = playlist.advance()?;
    let scene = PlaybackScene::watch(recording, file, setup.take_over_at, setup.record_kind);
    Some(Scene::Playback(Box::new(scene)))
}

/// Key redefinition over the keys of `settings`, for `mode`.
fn redefine_scene(settings: &Settings, mode: GameMode, variant: Variant) -> Scene {
    Scene::Redefine(Box::new(RedefineScene::new(
        &settings.keys,
        mode.two_diggers(),
        variant,
    )))
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
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent switches: sound, music, the hidden-page pause and the \
              damaged-settings rule"
)]
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
    /// The `--play` list.
    playlist: PlayList,
    /// The report of the `--verify` run, once it has ended.
    verified: Option<ReplayReport>,
    /// The result of the last watched playback that ran to its end, for
    /// the page (`data-result`).
    played: Option<String>,
    /// The high-score tables.
    scores: ScoreStore,
    /// Where the settings and the tables are saved.
    storage: Storage,
    /// Where the save-recording action hands the last game; `None` when
    /// there is nowhere to save.
    transfer: Option<Box<dyn FileTransfer>>,
    /// The notices raised besides the storage's, until they are taken.
    notices: Vec<Notice>,
    /// What an exit of the current scene does.
    exit: ExitPolicy,
    /// The page was hidden during live play: the next frame carries the
    /// pause action (add-web-build-and-releases, design decision 5).
    hidden_pause: bool,
    /// The runtime changes as last saved by
    /// [`Shell::save_changed_settings`].
    saved: Layer,
    /// The stored settings were damaged when the program started: the
    /// next save replaces them (`web-build` spec, damaged stored settings).
    replace_damaged: bool,
    /// The current scene.
    scene: Scene,
}

impl Shell {
    /// A shell on the title screen, with fresh score tables kept in
    /// memory only.
    pub(crate) fn new(settings: Settings, setup: Setup) -> Self {
        Self::with_scores(settings, setup, Scores::default())
    }

    /// A shell on the title screen, showing the tables of `scores`, which
    /// are kept in memory only.
    pub(crate) fn with_scores(settings: Settings, setup: Setup, scores: Scores) -> Self {
        Self::build(
            settings,
            setup,
            Storage::memory(),
            ScoreStore::in_memory(scores),
        )
    }

    /// A shell on the title screen, showing the tables kept in `storage`
    /// and saving the tables and the settings the player changes there.
    pub(crate) fn with_storage(settings: Settings, setup: Setup, storage: Storage) -> Self {
        let scores = ScoreStore::open_reporting(storage.store());
        Self::build(settings, setup, storage, scores)
    }

    /// A shell on the title screen over `storage` and the tables `scores`.
    fn build(settings: Settings, setup: Setup, storage: Storage, scores: ScoreStore) -> Self {
        let mode = settings.mode;
        let table = scores
            .scores()
            .table(&TableKey::from_mode(mode, setup.score_levels.clone()));
        let mut playlist = PlayList::new(&setup.play, setup.loop_playback);
        // A verification run plays its recording and nothing else; a
        // `--play` list comes before the title screen (or the key
        // redefinition `--redefine-keys` opens on).
        let first = match &setup.verify {
            Some(job) => Some(Scene::Playback(Box::new(PlaybackScene::verify(
                &job.recording,
                setup.record_path.as_ref().map(|_| setup.record_kind),
            )))),
            None => next_playback(&mut playlist, &setup),
        };
        let scene = match (first, setup.redefine_keys) {
            (Some(scene), _) => scene,
            (None, Some(Redefine::Usual)) => redefine_scene(&settings, mode, Variant::Usual),
            (None, Some(Redefine::All)) => redefine_scene(&settings, mode, Variant::All),
            (None, None) => Scene::Title(TitleScene::new(mode, table)),
        };
        Self {
            runtime: Layer::default(),
            mode,
            frame_time: settings.speed.frame_time(),
            sound_on: settings.sound_enabled,
            music_on: settings.music,
            games_started: 0,
            last_game: None,
            playlist,
            verified: None,
            played: None,
            scene,
            scores,
            storage,
            transfer: None,
            notices: Vec::new(),
            exit: ExitPolicy::Quit,
            hidden_pause: false,
            saved: Layer::default(),
            replace_damaged: false,
            settings,
            setup,
        }
    }

    /// Runs one frame.
    pub(crate) fn frame(&mut self, input: &ShellInput) -> FrameOutput<'_> {
        let paused;
        let input = if std::mem::take(&mut self.hidden_pause)
            && matches!(self.scene, Scene::Playing(_))
            && !self.entering_initials()
        {
            // The page was hidden during live play: the game enters its
            // own pause phase, as with the pause key.
            let mut with_pause = input.clone();
            with_pause.input.actions.push(Action::Pause);
            paused = with_pause;
            &paused
        } else {
            input
        };
        let mut out = Pending::default();
        self.function_actions(input, &mut out);
        let mut cx = Context {
            settings: &self.settings,
            mode: &mut self.mode,
            runtime: &mut self.runtime,
            scores: &mut self.scores,
            storage: &mut self.storage,
            transfer: &mut self.transfer,
            notices: &mut self.notices,
            setup: &self.setup,
            games_started: &mut self.games_started,
            last_game: &mut self.last_game,
            playlist: &mut self.playlist,
            verified: &mut self.verified,
            played: &mut self.played,
            out: &mut out,
        };
        let transition = match &mut self.scene {
            Scene::Title(title) => title.frame(input, &mut cx),
            Scene::Redefine(redefine) => redefine.frame(input, &mut cx),
            Scene::Playing(play) => play.frame(input, &mut cx),
            Scene::Playback(playback) => playback.frame(input, &mut cx),
        };
        match transition {
            Transition::Stay => {}
            Transition::To(scene) => self.scene = scene,
            Transition::Exit(code) => match self.exit {
                ExitPolicy::Quit => out.requests.push(Request::Exit(code)),
                ExitPolicy::StayOnTitle => {
                    if !matches!(self.scene, Scene::Title(_)) {
                        self.scene = self.title_scene();
                    }
                }
            },
        }
        if let Some(keys) = out.keys.take() {
            self.apply_keys(*keys, &mut out);
        }
        let notices = self.take_notices();
        FrameOutput {
            picture: self.picture(),
            sound: out.sound,
            controls: out.controls,
            frame_time: self.frame_time,
            requests: out.requests,
            notices,
        }
    }

    /// The function actions every current scene shares: speed, sound,
    /// music and fullscreen (`desktop-app` spec; `input-controls` spec,
    /// function actions fire once per press).
    fn function_actions(&mut self, input: &ShellInput, out: &mut Pending) {
        // During key redefinition every key press is a candidate key, so
        // no function action applies; only the Alt+Enter chord, which is
        // not a key press, still switches fullscreen (`desktop-app` spec,
        // fullscreen at any time).
        if matches!(self.scene, Scene::Redefine(_)) {
            for _ in 0..input.input.fullscreen_chords {
                out.requests.push(Request::ToggleFullscreen);
            }
            return;
        }
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

    /// Applies the key table of a completed redefinition (`input-controls`
    /// spec, redefinition is saved): the keys take effect at once, enter
    /// the runtime changes, and are written to the configuration file
    /// straight away, every action under its name. A failed save is
    /// reported as a notice; the keys stay in effect for this run.
    fn apply_keys(&mut self, keys: KeyTable, out: &mut Pending) {
        let mut changes = Layer::default();
        for (slot, keys) in changes.keys.iter_mut().zip(&keys) {
            *slot = Some(keys.clone());
        }
        self.runtime.keys.clone_from(&changes.keys);
        self.settings.keys = keys;
        self.save_changes(&changes);
        out.requests.push(Request::BindingsChanged);
    }

    /// Saves `changes` over the stored settings; a failure raises a
    /// notice and the changes stay in effect for this run. Stored settings
    /// that were damaged at start are replaced when
    /// [`Shell::replace_damaged_settings`] was called.
    fn save_changes(&mut self, changes: &Layer) {
        let store = self.storage.store_mut();
        let result = match crate::config::file::save(store, changes) {
            Err(SaveError::Invalid(_)) if self.replace_damaged => {
                crate::config::file::save_fresh(store, changes).map_err(SaveError::Store)
            }
            result => result,
        };
        match result {
            Ok(_) => {}
            Err(SaveError::Store(error)) => self.storage.failed(StoreKey::Settings, error),
            Err(SaveError::Invalid(error)) => self
                .storage
                .notice(Notice::SettingsNotSaved(error.to_string())),
        }
    }

    /// Saves the runtime changes if they differ from those last saved
    /// here: the browser saves the settings as they change rather than at
    /// exit (add-web-build-and-releases, design decision 4).
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        expect(dead_code, reason = "the browser saves settings as they change")
    )]
    pub(crate) fn save_changed_settings(&mut self) {
        if self.runtime != self.saved {
            let changes = self.runtime.clone();
            self.save_changes(&changes);
            self.saved = changes;
        }
    }

    /// The stored settings could not be read at start, so the defaults
    /// apply: the next save replaces them instead of refusing to touch
    /// them (`web-build` spec, damaged stored settings).
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        expect(dead_code, reason = "the browser starts over damaged settings")
    )]
    pub(crate) const fn replace_damaged_settings(&mut self) {
        self.replace_damaged = true;
    }

    /// Sets what an exit of the current scene does.
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        expect(dead_code, reason = "the browser's exit policy")
    )]
    pub(crate) const fn set_exit_policy(&mut self, exit: ExitPolicy) {
        self.exit = exit;
    }

    /// The title screen for the selected mode, showing its table.
    fn title_scene(&self) -> Scene {
        let table = self.scores.scores().table(&TableKey::from_mode(
            self.mode,
            self.setup.score_levels.clone(),
        ));
        Scene::Title(TitleScene::new(self.mode, table))
    }

    /// Saves the settings the player changed during the run (at exit, on
    /// the desktop: `configuration` spec, saving settings). A failure is a
    /// notice, taken with [`Shell::take_notices`].
    pub(crate) fn save_settings(&mut self) {
        let changes = self.runtime.clone();
        self.save_changes(&changes);
    }

    /// The notices raised since they were last taken: a frame's output
    /// carries them, and those raised outside a frame (by
    /// [`Shell::save_settings`] or [`Shell::event`]) are taken here.
    pub(crate) fn take_notices(&mut self) -> Vec<Notice> {
        let mut notices = std::mem::take(&mut self.notices);
        notices.extend(self.storage.take_notices());
        notices
    }

    /// Sets where the save-recording action hands the last game.
    pub(crate) fn set_transfer(&mut self, transfer: impl FileTransfer + 'static) {
        self.transfer = Some(Box::new(transfer));
    }

    /// Handles an event from the platform (add-web-build-and-releases,
    /// design decision 4). A picked recording is played back from the
    /// title screen with the desktop playback rules, and the title returns
    /// when it ends; one that cannot be played, or one picked away from the
    /// title screen, is rejected with a notice and the scene goes on.
    pub(crate) fn event(&mut self, event: AppEvent) {
        match event {
            AppEvent::RecordingPicked { name, bytes } => self.picked(name, &bytes),
            AppEvent::TransferFailed(TransferError::TooLarge { name, size }) => {
                self.notices.push(Notice::RecordingRejected {
                    name,
                    reason: Rejection::TooLarge { size },
                });
            }
            AppEvent::TransferFailed(TransferError::Unreadable { name, reason }) => {
                self.notices.push(Notice::RecordingRejected {
                    name,
                    reason: Rejection::Unreadable(reason),
                });
            }
            AppEvent::TransferFailed(error) => {
                self.notices.push(Notice::RecordingNotSaved(error));
            }
            AppEvent::StorageFailed(key, error) => self.storage.failed(key, error),
            AppEvent::SaveRecording => self.save_recording(),
            // A page hidden during live play pauses the game from the next
            // frame on, through the game's own pause phase; the title and a
            // playback simply go on when the platform steps them again
            // (`web-build` spec, hidden tab pauses the game).
            AppEvent::Visibility(visible) => {
                if !visible && matches!(self.scene, Scene::Playing(_)) {
                    self.hidden_pause = true;
                }
            }
        }
    }

    /// A recording picked by the player: rejected with a notice naming
    /// the reason when it is too large, is no recording, is of a kind not
    /// played, is damaged, or is picked away from the title screen.
    fn picked(&mut self, name: String, bytes: &[u8]) {
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let rejected = if size > MAX_IMPORT_BYTES {
            Some(Rejection::TooLarge { size })
        } else if !matches!(self.scene, Scene::Title(_)) {
            Some(Rejection::NotOnTitle)
        } else {
            None
        };
        if let Some(reason) = rejected {
            self.notices
                .push(Notice::RecordingRejected { name, reason });
            return;
        }
        match digger_core::recording::parse(bytes) {
            Ok(recording) => {
                let scene = PlaybackScene::watch(
                    &recording,
                    std::path::Path::new(&name),
                    None,
                    self.setup.record_kind,
                );
                self.scene = Scene::Playback(Box::new(scene));
            }
            Err(error) => self.notices.push(Notice::RecordingRejected {
                name,
                reason: Rejection::of(&error),
            }),
        }
    }

    /// What the program is doing, for the page.
    pub(crate) fn phase(&self) -> Phase {
        match &self.scene {
            Scene::Title(_) => Phase::Title,
            Scene::Redefine(_) => Phase::Redefine,
            Scene::Playing(play) if play.game().is_paused() => Phase::Paused,
            Scene::Playing(_) => Phase::Playing,
            Scene::Playback(_) => Phase::Playback,
        }
    }

    /// The platform switched between the window and fullscreen at the
    /// player's request: the new state enters the runtime changes, so it
    /// is written to the configuration file at exit (`configuration` spec,
    /// saving settings).
    pub(crate) fn fullscreen_changed(&mut self, fullscreen: bool) {
        self.runtime.fullscreen = Some(fullscreen);
    }

    /// The key and pad-button bindings in effect.
    pub(crate) fn bindings(&self) -> Bindings {
        Bindings::from_settings(&self.settings)
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
            Scene::Redefine(redefine) => redefine.picture(),
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
            storage: &mut self.storage,
            transfer: &mut self.transfer,
            notices: &mut self.notices,
            setup: &self.setup,
            games_started: &mut self.games_started,
            last_game: &mut self.last_game,
            playlist: &mut self.playlist,
            verified: &mut self.verified,
            played: &mut self.played,
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

    /// The resolved settings the program started with (with redefined
    /// keys applied).
    pub(crate) const fn settings(&self) -> &Settings {
        &self.settings
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

    /// The report of the `--verify` run, once it has ended: the platform
    /// turns it into the result line or the error that ends the program
    /// (`crate::headless::conclude`).
    pub(crate) const fn verify_report(&self) -> Option<&ReplayReport> {
        self.verified.as_ref()
    }

    /// The notices given for skipped `--play` files so far.
    pub(crate) fn playback_notices(&self) -> &[String] {
        self.playlist.notices()
    }

    /// The recording of the last finished game, if it was not saved yet.
    pub(crate) const fn last_game(&self) -> Option<&FinishedRecording> {
        self.last_game.as_ref()
    }

    /// Whether the save-recording action would hand a recording out now:
    /// a finished game's recording is kept, there is somewhere to save it,
    /// and the title screen is shown (`web-build` spec, saving a
    /// recording as a download).
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        expect(dead_code, reason = "shown on the browser's page")
    )]
    pub(crate) fn can_save(&self) -> bool {
        self.last_game.is_some() && self.transfer.is_some() && matches!(self.scene, Scene::Title(_))
    }

    /// The result of the last watched playback that ran to its end: its
    /// result line (`score=S level=L ticks=T`) when it matched its
    /// recording, else the first line of its divergence report.
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        expect(dead_code, reason = "shown on the browser's page")
    )]
    pub(crate) fn playback_result(&self) -> Option<&str> {
        self.played.as_deref()
    }

    /// The page's save-recording button: the title screen's save action.
    /// Away from the title screen nothing is saved.
    fn save_recording(&mut self) {
        if !matches!(self.scene, Scene::Title(_)) {
            return;
        }
        let mut out = Pending::default();
        let mut cx = Context {
            settings: &self.settings,
            mode: &mut self.mode,
            runtime: &mut self.runtime,
            scores: &mut self.scores,
            storage: &mut self.storage,
            transfer: &mut self.transfer,
            notices: &mut self.notices,
            setup: &self.setup,
            games_started: &mut self.games_started,
            last_game: &mut self.last_game,
            playlist: &mut self.playlist,
            verified: &mut self.verified,
            played: &mut self.played,
            out: &mut out,
        };
        cx.save_last_game();
    }
}
