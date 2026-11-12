//! Engine configuration and its errors (design D2, D13).

use core::fmt;

use crate::units::{SampleRate, Volume};

/// The settings a sound engine is created with.
///
/// Every field can also be changed later through a
/// [`SoundControl`](crate::command::SoundControl), except the sample rate,
/// which is fixed for the engine's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SoundConfig {
    /// The output sample rate; 44100 Hz by default.
    pub sample_rate: SampleRate,
    /// Whether sound is on (not muted) at the start; on by default.
    pub sound_on: bool,
    /// Whether music is on at the start; on by default.
    pub music_on: bool,
    /// The master volume; 100 % by default.
    pub volume: Volume,
    /// Whether the output filter (high-pass 1 kHz, low-pass 4 kHz) is on;
    /// on by default.
    pub filter: bool,
}

impl SoundConfig {
    /// The default configuration: 44100 Hz, sound on, music on, volume
    /// 100 %, filter on.
    pub const DEFAULT: Self = Self {
        sample_rate: SampleRate::DEFAULT,
        sound_on: true,
        music_on: true,
        volume: Volume::MAX,
        filter: true,
    };
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// An invalid engine configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConfigError {
    /// The sample rate is outside 8000..=192000 Hz.
    SampleRateOutOfRange {
        /// The rejected rate in Hz.
        hz: u32,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SampleRateOutOfRange { hz } => write!(
                f,
                "sample rate {hz} Hz is not supported; the supported range is {}..={} Hz",
                SampleRate::MIN_HZ,
                SampleRate::MAX_HZ
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_spec() {
        let config = SoundConfig::default();
        assert_eq!(config.sample_rate.hz(), 44_100);
        assert!(config.sound_on);
        assert!(config.music_on);
        assert_eq!(config.volume.percent(), 100);
        assert!(config.filter);
        assert_eq!(config, SoundConfig::DEFAULT);
    }
}
