//! The bag break effect (research 04 §2).

use super::{Effect, Step};
use crate::units::Divisor;

/// A breaking bag: divisor 15000 (79 Hz) for three ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Break {
    ticks_left: u8,
}

impl Break {
    /// A break with all three ticks to go.
    pub(crate) const fn new() -> Self {
        Self { ticks_left: 3 }
    }
}

impl Effect for Break {
    fn tick(&mut self) -> Step {
        self.ticks_left = self.ticks_left.saturating_sub(1);
        Step {
            offer: Some(Divisor(15_000)),
            done: self.ticks_left == 0,
        }
    }
}
