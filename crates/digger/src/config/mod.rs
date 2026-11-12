//! Settings: the resolved `Settings`, the all-optional `Layer`, merging and
//! validation (design D9).
//!
//! Every setting of the `configuration` spec is described once, in the
//! static schema of [`schema`]: its key path, type, range and default. The
//! file reader validates against it, the defaults are built from it and the
//! file writer walks it. A setting can come from three layers: the
//! built-in defaults, the configuration file and the command line; each
//! layer is a [`Layer`] of `Option`s and [`Settings::resolve`] takes the
//! last value present for each field.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the settings are used by the command line, shell and platform code of later task groups"
    )
)]

use std::fmt;
use std::time::Duration;

use crate::input::{Action, KeyName, PadAction, PadButton};

pub(crate) mod file;
pub(crate) mod paths;
pub(crate) mod schema;

use schema::{Field, Value};

/// The game speed setting, 0 to 500: the frame time is `speed × 2000` µs
/// and 0 means unpaced (`configuration` spec, game set-up options).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Speed(u16);

impl Speed {
    /// The highest speed value.
    pub(crate) const MAX: u16 = 500;
    /// Microseconds of frame time per speed unit.
    pub(crate) const MICROS_PER_UNIT: u32 = 2000;
    /// The default speed (frame time 80000 µs).
    pub(crate) const DEFAULT: Self = Self(40);

    /// The speed `value`, if it is in 0–500.
    pub(crate) const fn new(value: u16) -> Option<Self> {
        if value <= Self::MAX {
            Some(Self(value))
        } else {
            None
        }
    }

    /// The speed value.
    pub(crate) const fn get(self) -> u16 {
        self.0
    }

    /// The frame time this speed gives.
    pub(crate) fn frame_time(self) -> FrameTime {
        FrameTime::from_micros(u32::from(self.0) * Self::MICROS_PER_UNIT)
    }
}

/// The time one frame lasts; zero means unpaced (as fast as possible).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct FrameTime(Duration);

impl FrameTime {
    /// Unpaced: frames follow each other as fast as possible.
    pub(crate) const UNPACED: Self = Self(Duration::ZERO);

    /// A frame time of `micros` microseconds.
    pub(crate) fn from_micros(micros: u32) -> Self {
        Self(Duration::from_micros(u64::from(micros)))
    }

    /// The frame time in whole microseconds.
    pub(crate) fn as_micros(self) -> u128 {
        self.0.as_micros()
    }

    /// The frame time as a duration.
    pub(crate) const fn duration(self) -> Duration {
        self.0
    }

    /// Whether frames are not paced at all.
    pub(crate) const fn is_unpaced(self) -> bool {
        self.0.is_zero()
    }
}

/// The game mode the title screen opens with (`configuration` spec, game
/// settings). "Two players and two diggers" has no value here: the
/// combination is refused before settings are resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum GameMode {
    /// One player.
    #[default]
    OnePlayer,
    /// Two players taking turns.
    TwoPlayers,
    /// Two players at once, two Diggers.
    TwoDiggers,
    /// One player against the clock.
    Gauntlet,
    /// Two Diggers against the clock.
    TwoDiggerGauntlet,
}

impl GameMode {
    /// Every mode, in the order of the configuration file's list.
    pub(crate) const ALL: [Self; 5] = [
        Self::OnePlayer,
        Self::TwoPlayers,
        Self::TwoDiggers,
        Self::Gauntlet,
        Self::TwoDiggerGauntlet,
    ];

    /// The names of [`GameMode::ALL`], in order.
    pub(crate) const NAMES: [&'static str; 5] = [
        "one-player",
        "two-players",
        "two-diggers",
        "gauntlet",
        "two-digger-gauntlet",
    ];

    /// The mode's name in the configuration file.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::OnePlayer => "one-player",
            Self::TwoPlayers => "two-players",
            Self::TwoDiggers => "two-diggers",
            Self::Gauntlet => "gauntlet",
            Self::TwoDiggerGauntlet => "two-digger-gauntlet",
        }
    }

    /// The mode with this configuration-file name.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.name() == name)
    }

    /// The mode selected by the command line's mode options as a whole
    /// (`configuration` spec, mode options and precedence): `None` when no
    /// mode option was given, so the configured mode applies.
    ///
    /// # Errors
    ///
    /// `--players 2` together with `--two-diggers` or `--gauntlet` names no
    /// mode; the error says which of the two was combined with it.
    pub(crate) const fn from_options(
        players: Option<u8>,
        two_diggers: bool,
        gauntlet: bool,
    ) -> Result<Option<Self>, ModeConflict> {
        let two_players = matches!(players, Some(2));
        if two_players && two_diggers {
            return Err(ModeConflict::TwoDiggers);
        }
        if two_players && gauntlet {
            return Err(ModeConflict::Gauntlet);
        }
        Ok(match (players, two_diggers, gauntlet) {
            (None, false, false) => None,
            (_, true, true) => Some(Self::TwoDiggerGauntlet),
            (_, true, false) => Some(Self::TwoDiggers),
            (_, false, true) => Some(Self::Gauntlet),
            (Some(2), false, false) => Some(Self::TwoPlayers),
            (Some(_), false, false) => Some(Self::OnePlayer),
        })
    }

    /// Whether the mode has two Diggers on the field at once.
    pub(crate) const fn two_diggers(self) -> bool {
        matches!(self, Self::TwoDiggers | Self::TwoDiggerGauntlet)
    }

    /// Whether the mode is played against the clock.
    pub(crate) const fn gauntlet(self) -> bool {
        matches!(self, Self::Gauntlet | Self::TwoDiggerGauntlet)
    }
}

impl fmt::Display for GameMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Which mode option `--players 2` was combined with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModeConflict {
    /// `--players 2` with `--two-diggers`.
    TwoDiggers,
    /// `--players 2` with `--gauntlet`.
    Gauntlet,
}

/// How the 640×400 picture is fitted into the window (`configuration`
/// spec, video settings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum ScaleMode {
    /// The largest whole multiple that fits.
    #[default]
    Integer,
    /// The largest size that keeps the aspect ratio.
    Fit,
    /// The whole window.
    Stretch,
}

impl ScaleMode {
    /// Every scale mode.
    pub(crate) const ALL: [Self; 3] = [Self::Integer, Self::Fit, Self::Stretch];

    /// The names of [`ScaleMode::ALL`], in order.
    pub(crate) const NAMES: [&'static str; 3] = ["integer", "fit", "stretch"];

    /// The mode's name in the configuration file and on the command line.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Fit => "fit",
            Self::Stretch => "stretch",
        }
    }

    /// The scale mode with this name.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.name() == name)
    }
}

impl fmt::Display for ScaleMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The resolved settings: one value for every key of the configuration
/// file.
#[derive(Debug, Clone, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the struct mirrors the configuration file, whose on/off settings are booleans"
)]
pub(crate) struct Settings {
    /// `game.speed`.
    pub(crate) speed: Speed,
    /// `game.mode`.
    pub(crate) mode: GameMode,
    /// `game.gauntlet_time`, seconds.
    pub(crate) gauntlet_time: u16,
    /// `game.start_level`.
    pub(crate) start_level: u16,
    /// `game.unlimited_lives`.
    pub(crate) unlimited_lives: bool,
    /// `sound.enabled`.
    pub(crate) sound_enabled: bool,
    /// `sound.music`.
    pub(crate) music: bool,
    /// `sound.sample_rate`, Hz.
    pub(crate) sample_rate: u32,
    /// `sound.buffer_size`, frames.
    pub(crate) buffer_size: u32,
    /// `sound.volume`, percent.
    pub(crate) volume: u8,
    /// `video.fullscreen`.
    pub(crate) fullscreen: bool,
    /// `video.scale`.
    pub(crate) scale: ScaleMode,
    /// `video.reduce_flashing`.
    pub(crate) reduce_flashing: bool,
    /// `[keys]`: the keys of each action, indexed by [`Action::index`].
    pub(crate) keys: [Vec<KeyName>; Action::COUNT],
    /// `gamepad.enabled`.
    pub(crate) pad_enabled: bool,
    /// `gamepad.dead_zone`, a fraction of full deflection.
    pub(crate) dead_zone: f64,
    /// `gamepad.fire`, `pause`, `change_mode`, `quit`, indexed by
    /// [`PadAction::index`].
    pub(crate) pad_buttons: [Vec<PadButton>; PadAction::COUNT],
}

impl Default for Settings {
    /// The built-in defaults, taken from the schema.
    fn default() -> Self {
        let mut settings = Self {
            speed: Speed::DEFAULT,
            mode: GameMode::OnePlayer,
            gauntlet_time: 0,
            start_level: 0,
            unlimited_lives: false,
            sound_enabled: false,
            music: false,
            sample_rate: 0,
            buffer_size: 0,
            volume: 0,
            fullscreen: false,
            scale: ScaleMode::Integer,
            reduce_flashing: false,
            keys: Default::default(),
            pad_enabled: false,
            dead_zone: 0.0,
            pad_buttons: Default::default(),
        };
        settings.apply(&Layer::defaults());
        settings
    }
}

impl Settings {
    /// The settings in effect: the defaults, overridden by the file layer,
    /// overridden by the command-line layer (`configuration` spec,
    /// precedence). The command line's mode is already "the command line
    /// as a whole" (see [`GameMode::from_options`]): when it is `Some`,
    /// the file's mode is not consulted.
    pub(crate) fn resolve(file: &Layer, cli: &Layer) -> Self {
        let mut settings = Self::default();
        settings.apply(file);
        settings.apply(cli);
        settings
    }

    /// Overrides every field that `layer` has a value for.
    pub(crate) fn apply(&mut self, layer: &Layer) {
        fn take<T: Clone>(target: &mut T, source: Option<&T>) {
            if let Some(value) = source {
                target.clone_from(value);
            }
        }
        take(&mut self.speed, layer.speed.as_ref());
        take(&mut self.mode, layer.mode.as_ref());
        take(&mut self.gauntlet_time, layer.gauntlet_time.as_ref());
        take(&mut self.start_level, layer.start_level.as_ref());
        take(&mut self.unlimited_lives, layer.unlimited_lives.as_ref());
        take(&mut self.sound_enabled, layer.sound_enabled.as_ref());
        take(&mut self.music, layer.music.as_ref());
        take(&mut self.sample_rate, layer.sample_rate.as_ref());
        take(&mut self.buffer_size, layer.buffer_size.as_ref());
        take(&mut self.volume, layer.volume.as_ref());
        take(&mut self.fullscreen, layer.fullscreen.as_ref());
        take(&mut self.scale, layer.scale.as_ref());
        take(&mut self.reduce_flashing, layer.reduce_flashing.as_ref());
        for (target, source) in self.keys.iter_mut().zip(&layer.keys) {
            take(target, source.as_ref());
        }
        take(&mut self.pad_enabled, layer.pad_enabled.as_ref());
        take(&mut self.dead_zone, layer.dead_zone.as_ref());
        for (target, source) in self.pad_buttons.iter_mut().zip(&layer.pad_buttons) {
            take(target, source.as_ref());
        }
    }

    /// The keys bound to `action`.
    pub(crate) fn keys_of(&self, action: Action) -> &[KeyName] {
        self.keys.get(action.index()).map_or(&[], Vec::as_slice)
    }

    /// The buttons bound to `action`.
    pub(crate) fn buttons_of(&self, action: PadAction) -> &[PadButton] {
        self.pad_buttons
            .get(action.index())
            .map_or(&[], Vec::as_slice)
    }

    /// The first key bound to two actions, with both actions in table
    /// order.
    pub(crate) fn duplicate_key(&self) -> Option<(KeyName, Action, Action)> {
        let mut owners: Vec<(KeyName, Action)> = Vec::new();
        for action in Action::ALL {
            for &key in self.keys_of(action) {
                if let Some(&(_, first)) = owners.iter().find(|(known, _)| *known == key) {
                    return Some((key, first, action));
                }
                owners.push((key, action));
            }
        }
        None
    }

    /// The first gamepad button bound to two pad actions.
    pub(crate) fn duplicate_button(&self) -> Option<(PadButton, PadAction, PadAction)> {
        let mut owners: Vec<(PadButton, PadAction)> = Vec::new();
        for action in PadAction::ALL {
            for &button in self.buttons_of(action) {
                if let Some(&(_, first)) = owners.iter().find(|(known, _)| *known == button) {
                    return Some((button, first, action));
                }
                owners.push((button, action));
            }
        }
        None
    }

    /// The value of one setting, for writing the file.
    pub(crate) fn value(&self, field: Field) -> Value {
        match field {
            Field::Speed => Value::Integer(i64::from(self.speed.get())),
            Field::Mode => Value::Choice(self.mode.name()),
            Field::GauntletTime => Value::Integer(i64::from(self.gauntlet_time)),
            Field::StartLevel => Value::Integer(i64::from(self.start_level)),
            Field::UnlimitedLives => Value::Boolean(self.unlimited_lives),
            Field::SoundEnabled => Value::Boolean(self.sound_enabled),
            Field::Music => Value::Boolean(self.music),
            Field::SampleRate => Value::Integer(i64::from(self.sample_rate)),
            Field::BufferSize => Value::Integer(i64::from(self.buffer_size)),
            Field::Volume => Value::Integer(i64::from(self.volume)),
            Field::Fullscreen => Value::Boolean(self.fullscreen),
            Field::Scale => Value::Choice(self.scale.name()),
            Field::ReduceFlashing => Value::Boolean(self.reduce_flashing),
            Field::Key(action) => Value::Keys(self.keys_of(action).to_vec()),
            Field::PadEnabled => Value::Boolean(self.pad_enabled),
            Field::DeadZone => Value::Float(self.dead_zone),
            Field::PadButtons(action) => Value::Buttons(self.buttons_of(action).to_vec()),
        }
    }
}

/// A partial set of settings: the configuration file, the command line or
/// the changes the player made at run time. `None` means "not given here".
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Layer {
    /// `game.speed`.
    pub(crate) speed: Option<Speed>,
    /// `game.mode`.
    pub(crate) mode: Option<GameMode>,
    /// `game.gauntlet_time`.
    pub(crate) gauntlet_time: Option<u16>,
    /// `game.start_level`.
    pub(crate) start_level: Option<u16>,
    /// `game.unlimited_lives`.
    pub(crate) unlimited_lives: Option<bool>,
    /// `sound.enabled`.
    pub(crate) sound_enabled: Option<bool>,
    /// `sound.music`.
    pub(crate) music: Option<bool>,
    /// `sound.sample_rate`.
    pub(crate) sample_rate: Option<u32>,
    /// `sound.buffer_size`.
    pub(crate) buffer_size: Option<u32>,
    /// `sound.volume`.
    pub(crate) volume: Option<u8>,
    /// `video.fullscreen`.
    pub(crate) fullscreen: Option<bool>,
    /// `video.scale`.
    pub(crate) scale: Option<ScaleMode>,
    /// `video.reduce_flashing`.
    pub(crate) reduce_flashing: Option<bool>,
    /// `[keys]`, indexed by [`Action::index`].
    pub(crate) keys: [Option<Vec<KeyName>>; Action::COUNT],
    /// `gamepad.enabled`.
    pub(crate) pad_enabled: Option<bool>,
    /// `gamepad.dead_zone`.
    pub(crate) dead_zone: Option<f64>,
    /// The `[gamepad]` button lists, indexed by [`PadAction::index`].
    pub(crate) pad_buttons: [Option<Vec<PadButton>>; PadAction::COUNT],
}

impl Layer {
    /// The built-in defaults: every field set, from the schema.
    pub(crate) fn defaults() -> Self {
        let mut layer = Self::default();
        for entry in schema::schema() {
            layer.set(entry.field, entry.default_value());
        }
        layer
    }

    /// Whether no field is set.
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Sets one field from a value already checked against the schema. A
    /// value of the wrong shape for the field leaves the field unchanged
    /// (the schema rules it out; the default-value test proves it).
    pub(crate) fn set(&mut self, field: Field, value: Value) {
        match (field, value) {
            (Field::Speed, Value::Integer(v)) => {
                self.speed = u16::try_from(v).ok().and_then(Speed::new);
            }
            (Field::Mode, Value::Choice(name)) => self.mode = GameMode::from_name(name),
            (Field::GauntletTime, Value::Integer(v)) => self.gauntlet_time = u16::try_from(v).ok(),
            (Field::StartLevel, Value::Integer(v)) => self.start_level = u16::try_from(v).ok(),
            (Field::UnlimitedLives, Value::Boolean(v)) => self.unlimited_lives = Some(v),
            (Field::SoundEnabled, Value::Boolean(v)) => self.sound_enabled = Some(v),
            (Field::Music, Value::Boolean(v)) => self.music = Some(v),
            (Field::SampleRate, Value::Integer(v)) => self.sample_rate = u32::try_from(v).ok(),
            (Field::BufferSize, Value::Integer(v)) => self.buffer_size = u32::try_from(v).ok(),
            (Field::Volume, Value::Integer(v)) => self.volume = u8::try_from(v).ok(),
            (Field::Fullscreen, Value::Boolean(v)) => self.fullscreen = Some(v),
            (Field::Scale, Value::Choice(name)) => self.scale = ScaleMode::from_name(name),
            (Field::ReduceFlashing, Value::Boolean(v)) => self.reduce_flashing = Some(v),
            (Field::Key(action), Value::Keys(keys)) => {
                if let Some(slot) = self.keys.get_mut(action.index()) {
                    *slot = Some(keys);
                }
            }
            (Field::PadEnabled, Value::Boolean(v)) => self.pad_enabled = Some(v),
            (Field::DeadZone, Value::Float(v)) => self.dead_zone = Some(v),
            (Field::PadButtons(action), Value::Buttons(buttons)) => {
                if let Some(slot) = self.pad_buttons.get_mut(action.index()) {
                    *slot = Some(buttons);
                }
            }
            _ => debug_assert!(false, "value of the wrong shape for {field:?}"),
        }
    }

    /// Whether this layer sets the keys of `action`.
    pub(crate) fn sets_keys(&self, action: Action) -> bool {
        self.keys.get(action.index()).is_some_and(Option::is_some)
    }

    /// Whether this layer sets the buttons of `action`.
    pub(crate) fn sets_buttons(&self, action: PadAction) -> bool {
        self.pad_buttons
            .get(action.index())
            .is_some_and(Option::is_some)
    }

    /// Overrides every field `other` has a value for.
    pub(crate) fn merge(&mut self, other: &Self) {
        fn take<T: Clone>(target: &mut Option<T>, source: Option<&T>) {
            if let Some(value) = source {
                *target = Some(value.clone());
            }
        }
        take(&mut self.speed, other.speed.as_ref());
        take(&mut self.mode, other.mode.as_ref());
        take(&mut self.gauntlet_time, other.gauntlet_time.as_ref());
        take(&mut self.start_level, other.start_level.as_ref());
        take(&mut self.unlimited_lives, other.unlimited_lives.as_ref());
        take(&mut self.sound_enabled, other.sound_enabled.as_ref());
        take(&mut self.music, other.music.as_ref());
        take(&mut self.sample_rate, other.sample_rate.as_ref());
        take(&mut self.buffer_size, other.buffer_size.as_ref());
        take(&mut self.volume, other.volume.as_ref());
        take(&mut self.fullscreen, other.fullscreen.as_ref());
        take(&mut self.scale, other.scale.as_ref());
        take(&mut self.reduce_flashing, other.reduce_flashing.as_ref());
        for (target, source) in self.keys.iter_mut().zip(&other.keys) {
            take(target, source.as_ref());
        }
        take(&mut self.pad_enabled, other.pad_enabled.as_ref());
        take(&mut self.dead_zone, other.dead_zone.as_ref());
        for (target, source) in self.pad_buttons.iter_mut().zip(&other.pad_buttons) {
            take(target, source.as_ref());
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::{FrameTime, GameMode, Layer, ModeConflict, ScaleMode, Settings, Speed};
    use crate::input::{Action, KeyName, PadAction, PadButton};

    #[test]
    fn speed_and_frame_time() {
        assert_eq!(Speed::new(500).map(Speed::get), Some(500));
        assert_eq!(Speed::new(501), None);
        assert_eq!(Speed::DEFAULT.frame_time().as_micros(), 80_000);
        assert_eq!(
            Speed::new(30).map(|s| s.frame_time().as_micros()),
            Some(60_000)
        );
        assert_eq!(
            Speed::new(60).map(|s| s.frame_time().as_micros()),
            Some(120_000)
        );
        assert_eq!(
            Speed::new(0).map(Speed::frame_time),
            Some(FrameTime::UNPACED)
        );
        assert!(FrameTime::UNPACED.is_unpaced());
        assert!(!FrameTime::from_micros(70_000).is_unpaced());
        assert_eq!(
            FrameTime::from_micros(70_000).duration(),
            std::time::Duration::from_millis(70)
        );
    }

    #[test]
    fn mode_and_scale_names() {
        for (mode, name) in GameMode::ALL.into_iter().zip(GameMode::NAMES) {
            assert_eq!(mode.name(), name);
            assert_eq!(GameMode::from_name(name), Some(mode));
        }
        for (mode, name) in ScaleMode::ALL.into_iter().zip(ScaleMode::NAMES) {
            assert_eq!(mode.name(), name);
            assert_eq!(ScaleMode::from_name(name), Some(mode));
        }
        assert_eq!(GameMode::from_name("solo"), None);
        assert!(GameMode::TwoDiggerGauntlet.two_diggers());
        assert!(GameMode::TwoDiggerGauntlet.gauntlet());
        assert!(!GameMode::TwoPlayers.two_diggers());
    }

    #[test]
    fn mode_from_options() {
        let mode = GameMode::from_options;
        assert_eq!(mode(None, false, false), Ok(None));
        assert_eq!(mode(Some(1), false, false), Ok(Some(GameMode::OnePlayer)));
        assert_eq!(mode(Some(2), false, false), Ok(Some(GameMode::TwoPlayers)));
        assert_eq!(mode(None, true, false), Ok(Some(GameMode::TwoDiggers)));
        assert_eq!(mode(None, false, true), Ok(Some(GameMode::Gauntlet)));
        assert_eq!(
            mode(None, true, true),
            Ok(Some(GameMode::TwoDiggerGauntlet))
        );
        assert_eq!(mode(Some(1), true, false), Ok(Some(GameMode::TwoDiggers)));
        assert_eq!(mode(Some(2), true, false), Err(ModeConflict::TwoDiggers));
        assert_eq!(mode(Some(2), false, true), Err(ModeConflict::Gauntlet));
    }

    #[test]
    fn precedence_cli_over_file() {
        let file = Layer {
            speed: Speed::new(20),
            volume: Some(50),
            ..Layer::default()
        };
        let cli = Layer {
            speed: Speed::new(50),
            ..Layer::default()
        };
        let settings = Settings::resolve(&file, &cli);
        assert_eq!(settings.speed.frame_time().as_micros(), 100_000);
        // The file wins over the default where the command line is silent.
        assert_eq!(settings.volume, 50);
        // The default applies where neither says anything.
        assert_eq!(settings.sample_rate, 44_100);
        // Without a command-line value the file's speed applies.
        let settings = Settings::resolve(&file, &Layer::default());
        assert_eq!(settings.speed.frame_time().as_micros(), 40_000);
        // Neither: the default 80000 µs.
        let settings = Settings::resolve(&Layer::default(), &Layer::default());
        assert_eq!(settings.speed.frame_time().as_micros(), 80_000);
    }

    #[test]
    fn mode_option_replaces_file_mode() {
        let file = Layer {
            mode: Some(GameMode::Gauntlet),
            gauntlet_time: Some(90),
            ..Layer::default()
        };
        // `--two-diggers` alone: the command line's mode as a whole is
        // "two diggers", not the file's gauntlet combined with it.
        let cli = Layer {
            mode: GameMode::from_options(None, true, false).ok().flatten(),
            ..Layer::default()
        };
        let settings = Settings::resolve(&file, &cli);
        assert_eq!(settings.mode, GameMode::TwoDiggers);
        // The file's gauntlet time still applies to a later gauntlet.
        assert_eq!(settings.gauntlet_time, 90);
        // No mode option: the file's mode is kept.
        let cli = Layer {
            mode: GameMode::from_options(None, false, false).ok().flatten(),
            ..Layer::default()
        };
        assert_eq!(Settings::resolve(&file, &cli).mode, GameMode::Gauntlet);
    }

    #[test]
    fn duplicates_found() {
        let mut settings = Settings::default();
        assert_eq!(settings.duplicate_key(), None);
        assert_eq!(settings.duplicate_button(), None);
        let space = KeyName::parse("Space").into_iter().collect::<Vec<_>>();
        settings.keys[Action::P1Fire.index()] = space;
        let found = settings
            .duplicate_key()
            .map(|(key, a, b)| (key.name(), a, b));
        assert_eq!(found, Some(("Space", Action::P1Fire, Action::Pause)));
        settings.pad_buttons[PadAction::Quit.index()] = vec![PadButton::Start];
        assert_eq!(
            settings.duplicate_button(),
            Some((PadButton::Start, PadAction::Pause, PadAction::Quit))
        );
    }

    #[test]
    fn layer_merge_and_empty() {
        assert!(Layer::default().is_empty());
        assert!(!Layer::defaults().is_empty());
        let mut base = Layer {
            music: Some(true),
            ..Layer::default()
        };
        base.merge(&Layer {
            music: Some(false),
            fullscreen: Some(true),
            ..Layer::default()
        });
        assert_eq!(base.music, Some(false));
        assert_eq!(base.fullscreen, Some(true));
        let mut settings = Settings::default();
        settings.apply(&base);
        assert!(!settings.music);
        assert!(settings.fullscreen);
    }
}
