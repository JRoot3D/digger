//! The game's random-number generator and per-round seeding (design D7).
//!
//! One linear congruential generator with a signed 32-bit state drives every
//! random decision of the game; sound has its own and never draws from this
//! one. At every round start the state is set to that round's seed, derived
//! from the game's base seed by `SplitMix64` unless a recording supplies it
//! (game-flow spec, "Random-number generator", "Per-round seeding";
//! research 01 §7.1, §7.2).

use std::num::NonZeroU8;

/// The generator's multiplier, 22,695,477 (research 01 §7.1).
const MULTIPLIER: i32 = 0x015A_4E35;
/// The generator's increment (research 01 §7.1).
const INCREMENT: i32 = 1;
/// The mask that clears the state's top bit before the modulus is taken.
const RESULT_MASK: i32 = 0x7FFF_FFFF;

/// `SplitMix64`'s index step, the golden-ratio increment (research 01 §7.2).
const SPLITMIX_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
/// `SplitMix64`'s first mixing multiplier.
const SPLITMIX_MIX_1: u64 = 0xBF58_476D_1CE4_E5B9;
/// `SplitMix64`'s second mixing multiplier.
const SPLITMIX_MIX_2: u64 = 0x94D0_49BB_1331_11EB;

/// The divisor of a draw: never zero, so a draw cannot divide by zero
/// (compat-quirks U04). The game's two draws use 5..=14 and 6..=15.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Modulus(NonZeroU8);

impl Modulus {
    /// The modulus `n`, or `None` for 0.
    pub const fn new(n: u8) -> Option<Self> {
        match NonZeroU8::new(n) {
            Some(n) => Some(Self(n)),
            None => None,
        }
    }

    /// The modulus from a difficulty formula whose result is always at
    /// least 5.
    pub(super) const fn from_difficulty(n: u8) -> Self {
        match Self::new(n) {
            Some(modulus) => modulus,
            None => unreachable!(),
        }
    }

    /// The modulus as a number, at least 1.
    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

/// The game's random-number generator (research 01 §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GameRng {
    state: i32,
    /// Draws taken since the generator was created, for tests that assert
    /// the draw pattern (design D7).
    #[cfg(any(test, feature = "testkit"))]
    draws: u64,
}

impl GameRng {
    /// A generator in state `seed`.
    pub const fn new(seed: i32) -> Self {
        Self {
            state: seed,
            #[cfg(any(test, feature = "testkit"))]
            draws: 0,
        }
    }

    /// Sets the state to `seed`, as at every round start.
    pub const fn reseed(&mut self, seed: i32) {
        self.state = seed;
    }

    /// The current state.
    pub const fn state(self) -> i32 {
        self.state
    }

    /// One draw: the state becomes state × 22,695,477 + 1 with 32-bit
    /// wrap-around (compat-quirks U06), and the result is the state with
    /// its top bit cleared, modulo `modulus`.
    pub fn draw(&mut self, modulus: Modulus) -> i16 {
        self.state = self.state.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
        #[cfg(any(test, feature = "testkit"))]
        {
            self.draws += 1;
        }
        let result = (self.state & RESULT_MASK) % i32::from(modulus.get());
        // The result is below the modulus, at most 254, so it fits.
        i16::try_from(result).unwrap_or(i16::MAX)
    }

    /// Draws taken since the generator was created.
    #[cfg(any(test, feature = "testkit"))]
    pub const fn draws(self) -> u64 {
        self.draws
    }
}

/// The derived seed of round `round` for base seed `base`: the low 32 bits
/// of the `SplitMix64` output for index `round`, all in 64-bit wrap-around
/// arithmetic (research 01 §7.2; compat-quirks U06). The index step
/// `round + 1` wraps in 32 bits, as in the original.
pub const fn round_seed(base: u64, round: u32) -> i32 {
    let index = round.wrapping_add(1) as u64;
    let mut z = base.wrapping_add(index.wrapping_mul(SPLITMIX_GAMMA));
    z = (z ^ (z >> 30)).wrapping_mul(SPLITMIX_MIX_1);
    z = (z ^ (z >> 27)).wrapping_mul(SPLITMIX_MIX_2);
    z ^= z >> 31;
    // The seed is the output's low 32 bits reinterpreted as signed.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the seed is the low 32 bits of the `SplitMix64` output"
    )]
    let low = z as u32;
    low.cast_signed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_sequence_from_zero() {
        let mut rng = GameRng::new(0);
        let modulus = Modulus::from_difficulty(14);
        let mut states = Vec::new();
        let mut results = Vec::new();
        for _ in 0..3 {
            results.push(rng.draw(modulus));
            states.push(rng.state().cast_unsigned());
        }
        assert_eq!(states, [0x0000_0001, 0x015A_4E36, 0x8082_A52F]);
        assert_eq!(results, [1, 8, 1]);
        assert_eq!(rng.draws(), 3);
    }

    #[test]
    fn reseed_sets_the_state_only() {
        let mut rng = GameRng::new(0);
        let modulus = Modulus::from_difficulty(5);
        rng.draw(modulus);
        rng.reseed(-7);
        assert_eq!(rng.state(), -7);
        assert_eq!(rng.draws(), 1);
        // A negative state still yields a result in range.
        let result = rng.draw(modulus);
        assert!((0..5).contains(&result));
    }

    #[test]
    fn modulus_rejects_zero() {
        assert_eq!(Modulus::new(0), None);
        assert_eq!(Modulus::new(1).map(Modulus::get), Some(1));
        assert_eq!(Modulus::new(255).map(Modulus::get), Some(255));
    }

    #[test]
    fn round_seeds_base_0() {
        let seeds: Vec<u32> = (0..3).map(|r| round_seed(0, r).cast_unsigned()).collect();
        assert_eq!(seeds, [0x7B1D_CDAF, 0xA1B9_65F4, 0x8009_454F]);
    }

    #[test]
    fn round_seed_base_1() {
        assert_eq!(round_seed(1, 0).cast_unsigned(), 0x8902_5CC1);
    }

    #[test]
    fn round_seed_index_wraps_in_32_bits() {
        // Round u32::MAX uses index 0, the same step as base + 0.
        let mut z = 5u64;
        z = (z ^ (z >> 30)).wrapping_mul(SPLITMIX_MIX_1);
        z = (z ^ (z >> 27)).wrapping_mul(SPLITMIX_MIX_2);
        z ^= z >> 31;
        assert_eq!(
            round_seed(5, u32::MAX).cast_unsigned(),
            (z & 0xFFFF_FFFF) as u32
        );
    }
}
