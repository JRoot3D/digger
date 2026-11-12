//! The extra-life effect (research 04 §2).

use super::{Effect, Step};
use crate::units::Divisor;

/// An extra life: a counter runs from 96 down to 1, one step per tick; on
/// ticks where the counter divided by 3 is odd the divisor counter x 4 +
/// 600 sounds, giving 3 ticks of tone and 3 of nothing with rising pitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExtraLife {
    counter: u16,
}

impl ExtraLife {
    /// The effect at its first tick, counter 96.
    pub(crate) const fn new() -> Self {
        Self { counter: 96 }
    }
}

impl Effect for ExtraLife {
    fn tick(&mut self) -> Step {
        let counter = self.counter;
        self.counter = counter.saturating_sub(1);
        Step {
            offer: ((counter / 3) % 2 == 1).then_some(Divisor(counter * 4 + 600)),
            done: self.counter == 0,
        }
    }
}
