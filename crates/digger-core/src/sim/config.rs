//! Game configuration: modes, start level, gauntlet duration and their
//! validation (design D2, D10).
//!
//! The invalid combinations of the original's separate settings cannot be
//! expressed as a [`GameMode`]; [`GameMode::from_parts`] is where they are
//! rejected for callers that hold raw numbers (recording headers,
//! configuration files). [`StartLevel`] and [`GauntletSeconds`] are
//! validated newtypes, so a [`GameConfig`] is always playable
//! (research 01 §1.1, §1.2, §9; compat-quirks U03, U04).

use std::fmt;

use super::level::{LevelNumber, LevelSet};

/// Default gauntlet duration in seconds, also used for a requested
/// duration of 0 (research 01 §1.1, `/G`).
const DEFAULT_GAUNTLET_SECONDS: u16 = 120;
/// Longest gauntlet duration in seconds; longer requests are clamped
/// (research 01 §1.1, `/G`).
const MAX_GAUNTLET_SECONDS: u16 = 3599;

/// One of the five ways to play (game-flow spec, "Game modes").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameMode {
    /// One player with one digger.
    #[default]
    OnePlayer,
    /// Two players taking turns, one digger each, each with their own
    /// level, field, score and lives.
    TwoPlayers,
    /// Two diggers playing simultaneously on one shared field.
    TwoDiggers,
    /// One digger against a clock.
    Gauntlet {
        /// Length of the gauntlet.
        time: GauntletSeconds,
    },
    /// Two simultaneous diggers against a clock.
    GauntletTwoDiggers {
        /// Length of the gauntlet.
        time: GauntletSeconds,
    },
}

impl GameMode {
    /// The mode for settings that arrive as separate numbers: `players`
    /// alternating players (1 or 2), `diggers` simultaneous diggers (1 or
    /// 2) and an optional gauntlet duration in seconds, clamped as
    /// [`GauntletSeconds::new`] describes.
    ///
    /// # Errors
    ///
    /// [`ConfigError::PlayersOutOfRange`] or
    /// [`ConfigError::DiggersOutOfRange`] for a count other than 1 or 2;
    /// [`ConfigError::TwoPlayersWithTwoDiggers`] and
    /// [`ConfigError::TwoPlayersWithGauntlet`] for the combinations the
    /// original cannot play (compat-quirks U03).
    pub fn from_parts(
        players: u8,
        diggers: u8,
        gauntlet: Option<u32>,
    ) -> Result<Self, ConfigError> {
        if !(1..=2).contains(&players) {
            return Err(ConfigError::PlayersOutOfRange { value: players });
        }
        if !(1..=2).contains(&diggers) {
            return Err(ConfigError::DiggersOutOfRange { value: diggers });
        }
        let time = gauntlet.map(GauntletSeconds::new);
        match (players, diggers, time) {
            (2, 2, _) => Err(ConfigError::TwoPlayersWithTwoDiggers),
            (2, _, Some(_)) => Err(ConfigError::TwoPlayersWithGauntlet),
            (2, _, None) => Ok(Self::TwoPlayers),
            (_, 2, Some(time)) => Ok(Self::GauntletTwoDiggers { time }),
            (_, 2, None) => Ok(Self::TwoDiggers),
            (_, _, Some(time)) => Ok(Self::Gauntlet { time }),
            (_, _, None) => Ok(Self::OnePlayer),
        }
    }

    /// Number of players taking turns: 2 in [`GameMode::TwoPlayers`],
    /// otherwise 1.
    pub const fn players(self) -> u8 {
        match self {
            Self::TwoPlayers => 2,
            _ => 1,
        }
    }

    /// Number of diggers on the field at once: 2 in the two-digger modes,
    /// otherwise 1.
    pub const fn diggers(self) -> u8 {
        match self {
            Self::TwoDiggers | Self::GauntletTwoDiggers { .. } => 2,
            _ => 1,
        }
    }

    /// Whether two diggers play simultaneously.
    pub const fn two_diggers(self) -> bool {
        self.diggers() == 2
    }

    /// Whether two players take turns.
    pub const fn alternating(self) -> bool {
        matches!(self, Self::TwoPlayers)
    }

    /// The gauntlet duration, or `None` outside the gauntlet modes.
    pub const fn gauntlet(self) -> Option<GauntletSeconds> {
        match self {
            Self::Gauntlet { time } | Self::GauntletTwoDiggers { time } => Some(time),
            _ => None,
        }
    }
}

/// The level a game starts on, 1 to 1000 (game-flow spec, "Invalid
/// configurations are rejected"; compat-quirks U04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StartLevel(LevelNumber);

impl StartLevel {
    /// Level 1, the default.
    pub const FIRST: Self = Self(LevelNumber::FIRST);

    /// The start level `level`.
    ///
    /// # Errors
    ///
    /// [`ConfigError::StartLevelOutOfRange`] when `level` is outside
    /// 1..=1000.
    pub fn new(level: i64) -> Result<Self, ConfigError> {
        u16::try_from(level)
            .ok()
            .and_then(LevelNumber::new)
            .map(Self)
            .ok_or(ConfigError::StartLevelOutOfRange { value: level })
    }

    /// The level number.
    pub const fn level(self) -> LevelNumber {
        self.0
    }
}

impl Default for StartLevel {
    fn default() -> Self {
        Self::FIRST
    }
}

impl TryFrom<i64> for StartLevel {
    type Error = ConfigError;

    fn try_from(level: i64) -> Result<Self, ConfigError> {
        Self::new(level)
    }
}

impl From<LevelNumber> for StartLevel {
    fn from(level: LevelNumber) -> Self {
        Self(level)
    }
}

/// A gauntlet duration in whole seconds, 1 to 3599 (research 01 §1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GauntletSeconds(u16);

impl GauntletSeconds {
    /// The default duration, 120 seconds.
    pub const DEFAULT: Self = Self(DEFAULT_GAUNTLET_SECONDS);
    /// The longest duration, 3599 seconds.
    pub const MAX: Self = Self(MAX_GAUNTLET_SECONDS);

    /// The duration for a request of `seconds`: 0 is read as 120 and
    /// anything above 3599 as 3599, as the original's `/G` option does
    /// (game-flow spec, "Gauntlet duration limits").
    pub const fn new(seconds: u32) -> Self {
        if seconds == 0 {
            Self::DEFAULT
        } else if seconds > MAX_GAUNTLET_SECONDS as u32 {
            Self::MAX
        } else {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "in range 1..=3599 here, so the narrowing is exact"
            )]
            let seconds = seconds as u16;
            Self(seconds)
        }
    }

    /// The duration in seconds.
    pub const fn seconds(self) -> u16 {
        self.0
    }
}

impl Default for GauntletSeconds {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl From<u32> for GauntletSeconds {
    fn from(seconds: u32) -> Self {
        Self::new(seconds)
    }
}

/// Everything needed to start a game (design D2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GameConfig {
    /// The mode of play.
    pub mode: GameMode,
    /// The level every player starts on.
    pub start_level: StartLevel,
    /// Whether lives are never used up.
    pub unlimited_lives: bool,
    /// The eight level layouts and the bonus-life step.
    pub levels: LevelSet,
}

/// A configuration that cannot be played (design D10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConfigError {
    /// Two alternating players combined with two simultaneous diggers.
    TwoPlayersWithTwoDiggers,
    /// Two alternating players combined with gauntlet.
    TwoPlayersWithGauntlet,
    /// A player count other than 1 or 2.
    PlayersOutOfRange {
        /// The rejected count.
        value: u8,
    },
    /// A digger count other than 1 or 2.
    DiggersOutOfRange {
        /// The rejected count.
        value: u8,
    },
    /// A start level outside 1..=1000.
    StartLevelOutOfRange {
        /// The rejected level.
        value: i64,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TwoPlayersWithTwoDiggers => f.write_str(
                "two alternating players cannot be combined with two simultaneous diggers",
            ),
            Self::TwoPlayersWithGauntlet => {
                f.write_str("two alternating players cannot be combined with gauntlet mode")
            }
            Self::PlayersOutOfRange { value } => write!(
                f,
                "{value} players requested: the number of players must be 1 or 2"
            ),
            Self::DiggersOutOfRange { value } => write!(
                f,
                "{value} diggers requested: the number of diggers must be 1 or 2"
            ),
            Self::StartLevelOutOfRange { value } => {
                write!(f, "start level {value} is outside the valid range 1..1000")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_players_with_two_diggers_rejected() {
        assert_eq!(
            GameMode::from_parts(2, 2, None),
            Err(ConfigError::TwoPlayersWithTwoDiggers)
        );
        let message = ConfigError::TwoPlayersWithTwoDiggers.to_string();
        assert!(message.contains("two alternating players"), "{message}");
        assert!(message.contains("two simultaneous diggers"), "{message}");
        // Gauntlet does not make the combination valid.
        assert_eq!(
            GameMode::from_parts(2, 2, Some(60)),
            Err(ConfigError::TwoPlayersWithTwoDiggers)
        );
    }

    #[test]
    fn two_players_with_gauntlet_rejected() {
        assert_eq!(
            GameMode::from_parts(2, 1, Some(120)),
            Err(ConfigError::TwoPlayersWithGauntlet)
        );
        let message = ConfigError::TwoPlayersWithGauntlet.to_string();
        assert!(message.contains("two alternating players"), "{message}");
        assert!(message.contains("gauntlet"), "{message}");
    }

    #[test]
    fn valid_parts_give_each_mode() {
        let time = GauntletSeconds::new(90);
        assert_eq!(GameMode::from_parts(1, 1, None), Ok(GameMode::OnePlayer));
        assert_eq!(GameMode::from_parts(2, 1, None), Ok(GameMode::TwoPlayers));
        assert_eq!(GameMode::from_parts(1, 2, None), Ok(GameMode::TwoDiggers));
        assert_eq!(
            GameMode::from_parts(1, 1, Some(90)),
            Ok(GameMode::Gauntlet { time })
        );
        assert_eq!(
            GameMode::from_parts(1, 2, Some(90)),
            Ok(GameMode::GauntletTwoDiggers { time })
        );
        assert_eq!(
            GameMode::from_parts(0, 1, None),
            Err(ConfigError::PlayersOutOfRange { value: 0 })
        );
        assert_eq!(
            GameMode::from_parts(1, 3, None),
            Err(ConfigError::DiggersOutOfRange { value: 3 })
        );
    }

    #[test]
    fn mode_queries() {
        let time = GauntletSeconds::DEFAULT;
        let modes = [
            (GameMode::OnePlayer, 1, 1, None),
            (GameMode::TwoPlayers, 2, 1, None),
            (GameMode::TwoDiggers, 1, 2, None),
            (GameMode::Gauntlet { time }, 1, 1, Some(time)),
            (GameMode::GauntletTwoDiggers { time }, 1, 2, Some(time)),
        ];
        for (mode, players, diggers, gauntlet) in modes {
            assert_eq!(mode.players(), players, "{mode:?}");
            assert_eq!(mode.diggers(), diggers, "{mode:?}");
            assert_eq!(mode.alternating(), players == 2, "{mode:?}");
            assert_eq!(mode.two_diggers(), diggers == 2, "{mode:?}");
            assert_eq!(mode.gauntlet(), gauntlet, "{mode:?}");
        }
    }

    #[test]
    fn start_level_0_and_1001_rejected() {
        for level in [0, 1001, -1, 65_536] {
            let error = ConfigError::StartLevelOutOfRange { value: level };
            assert_eq!(StartLevel::new(level), Err(error));
            let message = error.to_string();
            assert!(message.contains("1..1000"), "{message}");
            assert!(message.contains(&level.to_string()), "{message}");
        }
        assert_eq!(StartLevel::new(1).map(|s| s.level().get()), Ok(1));
        assert_eq!(StartLevel::new(1000).map(|s| s.level().get()), Ok(1000));
        assert_eq!(StartLevel::default(), StartLevel::FIRST);
    }

    #[test]
    fn gauntlet_duration_clamped() {
        assert_eq!(GauntletSeconds::new(0).seconds(), 120);
        assert_eq!(GauntletSeconds::new(5000).seconds(), 3599);
        assert_eq!(GauntletSeconds::new(3599).seconds(), 3599);
        assert_eq!(GauntletSeconds::new(3600).seconds(), 3599);
        assert_eq!(GauntletSeconds::new(1).seconds(), 1);
        assert_eq!(GauntletSeconds::new(u32::MAX).seconds(), 3599);
        assert_eq!(
            GameMode::from_parts(1, 1, Some(0)).map(GameMode::gauntlet),
            Ok(Some(GauntletSeconds::new(120)))
        );
        assert_eq!(
            GameMode::from_parts(1, 2, Some(5000)).map(GameMode::gauntlet),
            Ok(Some(GauntletSeconds::new(3599)))
        );
    }

    #[test]
    fn default_config_is_one_player_level_1_builtin() {
        let config = GameConfig::default();
        assert_eq!(config.mode, GameMode::OnePlayer);
        assert_eq!(config.start_level, StartLevel::FIRST);
        assert!(!config.unlimited_lives);
        assert_eq!(config.levels, LevelSet::builtin());
    }
}
