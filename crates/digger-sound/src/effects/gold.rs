//! The gold effect (research 04 §2).

use super::{Effect, Step};
use crate::units::Divisor;

/// Collected gold: two interleaved sweeps for 31 ticks, the first falling
/// in divisor from 4000 by a sixteenth per tick and heard on odd ticks,
/// the second rising from 500 by a sixteenth per tick and heard on even
/// ticks.
///
/// The effect still sounds on the tick on which it ends, its 31st (kept
/// quirk SQ5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Gold {
    rising: u16,
    falling: u16,
    /// Ticks played so far.
    tick: u8,
}

impl Gold {
    /// The number of ticks the effect sounds.
    pub(crate) const LENGTH: u8 = 31;

    /// Gold at its first tick.
    pub(crate) const fn new() -> Self {
        Self {
            rising: 500,
            falling: 4000,
            tick: 0,
        }
    }
}

impl Effect for Gold {
    fn tick(&mut self) -> Step {
        self.tick += 1;
        // Each value is read before this tick's change.
        let divisor = if self.tick % 2 == 1 {
            self.falling
        } else {
            self.rising
        };
        self.rising += self.rising >> 4;
        self.falling -= self.falling >> 4;
        Step {
            offer: Some(Divisor(divisor)),
            done: self.tick == Self::LENGTH,
        }
    }
}
