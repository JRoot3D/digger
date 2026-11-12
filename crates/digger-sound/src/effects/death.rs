//! The digger death effect (research 04 §2).

use super::Step;
use crate::units::Divisor;

/// What the death effect did on one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeathStep {
    /// The offer and whether the effect ended.
    pub(crate) step: Step,
    /// Whether any tune is to be stopped: true on the effect's first tick
    /// only (design D3). The effect never touches the music itself.
    pub(crate) stop_music: bool,
}

/// A dying digger: 51 ticks, the divisor falling from 19000 to 10000 by
/// 1000 per tick over ticks 1 to 10 and then rising by 500 per tick until
/// the tick on which it exceeds 30000.
///
/// The effect offers its divisor on every tick, so it displaces music
/// throughout, but by the divisor gate it is heard only on ticks 4 to 22
/// (kept quirk SQ1, isolated in `Voice::tone`). It still sounds on the
/// tick on which it ends (kept quirk SQ5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Death {
    divisor: u16,
    /// Ticks played so far.
    tick: u8,
}

impl Death {
    /// The divisor after which the effect ends.
    const LAST: u16 = 30_000;

    /// A death effect at its first tick.
    pub(crate) const fn new() -> Self {
        Self {
            divisor: 20_000,
            tick: 0,
        }
    }

    /// Advances the effect by one tick.
    pub(crate) fn tick(&mut self) -> DeathStep {
        self.tick = self.tick.saturating_add(1);
        if self.tick <= 10 {
            self.divisor = 20_000 - 1000 * u16::from(self.tick);
        } else {
            self.divisor += 500;
        }
        DeathStep {
            step: Step {
                offer: Some(Divisor(self.divisor)),
                done: self.divisor > Self::LAST,
            },
            stop_music: self.tick == 1,
        }
    }
}
