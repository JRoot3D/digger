//! A test-local input generator for the long-running integration tests
//! (design D11: determinism, observer round trip, soak).
//!
//! It plays like an impatient person: each slot holds one direction (or
//! none) for a few frames to a few dozen, sometimes two at once, and taps
//! fire now and then. The sequence depends only on the seed, so every run
//! with the same seed sees the same inputs.

use digger_core::{Controls, FrameInput};

/// A small xorshift generator, independent of the game's own generator.
#[derive(Debug, Clone)]
pub(crate) struct InputGen {
    state: u64,
    slots: [Held; 2],
}

/// What one slot holds and for how many more frames.
#[derive(Debug, Clone, Copy, Default)]
struct Held {
    controls: Controls,
    left: u32,
}

impl InputGen {
    /// A generator seeded with `seed`.
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            // Never zero: xorshift stays at zero forever.
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
            slots: [Held::default(); 2],
        }
    }

    /// The next 64 random bits.
    pub(crate) fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// A number in `0..n`, for `n` > 0.
    pub(crate) fn below(&mut self, n: u32) -> u32 {
        u32::try_from(self.next_u64() % u64::from(n)).unwrap_or_default()
    }

    /// The controls of the next frame, without pause or quit requests.
    pub(crate) fn controls(&mut self) -> FrameInput {
        let mut input = FrameInput::NONE;
        let mut slots = self.slots;
        for (out, held) in input.slots.iter_mut().zip(&mut slots) {
            if held.left == 0 {
                *held = self.fresh_hold();
            }
            held.left -= 1;
            let mut controls = held.controls;
            // Fire is tapped, not held: about one frame in twelve.
            controls.fire = self.below(12) == 0;
            *out = controls;
        }
        self.slots = slots;
        input
    }

    /// A new hold: one direction most of the time, sometimes nothing,
    /// sometimes two directions at once.
    fn fresh_hold(&mut self) -> Held {
        let mut controls = Controls::NONE;
        let pick = self.below(10);
        let mut press = |which: u32| match which {
            0 => controls.up = true,
            1 => controls.down = true,
            2 => controls.left = true,
            _ => controls.right = true,
        };
        match pick {
            0 => {}
            1 => {
                press(self.below(4));
                press(self.below(4));
            }
            _ => press(self.below(4)),
        }
        Held {
            controls,
            left: 1 + self.below(40),
        }
    }
}
