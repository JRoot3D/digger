//! The sound engine's own noise generator (design D3, kept quirk SQ9).
//!
//! The fireball flight effect jitters its pitch with values from this
//! generator. It belongs to the sound engine alone: it never reads or
//! advances the game's random numbers, starts at state 0 when the engine
//! is created and is not reseeded by a stop-all (research 04 §2, §5).

/// A 32-bit linear congruential generator: `state = state * 0x15A4E35 + 1`
/// modulo 2^32.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct NoiseRng {
    state: u32,
}

impl NoiseRng {
    /// The multiplier of the generator.
    const MULTIPLIER: u32 = 0x015A_4E35;

    /// A generator in its initial state, 0.
    pub const fn new() -> Self {
        Self { state: 0 }
    }

    /// The current state.
    pub const fn state(self) -> u32 {
        self.state
    }

    /// Advances the state and returns its low 31 bits modulo `range`.
    ///
    /// A `range` of 0 returns 0 (after advancing the state); the game never
    /// asks for one.
    pub const fn draw(&mut self, range: u32) -> u32 {
        self.state = self.state.wrapping_mul(Self::MULTIPLIER).wrapping_add(1);
        match (self.state & 0x7FFF_FFFF).checked_rem(range) {
            Some(value) => value,
            None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_first_values() {
        let mut rng = NoiseRng::new();
        assert_eq!(rng.state(), 0);
        assert_eq!(rng.draw(63), 1);
        assert_eq!(rng.state(), 1);
        assert_eq!(rng.draw(64), 54);
        assert_eq!(rng.state(), 22_695_478);
    }

    #[test]
    fn noise_wraps_and_uses_low_31_bits() {
        let mut rng = NoiseRng { state: u32::MAX };
        // u32::MAX * 0x15A4E35 + 1 wraps to 0x15A4E35 * (2^32 - 1) + 1,
        // which is -0x15A4E35 + 1 modulo 2^32.
        let expected = 0u32.wrapping_sub(NoiseRng::MULTIPLIER).wrapping_add(1);
        assert_eq!(rng.draw(u32::MAX), expected & 0x7FFF_FFFF);
        assert_eq!(rng.state(), expected);
        assert_eq!(rng.draw(0), 0);
    }
}
