//! The gauntlet clock (design D1).
//!
//! The clock counts in 1,193,181ths of a second as an unsigned 32-bit value
//! (game-flow spec, "Gauntlet clock"; research 01 §7.4). This module holds
//! its value, the time-out flag and the bonus addition; the per-tick
//! countdown and the time-out are driven by the digger update (task 14.7).

use super::config::GauntletSeconds;

/// Clock units per second: the original's timer frequency.
pub(crate) const CLOCK_UNITS_PER_SECOND: u32 = 1_193_181;

/// Clock units a scoring bonus adds in gauntlet mode: 15 seconds
/// (scoring spec, "Bonus time in gauntlet mode"; research 01 §6.2).
const BONUS_UNITS: u32 = 17_897_715;

/// The gauntlet clock of a game in a gauntlet mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GauntletClock {
    /// The time left, in clock units.
    units: u32,
    /// Whether the game has run out of time.
    timed_out: bool,
}

impl GauntletClock {
    /// The clock at game start: `time` × 1,193,181 units, not yet timed
    /// out. The longest duration, 3599 seconds, still fits in 32 bits.
    pub(crate) fn start(time: GauntletSeconds) -> Self {
        Self {
            units: u32::from(time.seconds()) * CLOCK_UNITS_PER_SECOND,
            timed_out: false,
        }
    }

    /// The time left, in clock units.
    pub(crate) const fn units(self) -> u32 {
        self.units
    }

    /// Whether the game has run out of time.
    pub(crate) const fn timed_out(self) -> bool {
        self.timed_out
    }

    /// Adds the 15-second bonus.
    ///
    /// **Quirk Q13:** the addition wraps around past 2^32 for durations
    /// near the maximum, as in the original (research 01 §7.4).
    pub(crate) const fn add_bonus(&mut self) {
        self.units = self.units.wrapping_add(BONUS_UNITS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_starts_at_seconds_times_1193181() {
        let clock = GauntletClock::start(GauntletSeconds::DEFAULT);
        assert_eq!(clock.units(), 143_181_720);
        assert!(!clock.timed_out());
        assert_eq!(
            GauntletClock::start(GauntletSeconds::MAX).units(),
            4_294_258_419
        );
    }

    #[test]
    fn bonus_adds_17897715_with_wrap() {
        let mut clock = GauntletClock::start(GauntletSeconds::DEFAULT);
        clock.add_bonus();
        assert_eq!(clock.units(), 143_181_720 + 17_897_715);
        let mut clock = GauntletClock::start(GauntletSeconds::MAX);
        clock.add_bonus();
        assert_eq!(clock.units(), 4_294_258_419_u32.wrapping_add(17_897_715));
    }
}
