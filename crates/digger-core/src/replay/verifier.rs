//! Checkpoint comparison and divergence reports (replay-verification spec,
//! "Checkpoint verification during eDRF playback" and "Divergence report";
//! design D6 of `add-recording-and-conformance`).

use core::fmt;

use crate::StateView;
use crate::recording::{EdrfRecording, EndRecord};
use crate::state_hash;

/// The tick checkpoint cadence: a `C` checkpoint is taken whenever the
/// tick count, after being incremented, is a multiple of this (quirk P12).
pub const CHECKPOINT_EVERY: u32 = 256;

/// The first checkpoint at which the game and its recording disagree
/// (replay-verification spec, "Divergence report").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Divergence {
    /// A tick checkpoint (`C`) differs.
    Tick {
        /// The tick count at which the checkpoint was taken.
        tick: u32,
        /// The recorded hash.
        expected: u32,
        /// The computed hash.
        actual: u32,
    },
    /// A round-end checkpoint (`E`) differs.
    RoundEnd {
        /// The tick count at which the round ended.
        tick: u32,
        /// The recorded hash.
        expected: u32,
        /// The computed hash.
        actual: u32,
    },
    /// The game ends at another tick or in another state than the end
    /// record (`Z`) says.
    End {
        /// The recorded tick total and hash.
        expected: EndRecord,
        /// The computed tick total and hash.
        actual: EndRecord,
    },
}

impl Divergence {
    /// The tick count at which the failing checkpoint was taken (for the
    /// end record, the game's tick total).
    pub const fn tick(&self) -> u32 {
        match self {
            Self::Tick { tick, .. } | Self::RoundEnd { tick, .. } => *tick,
            Self::End { actual, .. } => actual.ticks,
        }
    }
}

impl fmt::Display for Divergence {
    /// The spec's message, without a line end.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let diverges = |f: &mut fmt::Formatter<'_>, tick, what, actual, expected| {
            write!(
                f,
                "eDRF: the game diverges from the recording before tick {tick} \
                 ({what} state {actual:08X}, recorded {expected:08X})"
            )
        };
        match *self {
            Self::Tick {
                tick,
                expected,
                actual,
            } => diverges(f, tick, "tick", actual, expected),
            Self::RoundEnd {
                tick,
                expected,
                actual,
            } => diverges(f, tick, "end of round", actual, expected),
            Self::End { expected, actual } => write!(
                f,
                "eDRF: the game ends differently from the recording \
                 (expected {}/{:08X}, got {}/{:08X})",
                expected.ticks, expected.hash, actual.ticks, actual.hash
            ),
        }
    }
}

/// The recorded checkpoints of one kind and the next one to compare.
#[derive(Debug, Clone, Default)]
struct Queue {
    hashes: Vec<u32>,
    next: usize,
}

impl Queue {
    /// The next recorded hash, taken off the queue; `None` once the
    /// recording has no more of this kind.
    fn pop(&mut self) -> Option<u32> {
        let hash = self.hashes.get(self.next).copied()?;
        self.next = self.next.saturating_add(1);
        Some(hash)
    }
}

/// Compares the state hash with an eDRF recording's checkpoints. It only
/// observes: [`ReplayObserver`](super::ReplayObserver) tells it the state
/// the game is in and whether comparing still makes sense.
///
/// Each kind of checkpoint is compared with the next recorded one of the
/// same kind, in order. A point the game reaches with no checkpoint of
/// its kind left to compare is not a mismatch, and neither is a recorded
/// checkpoint the game never reaches. The first mismatch is kept and
/// ends the comparison.
#[derive(Debug, Clone)]
pub(crate) struct Verifier {
    ticks: Queue,
    round_ends: Queue,
    end: Option<EndRecord>,
    divergence: Option<Divergence>,
    last_match: Option<u32>,
    actual_end: Option<EndRecord>,
}

impl Verifier {
    /// A verifier for `recording`'s `C`, `E` and `Z` records.
    pub(crate) fn new(recording: &EdrfRecording) -> Self {
        Self::from_parts(
            recording.checkpoints().to_vec(),
            recording.round_ends().to_vec(),
            recording.end(),
        )
    }

    /// A verifier for these recorded checkpoints.
    fn from_parts(ticks: Vec<u32>, round_ends: Vec<u32>, end: Option<EndRecord>) -> Self {
        Self {
            ticks: Queue {
                hashes: ticks,
                next: 0,
            },
            round_ends: Queue {
                hashes: round_ends,
                next: 0,
            },
            end,
            divergence: None,
            last_match: None,
            actual_end: None,
        }
    }

    /// The first mismatch, if any.
    pub(crate) const fn divergence(&self) -> Option<&Divergence> {
        self.divergence.as_ref()
    }

    /// The tick count of the last checkpoint that matched.
    pub(crate) const fn last_match(&self) -> Option<u32> {
        self.last_match
    }

    /// Whether the recording has an end record.
    pub(crate) const fn has_end(&self) -> bool {
        self.end.is_some()
    }

    /// Compares one computed hash with the next recorded one of a kind.
    fn compare(
        &mut self,
        kind: fn(&mut Self) -> &mut Queue,
        tick: u32,
        actual: u32,
        divergence: fn(u32, u32, u32) -> Divergence,
    ) {
        if self.divergence.is_some() {
            return;
        }
        let Some(expected) = kind(self).pop() else {
            return;
        };
        if expected == actual {
            self.last_match = Some(tick);
        } else {
            self.divergence = Some(divergence(tick, expected, actual));
        }
    }

    /// Compares the hash computed after tick `tick`, a multiple of
    /// [`CHECKPOINT_EVERY`], with the next `C` record.
    fn check_tick(&mut self, tick: u32, actual: u32) {
        self.compare(
            |v| &mut v.ticks,
            tick,
            actual,
            |tick, expected, actual| Divergence::Tick {
                tick,
                expected,
                actual,
            },
        );
    }

    /// Compares the round-end hash of the round ending at `tick` with the
    /// next `E` record.
    fn check_round_end(&mut self, tick: u32, actual: u32) {
        self.compare(
            |v| &mut v.round_ends,
            tick,
            actual,
            |tick, expected, actual| Divergence::RoundEnd {
                tick,
                expected,
                actual,
            },
        );
    }

    /// Compares the game's end with the `Z` record, if there is one.
    fn check_end(&mut self, actual: EndRecord) {
        self.actual_end = Some(actual);
        if self.divergence.is_some() {
            return;
        }
        let Some(expected) = self.end else {
            return;
        };
        if expected == actual {
            self.last_match = Some(actual.ticks);
        } else {
            self.divergence = Some(Divergence::End { expected, actual });
        }
    }

    /// After tick `tick`: compares a `C` checkpoint when `tick` is a
    /// multiple of [`CHECKPOINT_EVERY`]. `active` is false once the
    /// playback has failed in another way, which ends the comparison.
    pub(crate) fn tick_end(&mut self, tick: u32, state: StateView<'_>, active: bool) {
        if active && tick.is_multiple_of(CHECKPOINT_EVERY) {
            self.check_tick(tick, state_hash::hash(state));
        }
    }

    /// At a round end, after its clean-up: compares the next `E` record.
    /// `active` is false once the playback has failed, and when the caller
    /// stopped the playback before the recording ran out (the recording
    /// has nothing to compare this cut-short round with).
    pub(crate) fn round_end(&mut self, state: StateView<'_>, active: bool) {
        if active {
            let tick = state.ticks();
            self.check_round_end(tick, state_hash::hash_at_round_end(state, tick));
        }
    }

    /// At the game's end: compares the `Z` record. `active` is false once
    /// the playback has failed, and when the game ended before the
    /// recording ran out (the playback is then stopped, not compared).
    pub(crate) fn game_end(&mut self, state: StateView<'_>, active: bool) {
        if active {
            self.check_end(EndRecord {
                ticks: state.ticks(),
                hash: state_hash::hash(state),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::{Recording, parse};

    fn end(ticks: u32, hash: u32) -> EndRecord {
        EndRecord { ticks, hash }
    }

    #[test]
    fn checkpoint_mismatch_reports_tick_and_hashes() {
        let mut v = Verifier::from_parts(vec![0x1111_1111, 0x9988_7766], vec![], None);
        v.check_tick(256, 0x1111_1111);
        assert_eq!(v.divergence(), None);
        v.check_tick(512, 0x0A1B_2C3D);
        let divergence = Divergence::Tick {
            tick: 512,
            expected: 0x9988_7766,
            actual: 0x0A1B_2C3D,
        };
        assert_eq!(v.divergence(), Some(&divergence));
        assert_eq!(v.last_match(), Some(256));
        assert_eq!(
            divergence.to_string(),
            "eDRF: the game diverges from the recording before tick 512 \
             (tick state 0A1B2C3D, recorded 99887766)"
        );

        let mut v = Verifier::from_parts(vec![], vec![0x0000_0001], None);
        v.check_round_end(1378, 0x00AB_CDEF);
        let divergence = v.divergence().copied();
        assert_eq!(divergence.map(|d| d.tick()), Some(1378));
        assert_eq!(
            divergence.map(|d| d.to_string()).as_deref(),
            Some(
                "eDRF: the game diverges from the recording before tick 1378 \
                 (end of round state 00ABCDEF, recorded 00000001)"
            )
        );
        assert_eq!(v.last_match(), None);
    }

    #[test]
    fn only_first_failure_reported() {
        let mut v = Verifier::from_parts(vec![1, 2, 3, 4], vec![5, 6], Some(end(1300, 7)));
        v.check_tick(256, 1);
        v.check_tick(512, 2);
        v.check_tick(768, 99);
        v.check_tick(1024, 98);
        v.check_round_end(1100, 97);
        v.check_tick(1280, 96);
        v.check_end(end(1301, 95));
        assert_eq!(
            v.divergence(),
            Some(&Divergence::Tick {
                tick: 768,
                expected: 3,
                actual: 99,
            })
        );
        assert_eq!(v.last_match(), Some(512));
    }

    #[test]
    fn end_record_tick_mismatch() {
        // Another tick total with the same hash, then the same total with
        // another hash: both fail.
        for actual in [end(4490, 0x1122_3344), end(4489, 0x1122_3345)] {
            let mut v = Verifier::from_parts(vec![], vec![], Some(end(4489, 0x1122_3344)));
            v.check_end(actual);
            let divergence = v.divergence().copied();
            assert_eq!(
                divergence,
                Some(Divergence::End {
                    expected: end(4489, 0x1122_3344),
                    actual,
                })
            );
            assert_eq!(divergence.map(|d| d.tick()), Some(actual.ticks));
        }
        let mut v = Verifier::from_parts(vec![], vec![], Some(end(4489, 0x1122_3344)));
        v.check_end(end(4490, 0x1122_3344));
        assert_eq!(
            v.divergence().map(ToString::to_string).as_deref(),
            Some(
                "eDRF: the game ends differently from the recording \
                 (expected 4489/11223344, got 4490/11223344)"
            )
        );
        // A matching end record is the last matching checkpoint.
        let mut v = Verifier::from_parts(vec![], vec![], Some(end(4489, 0x1122_3344)));
        v.check_end(end(4489, 0x1122_3344));
        assert_eq!(v.divergence(), None);
        assert_eq!(v.last_match(), Some(4489));
    }

    #[test]
    fn truncated_checked_up_to_cut() {
        // An eDRF cut after its checkpoint for tick 512: ticks 256 and 512
        // are compared; later points have nothing to compare with.
        let text = "eDRF 2\nRS DIG 20261111\n1\n20000\nR 00000000\n\
                    I @512\nC 00000100\nC 00000200\n";
        let Ok(Recording::Edrf(recording)) = parse(text.as_bytes()) else {
            unreachable!("a valid eDRF")
        };
        let mut v = Verifier::new(&recording);
        assert!(!v.has_end());
        v.check_tick(256, 0x100);
        v.check_tick(512, 0x200);
        v.check_tick(768, 0xDEAD);
        v.check_round_end(800, 0xBEEF);
        v.check_end(end(800, 0xF00D));
        assert_eq!(v.divergence(), None);
        assert_eq!(v.last_match(), Some(512));

        // A mismatch before the cut is still found.
        let mut v = Verifier::new(&recording);
        v.check_tick(256, 0x100);
        v.check_tick(512, 0x201);
        assert_eq!(v.divergence().map(Divergence::tick), Some(512));
    }
}
