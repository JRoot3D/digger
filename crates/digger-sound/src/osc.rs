//! The square-wave generator: an exact integer phase accumulator (design
//! D6; research 04 §4.1).
//!
//! A [`SquareOsc`] keeps its phase as an integer `acc` in `0..rate`, so
//! `acc / rate` is the exact fraction of a period already played. For a
//! tone of `hz` Hz each sample is high while `2 * acc < rate` and then
//! `acc` advances by `hz` modulo `rate`. Frequencies are whole Hz and the
//! rate is an integer, so the wave is exactly periodic over one second,
//! never drifts and uses no floating point: unfiltered output is the same
//! on every platform.
//!
//! The phase is kept when the frequency changes, so a pitch change adds no
//! transition, and it is reset to 0 when the oscillator falls silent.
//!
//! The same accumulator gates the level-done jingle's carrier on and off:
//! the gate is on for the first half of each beat period. A beat of 0 Hz
//! never advances the accumulator, so the gate stays on (fixed quirk SF9).

use crate::units::{Hertz, SampleRate};
use crate::voice::Tone;

/// A 50 % duty square-wave oscillator with an integer phase accumulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SquareOsc {
    /// The phase in `1 / rate` periods; always `< rate`.
    acc: u32,
    /// The sample rate in Hz.
    rate: u32,
}

impl SquareOsc {
    /// An oscillator for `rate`, at phase 0.
    pub const fn new(rate: SampleRate) -> Self {
        Self {
            acc: 0,
            rate: rate.hz(),
        }
    }

    /// The current phase, in `1 / rate` periods (`0..rate`).
    pub const fn phase(&self) -> u32 {
        self.acc
    }

    /// Returns the oscillator to phase 0, as when its tone falls silent.
    pub const fn reset(&mut self) {
        self.acc = 0;
    }

    /// Whether the current sample is in the high half of the period, then
    /// advances by one sample of a tone of `hz` Hz.
    ///
    /// `hz` is expected to be below half the sample rate (the voice stage
    /// guarantees it for tones); any value is accepted and taken modulo
    /// the rate, so this never panics.
    pub fn next_high(&mut self, hz: Hertz) -> bool {
        let high = u64::from(self.acc) * 2 < u64::from(self.rate);
        // Both terms are below the rate, so the sum cannot overflow and
        // one subtraction brings it back into 0..rate.
        let step = hz.get() % self.rate;
        let next = self.acc + step;
        self.acc = if next >= self.rate {
            next - self.rate
        } else {
            next
        };
        high
    }

    /// The next sample of `tone`: `+peak` in the high half of the period,
    /// `-peak` in the low half.
    pub fn next_sample(&mut self, tone: Tone) -> i16 {
        if self.next_high(tone.hz) {
            tone.peak
        } else {
            -tone.peak
        }
    }
}

/// The on/off gate of the level-done jingle: on for the first half of each
/// period of the beat frequency, off for the second half (research 04 §2).
///
/// A beat of 0 Hz means the carrier is never gated off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Gate {
    osc: SquareOsc,
}

impl Gate {
    /// A gate for `rate`, at the start of an "on" half.
    pub const fn new(rate: SampleRate) -> Self {
        Self {
            osc: SquareOsc::new(rate),
        }
    }

    /// Returns the gate to the start of an "on" half.
    pub const fn reset(&mut self) {
        self.osc.reset();
    }

    /// Whether the gate is on for the current sample, then advances by one
    /// sample of a beat of `beat` Hz.
    pub fn next_on(&mut self, beat: Hertz) -> bool {
        if beat.get() == 0 {
            return true;
        }
        self.osc.next_high(beat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(hz: u32) -> SampleRate {
        SampleRate::new(hz).unwrap_or(SampleRate::DEFAULT)
    }

    /// Counts sign changes between consecutive samples.
    fn transitions(samples: &[i16]) -> usize {
        samples
            .windows(2)
            .filter(|w| w.first().map(|&a| a > 0) != w.get(1).map(|&b| b > 0))
            .count()
    }

    #[test]
    fn one_second_of_523_hz_has_1046_transitions() {
        let tone = Tone {
            hz: Hertz(523),
            peak: 16_383,
        };
        let mut osc = SquareOsc::new(SampleRate::DEFAULT);
        // 44100 consecutive pairs of samples: one whole second.
        let samples: Vec<i16> = (0..=44_100).map(|_| osc.next_sample(tone)).collect();
        assert_eq!(transitions(&samples), 1046);
        assert!(samples.iter().all(|&s| s == 16_383 || s == -16_383));
        // The phase is back where it started after exactly one second.
        let mut osc = SquareOsc::new(SampleRate::DEFAULT);
        for _ in 0..44_100 {
            osc.next_high(tone.hz);
        }
        assert_eq!(osc.phase(), 0);
    }

    #[test]
    fn phase_kept_across_frequency_change() {
        let rate = SampleRate::DEFAULT;
        // 1193181 / 1313 = 908 Hz, then 1193181 / 1149 = 1038 Hz (the
        // explosion's first two divisors).
        let (a, b) = (Hertz(908), Hertz(1038));
        let mut osc = SquareOsc::new(rate);
        let mut reference = SquareOsc::new(rate);
        for _ in 0..606 {
            assert_eq!(osc.next_high(a), reference.next_high(a));
        }
        let phase = osc.phase();
        assert_eq!(phase, 606 * 908 % 44_100);
        // The first sample at the new frequency is the one the old
        // frequency would have produced: the change adds no transition.
        let continued = reference.next_high(a);
        assert_eq!(osc.next_high(b), continued);
        assert_eq!(osc.phase(), (phase + 1038) % 44_100);
        // Resetting returns to phase 0, the start of a high half.
        osc.reset();
        assert_eq!(osc.phase(), 0);
        assert!(osc.next_high(b));
    }

    #[test]
    fn gate_on_for_first_half_of_beat() {
        // 8 Hz at 8000 Hz: 1000 samples per beat, on for the first 500.
        let mut gate = Gate::new(rate(8_000));
        let on: Vec<bool> = (0..2_000).map(|_| gate.next_on(Hertz(8))).collect();
        for (i, &state) in on.iter().enumerate() {
            assert_eq!(state, i % 1_000 < 500, "sample {i}");
        }
        // At 44100 Hz a 31 Hz beat is on for 712 or 711 samples per half.
        let mut gate = Gate::new(SampleRate::DEFAULT);
        let first_off = (0..44_100).position(|_| !gate.next_on(Hertz(31)));
        assert_eq!(first_off, Some(712));
        // Reset starts an "on" half again.
        gate.reset();
        assert!(gate.next_on(Hertz(31)));
    }

    #[test]
    fn zero_beat_never_gates_off() {
        let mut gate = Gate::new(SampleRate::DEFAULT);
        assert!((0..44_100).all(|_| gate.next_on(Hertz(0))));
    }
}
