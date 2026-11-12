//! The bonus siren (research 04 §2).

use super::{Effect, Step};
use crate::units::Divisor;

/// The siren's high tone (970 Hz).
const HIGH: Divisor = Divisor(1230);
/// The siren's low tone (788 Hz).
const LOW: Divisor = Divisor(1513);

/// The two-tone siren before and at the end of bonus mode, in a 16-tick
/// cycle.
///
/// The simulation sends bonus-start on every frame of the flashing, so a
/// start for a running siren keeps its position (as the original's start
/// only raises a flag); only a bonus-stop resets it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Bonus {
    /// Position in the cycle, 0..=15; the first tick is position 1.
    position: u8,
}

impl Bonus {
    /// A siren at position 0, so that its first tick is position 1.
    pub(crate) const fn new() -> Self {
        Self { position: 0 }
    }
}

impl Effect for Bonus {
    fn tick(&mut self) -> Step {
        self.position = (self.position + 1) % 16;
        match self.position {
            0..=5 => Step::offer(HIGH),
            8..=13 => Step::offer(LOW),
            _ => Step::quiet(),
        }
    }
}
