//! The eat-monster effect (research 04 §2).

use super::{Effect, Step};
use crate::units::Divisor;

/// The value each chirp starts from.
const START: u16 = 2000;
/// Ticks of one chirp, followed by one tick of nothing.
const CHIRP: u8 = 20;

/// An eaten monster: three identical rising chirps of 20 ticks, each
/// followed by one tick of nothing (63 ticks).
///
/// In a chirp a value starts at 2000 and loses a sixteenth of itself on
/// every tick. A tone sounds on even ticks only: on ticks 2, 6, 10, ... of
/// a chirp at the value after that tick's change, on ticks 4, 8, 12, ...
/// at the value before it, as the original reads its value at two
/// different points of the update (research 04 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EatMonster {
    value: u16,
    /// Ticks played in the current chirp, 0..=CHIRP; CHIRP is its silent
    /// tick.
    tick: u8,
    /// Chirps still to play, counting the current one.
    chirps_left: u8,
}

impl EatMonster {
    /// The effect at the start of its first chirp.
    pub(crate) const fn new() -> Self {
        Self {
            value: START,
            tick: 0,
            chirps_left: 3,
        }
    }
}

impl Effect for EatMonster {
    fn tick(&mut self) -> Step {
        if self.tick == CHIRP {
            // The silent tick between chirps (and after the last one).
            self.tick = 0;
            self.value = START;
            self.chirps_left = self.chirps_left.saturating_sub(1);
            return Step {
                offer: None,
                done: self.chirps_left == 0,
            };
        }
        self.tick += 1;
        let before = self.value;
        self.value -= before >> 4;
        let offer = match self.tick % 4 {
            2 => Some(Divisor(self.value)),
            0 => Some(Divisor(before)),
            _ => None,
        };
        Step { offer, done: false }
    }
}
