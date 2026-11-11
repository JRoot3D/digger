//! The gauntlet clock (design D1).
//!
//! The clock counts in 1,193,181ths of a second as an unsigned 32-bit value
//! (game-flow spec, "Gauntlet clock"; research 01 §7.4). This module holds
//! its value, the time-out flag, the per-tick countdown and the bonus
//! addition; the countdown is driven by the digger update at the start of
//! every tick, main phase and wind-down alike.

use super::config::GauntletSeconds;

/// Clock units per second: the original's timer frequency.
pub(crate) const CLOCK_UNITS_PER_SECOND: u32 = 1_193_181;

/// Clock units taken off at every tick: the original's default frame
/// length in microseconds, whatever speed the game runs at (research 01
/// §7.4).
const UNITS_PER_TICK: u32 = 80_000;

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

    /// The countdown at the start of a tick's digger update: the game is
    /// marked timed out if fewer than 80,000 units are left, and 80,000
    /// units are taken off.
    ///
    /// **Quirk Q13:** the subtraction wraps around, so the tick that runs
    /// out leaves a value just below 2^32 (research 01 §7.4).
    pub(crate) const fn count_down(&mut self) {
        if self.units < UNITS_PER_TICK {
            self.timed_out = true;
        }
        self.units = self.units.wrapping_sub(UNITS_PER_TICK);
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
    fn countdown_marks_time_out_and_wraps() {
        let mut clock = GauntletClock::start(GauntletSeconds::DEFAULT);
        for _ in 0..1789 {
            clock.count_down();
        }
        assert!(!clock.timed_out());
        assert_eq!(clock.units(), 61_720);
        clock.count_down();
        assert!(clock.timed_out());
        assert_eq!(clock.units(), 4_294_949_016);
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
