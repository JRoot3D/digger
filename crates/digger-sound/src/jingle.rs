//! The level-done jingle (design D4; research 04 §2).
//!
//! Eleven notes of 21 ticks each. Each note is a full-amplitude carrier at
//! `1193181 / (D + 35)` Hz switched on and off at the beat frequency
//! `1193181 / D - 1193181 / (D + 35)` Hz, the original's two-oscillator
//! "polyphonic" sound. While the jingle plays, effects and music are
//! frozen; that rule is applied by the synth, which skips them.

use crate::units::{Divisor, Hertz};
use crate::voice::Voice;

/// The note divisors of the jingle (research 04 §2).
pub const JINGLE_DIVISORS: [Divisor; 11] = [
    Divisor(2280),
    Divisor(1810),
    Divisor(1522),
    Divisor(2032),
    Divisor(1708),
    Divisor(1356),
    Divisor(1810),
    Divisor(1522),
    Divisor(1208),
    Divisor(1140),
    Divisor(1140),
];

/// The ticks each note lasts.
pub const NOTE_TICKS: u8 = 21;

/// How far the carrier's divisor is above the note's.
const CARRIER_OFFSET: u16 = 35;

/// The carrier divisor of a jingle note with divisor `note`: `note + 35`.
pub const fn carrier(note: Divisor) -> Divisor {
    Divisor(note.0.saturating_add(CARRIER_OFFSET))
}

/// The beat frequency of a jingle note with divisor `note`: the difference
/// between its own frequency and its carrier's.
///
/// It is 0 Hz when the two frequencies are equal (never for the jingle's
/// own notes), which the oscillator treats as "never gated off", so no
/// division by a zero frequency can occur (fixed quirk SF9).
pub const fn beat(note: Divisor) -> Hertz {
    Hertz(note.hertz().0.saturating_sub(carrier(note).hertz().0))
}

/// The voice of a jingle note with divisor `note`.
pub const fn note_voice(note: Divisor) -> Voice {
    Voice::Jingle {
        carrier: carrier(note),
        beat: beat(note),
    }
}

/// What the jingle did on one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JingleStep {
    /// The voice sounding on this tick.
    pub voice: Voice,
    /// Whether this was the jingle's last tick.
    pub done: bool,
}

/// A level-done jingle in progress.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct LevelDone {
    /// The note playing, 0..=10.
    note: u8,
    /// Ticks of the current note played so far.
    ticks: u8,
}

impl LevelDone {
    /// The jingle at its first tick.
    pub const fn new() -> Self {
        Self { note: 0, ticks: 0 }
    }

    /// Advances the jingle by one tick.
    pub fn tick(&mut self) -> JingleStep {
        let index = usize::from(self.note);
        let note = JINGLE_DIVISORS.get(index).copied().unwrap_or(Divisor(1140));
        self.ticks += 1;
        if self.ticks == NOTE_TICKS {
            self.ticks = 0;
            self.note += 1;
        }
        JingleStep {
            voice: note_voice(note),
            done: usize::from(self.note) >= JINGLE_DIVISORS.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_beat_is_ungated() {
        // 1193181 / 16000 = 74 Hz and 1193181 / 16035 = 74 Hz: no beat.
        assert_eq!(beat(Divisor(16_000)), Hertz(0));
        let voice = note_voice(Divisor(16_000));
        assert_eq!(
            voice,
            Voice::Jingle {
                carrier: Divisor(16_035),
                beat: Hertz(0)
            }
        );
        // The carrier still sounds; nothing divides by the zero beat.
        assert_eq!(
            voice.tone(crate::SampleRate::DEFAULT).map(|t| t.hz),
            Some(Hertz(74))
        );
        // No note of the jingle itself has a zero beat.
        assert!(JINGLE_DIVISORS.iter().all(|&d| beat(d).get() > 0));
    }

    #[test]
    fn eleven_notes_of_21_ticks() {
        let mut jingle = LevelDone::new();
        let steps: Vec<JingleStep> = (0..231).map(|_| jingle.tick()).collect();
        assert!(steps.iter().take(230).all(|s| !s.done));
        assert!(steps.last().is_some_and(|s| s.done));
        for (n, chunk) in steps.chunks(21).enumerate() {
            let expected = JINGLE_DIVISORS.get(n).copied().map(note_voice);
            assert!(chunk.iter().all(|s| Some(s.voice) == expected), "note {n}");
        }
    }
}
