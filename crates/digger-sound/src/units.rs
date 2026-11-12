//! Newtypes for the quantities the sound engine counts in (design D1, D13).
//!
//! Upstream keeps every one of these in a plain `int16_t`; here each has its
//! own type, so a divisor cannot be passed where a frequency is expected and
//! out-of-range values are rejected or clamped where they enter.

use core::fmt;

use crate::config::ConfigError;

/// The PC timer's input clock in Hz: a tone's frequency is this divided by
/// its [`Divisor`] (research 04 §1.2).
pub const TIMER_CLOCK_HZ: u32 = 1_193_181;

/// A pitch value as the original game stores it: the PC timer divisor.
///
/// The tone's frequency is [`TIMER_CLOCK_HZ`] divided by the divisor with
/// integer division (kept quirk SQ2). Whether a divisor is audible at all
/// (41..=16383) is decided by the voice stage, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Divisor(pub u16);

impl Divisor {
    /// The divisor's value.
    pub const fn get(self) -> u16 {
        self.0
    }

    /// The tone frequency, `1193181 / divisor` truncated to whole Hz.
    ///
    /// Divisor 0 has no frequency and gives 0 Hz; it is never audible.
    pub const fn hertz(self) -> Hertz {
        match TIMER_CLOCK_HZ.checked_div(self.0 as u32) {
            Some(hz) => Hertz(hz),
            None => Hertz(0),
        }
    }
}

impl fmt::Display for Divisor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A frequency in whole Hz.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hertz(pub u32);

impl Hertz {
    /// The frequency's value in Hz.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for Hertz {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} Hz", self.0)
    }
}

/// A count of sound ticks (about 13.74 ms each; research 04 §1.3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SoundTicks(pub u32);

impl SoundTicks {
    /// The number of ticks.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A music amplitude level, 1..=50 (research 04 §3.2).
///
/// Level 1 is silent and level 50 is full amplitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Level(u8);

impl Level {
    /// The lowest level, which is silent.
    pub const MIN: Self = Self(1);
    /// The highest level, full amplitude.
    pub const MAX: Self = Self(50);

    /// Level `n`, or `None` when `n` is outside 1..=50.
    pub const fn new(n: u8) -> Option<Self> {
        if n >= Self::MIN.0 && n <= Self::MAX.0 {
            Some(Self(n))
        } else {
            None
        }
    }

    /// Level `n` limited to 1..=50.
    pub const fn saturating(n: u8) -> Self {
        if n < Self::MIN.0 {
            Self::MIN
        } else if n > Self::MAX.0 {
            Self::MAX
        } else {
            Self(n)
        }
    }

    /// The level's value, 1..=50.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// The master volume in percent, 0..=100 (research 04 §4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Volume(u8);

impl Volume {
    /// Silence.
    pub const MUTE: Self = Self(0);
    /// Full volume, the default.
    pub const MAX: Self = Self(100);

    /// A volume of `percent`; values above 100 are treated as 100.
    pub const fn new(percent: u8) -> Self {
        if percent > Self::MAX.0 {
            Self::MAX
        } else {
            Self(percent)
        }
    }

    /// The volume in percent, 0..=100.
    pub const fn percent(self) -> u8 {
        self.0
    }
}

impl Default for Volume {
    fn default() -> Self {
        Self::MAX
    }
}

/// An output sample rate in Hz, validated to 8000..=192000 (research 04
/// §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SampleRate(u32);

impl SampleRate {
    /// The lowest supported rate.
    pub const MIN_HZ: u32 = 8_000;
    /// The highest supported rate.
    pub const MAX_HZ: u32 = 192_000;
    /// The default rate, 44100 Hz.
    pub const DEFAULT: Self = Self(44_100);

    /// The rate `hz`, or [`ConfigError::SampleRateOutOfRange`] when it is
    /// outside 8000..=192000.
    pub const fn new(hz: u32) -> Result<Self, ConfigError> {
        if hz >= Self::MIN_HZ && hz <= Self::MAX_HZ {
            Ok(Self(hz))
        } else {
            Err(ConfigError::SampleRateOutOfRange { hz })
        }
    }

    /// The rate in Hz.
    pub const fn hz(self) -> u32 {
        self.0
    }

    /// The number of samples in one sound tick: the rate divided by 72.8,
    /// rounded to the nearest integer, computed in integers as
    /// `(rate * 10 + 364) / 728` (design D7; 606 at 44100 Hz).
    pub const fn samples_per_tick(self) -> u32 {
        (self.0 * 10 + 364) / 728
    }
}

impl Default for SampleRate {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl TryFrom<u32> for SampleRate {
    type Error = ConfigError;

    fn try_from(hz: u32) -> Result<Self, ConfigError> {
        Self::new(hz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divisor_division_truncates() {
        assert_eq!(Divisor(2280).hertz(), Hertz(523));
        assert_eq!(Divisor(15000).hertz(), Hertz(79));
        assert_eq!(Divisor(16383).hertz(), Hertz(72));
        assert_eq!(Divisor(0).hertz(), Hertz(0));
    }

    #[test]
    fn samples_per_tick_table() {
        for (hz, n) in [
            (8_000, 110),
            (22_050, 303),
            (44_100, 606),
            (48_000, 659),
            (96_000, 1319),
        ] {
            assert_eq!(
                SampleRate::new(hz).map(SampleRate::samples_per_tick),
                Ok(n),
                "{hz} Hz"
            );
        }
        assert_eq!(SampleRate::DEFAULT.samples_per_tick(), 606);
    }

    #[test]
    fn sample_rate_out_of_range_is_rejected() {
        for hz in [0, 4_000, 7_999, 192_001, u32::MAX] {
            assert_eq!(
                SampleRate::new(hz),
                Err(ConfigError::SampleRateOutOfRange { hz }),
                "{hz} Hz"
            );
        }
        assert!(SampleRate::new(8_000).is_ok());
        assert!(SampleRate::new(192_000).is_ok());
        let message = ConfigError::SampleRateOutOfRange { hz: 4_000 }.to_string();
        assert!(
            message.contains("8000") && message.contains("192000"),
            "{message}"
        );
    }

    #[test]
    fn level_range() {
        assert_eq!(Level::new(0), None);
        assert_eq!(Level::new(1), Some(Level::MIN));
        assert_eq!(Level::new(50), Some(Level::MAX));
        assert_eq!(Level::new(51), None);
        assert_eq!(Level::saturating(0), Level::MIN);
        assert_eq!(Level::saturating(200), Level::MAX);
        assert_eq!(Level::saturating(8).get(), 8);
    }

    #[test]
    fn volume_clamps_to_100() {
        assert_eq!(Volume::new(50).percent(), 50);
        assert_eq!(Volume::new(101), Volume::MAX);
        assert_eq!(Volume::new(255).percent(), 100);
        assert_eq!(Volume::default(), Volume::MAX);
    }
}
