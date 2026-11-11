//! Frame length from the configured speed; pure functions, no clock
//! (design D1).
//!
//! The intended real-time length of a frame is speed × 2000 microseconds,
//! 80,000 µs (12.5 frames per second) at the default speed of 40; speed 0
//! means "do not wait". The frontend owns the clock and uses this value to
//! pace its calls to `step`; no game result depends on it (game-flow spec,
//! "Speed setting and tick length"; research 01 §4, research 05 §6.3).

/// Microseconds of frame length per speed unit.
const MICROS_PER_SPEED_UNIT: u32 = 2000;
/// The default speed.
const DEFAULT_SPEED: u32 = 40;
/// The change of one "faster" or "slower" step, in microseconds.
const SPEED_STEP_MICROS: u32 = 10_000;

/// The intended real-time length of one frame, in microseconds; 0 means
/// frames follow each other without waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FrameLength(u32);

impl FrameLength {
    /// The length at the default speed of 40: 80,000 µs.
    pub const DEFAULT: Self = Self::from_speed(DEFAULT_SPEED);
    /// No waiting between frames (speed 0).
    pub const NO_WAIT: Self = Self(0);

    /// The length for `speed`: speed × 2000 µs, saturating; speed 0 gives
    /// [`Self::NO_WAIT`].
    pub const fn from_speed(speed: u32) -> Self {
        Self(speed.saturating_mul(MICROS_PER_SPEED_UNIT))
    }

    /// The length of `micros` microseconds; 0 is [`Self::NO_WAIT`].
    pub const fn from_micros(micros: u32) -> Self {
        Self(micros)
    }

    /// The length in microseconds; 0 for no waiting.
    pub const fn micros(self) -> u32 {
        self.0
    }

    /// Whether the frontend should wait between frames at all.
    pub const fn waits(self) -> bool {
        self.0 != 0
    }

    /// One "faster" step: 10,000 µs shorter, but only while the length
    /// exceeds 10,000 µs (research 01 §4).
    #[must_use]
    pub const fn faster(self) -> Self {
        if self.0 > SPEED_STEP_MICROS {
            Self(self.0 - SPEED_STEP_MICROS)
        } else {
            self
        }
    }

    /// One "slower" step: 10,000 µs longer, saturating (research 01 §4).
    #[must_use]
    pub const fn slower(self) -> Self {
        Self(self.0.saturating_add(SPEED_STEP_MICROS))
    }
}

impl Default for FrameLength {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_speed_is_80000_us() {
        assert_eq!(FrameLength::default().micros(), 80_000);
        assert_eq!(FrameLength::DEFAULT, FrameLength::from_speed(40));
        assert_eq!(FrameLength::from_speed(1).micros(), 2000);
        assert!(FrameLength::DEFAULT.waits());
    }

    #[test]
    fn faster_stops_at_10000() {
        let length = FrameLength::from_micros(30_000);
        assert_eq!(length.faster().micros(), 20_000);
        assert_eq!(length.faster().faster().micros(), 10_000);
        assert_eq!(FrameLength::from_micros(10_000).faster().micros(), 10_000);
        // Above 10,000 the full step is taken, even below 10,000 after it.
        assert_eq!(FrameLength::from_micros(12_000).faster().micros(), 2000);
        assert_eq!(FrameLength::from_micros(2000).faster().micros(), 2000);
    }

    #[test]
    fn slower_adds_10000_saturating() {
        assert_eq!(FrameLength::DEFAULT.slower().micros(), 90_000);
        assert_eq!(FrameLength::NO_WAIT.slower().micros(), 10_000);
        assert_eq!(
            FrameLength::from_micros(u32::MAX).slower().micros(),
            u32::MAX
        );
        assert_eq!(FrameLength::from_speed(u32::MAX).micros(), u32::MAX);
    }

    #[test]
    fn speed_zero_means_no_wait() {
        let length = FrameLength::from_speed(0);
        assert_eq!(length, FrameLength::NO_WAIT);
        assert_eq!(length.micros(), 0);
        assert!(!length.waits());
        assert_eq!(length.faster(), FrameLength::NO_WAIT);
    }
}
