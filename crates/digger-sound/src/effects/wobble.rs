//! The bag wobble effect (research 04 §2).

use super::{Effect, Step};
use crate::units::Divisor;

/// A wobbling bag: a one-tick blip every 16 ticks in a 64-tick cycle.
///
/// The simulation sends wobble-start again every other frame while a bag
/// wobbles, so a start for a running wobble keeps its position (as the
/// original's start only raises a flag); only a wobble-stop resets it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Wobble {
    /// Ticks since the start, modulo 64.
    count: u8,
}

impl Wobble {
    /// A wobble at the start of its cycle.
    pub(crate) const fn new() -> Self {
        Self { count: 0 }
    }
}

impl Effect for Wobble {
    fn tick(&mut self) -> Step {
        self.count = (self.count + 1) % 64;
        match self.count {
            0 => Step::offer(Divisor(2000)),
            16 | 48 => Step::offer(Divisor(2500)),
            32 => Step::offer(Divisor(3000)),
            _ => Step::quiet(),
        }
    }
}
