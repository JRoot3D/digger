//! The fireball explosion effect (research 04 §2).

use super::{Effect, Step};
use crate::units::Divisor;

/// An exploding fireball: a 10-tick rising chirp, the divisor losing an
/// eighth of itself each tick from 1500 and then sounding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Explode {
    divisor: u16,
    ticks_left: u8,
}

impl Explode {
    /// An explosion at its first tick.
    pub(crate) const fn new() -> Self {
        Self {
            divisor: 1500,
            ticks_left: 10,
        }
    }
}

impl Effect for Explode {
    fn tick(&mut self) -> Step {
        self.divisor -= self.divisor >> 3;
        self.ticks_left = self.ticks_left.saturating_sub(1);
        Step {
            offer: Some(Divisor(self.divisor)),
            done: self.ticks_left == 0,
        }
    }
}
