//! The emerald click and the emerald scale (research 04 §2).

use super::{Effect, Step};
use crate::command::EmeraldStep;
use crate::units::Divisor;

/// The divisors of the rising C major scale, steps 0 to 7: 523, 587, 659,
/// 698, 783, 879, 987 and 1046 Hz.
pub const EMERALD_DIVISORS: [Divisor; 8] = [
    Divisor(2280),
    Divisor(2032),
    Divisor(1810),
    Divisor(1708),
    Divisor(1522),
    Divisor(1356),
    Divisor(1208),
    Divisor(1140),
];

/// The divisor of scale step `step`.
///
/// `EmeraldStep` only holds 0..=7, so the table lookup cannot fail; the
/// original indexed its table without a check (fixed quirk SF9).
pub fn emerald_divisor(step: EmeraldStep) -> Divisor {
    match EMERALD_DIVISORS.get(step.index()) {
        Some(&divisor) => divisor,
        None => unreachable!("an EmeraldStep is at most 7"),
    }
}

/// The click of an eaten emerald: divisor 1000 (1193 Hz) for one tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Click;

impl Effect for Click {
    fn tick(&mut self) -> Step {
        Step {
            offer: Some(Divisor(1000)),
            done: true,
        }
    }
}

/// One note of the emerald scale: seven bursts of 2 ticks of tone and 6
/// of nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EmeraldScale {
    divisor: Divisor,
    /// Bursts still to play, counting the current one.
    bursts_left: u8,
    /// Position in the current burst, 0..=7.
    position: u8,
}

impl EmeraldScale {
    /// The note of `step`, at the start of its first burst.
    pub(crate) fn new(step: EmeraldStep) -> Self {
        Self {
            divisor: emerald_divisor(step),
            bursts_left: 7,
            position: 0,
        }
    }
}

impl Effect for EmeraldScale {
    fn tick(&mut self) -> Step {
        let offer = (self.position < 2).then_some(self.divisor);
        self.position += 1;
        if self.position == 8 {
            self.position = 0;
            self.bursts_left = self.bursts_left.saturating_sub(1);
        }
        Step {
            offer,
            done: self.bursts_left == 0,
        }
    }
}
