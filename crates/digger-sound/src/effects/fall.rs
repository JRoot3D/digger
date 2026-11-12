//! The bag fall effect (research 04 §2).

use super::{Effect, Step};
use crate::units::Divisor;

/// The divisor of the first cycle (1193 Hz).
const START: u16 = 1000;
/// How much the divisor rises per cycle.
const STEP: u16 = 50;

/// A falling bag: a 4-tick cycle with a tone on its third tick, the pitch
/// falling by 50 divisor units per cycle.
///
/// A fall-start always constructs a fresh state, so a restart begins the
/// same cycle at divisor 1000 (fixed quirk SF6: the original kept the
/// on/off phase across a stop and a restart).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fall {
    divisor: u16,
    /// Position in the cycle, 0..=3.
    phase: u8,
}

impl Fall {
    /// A fall at the start of its first cycle.
    pub(crate) const fn new() -> Self {
        Self {
            divisor: START,
            phase: 0,
        }
    }
}

impl Effect for Fall {
    fn tick(&mut self) -> Step {
        let phase = self.phase;
        self.phase = (phase + 1) % 4;
        match phase {
            2 => Step::offer(Divisor(self.divisor)),
            3 => {
                self.divisor += STEP;
                Step::quiet()
            }
            _ => Step::quiet(),
        }
    }
}
