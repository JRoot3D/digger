//! The optional output filter: a first-order high-pass at 1000 Hz followed
//! by a first-order low-pass at 4000 Hz (design D7; research 04 §4.2).
//!
//! It softens the naive square wave the way the original's `CMake` build
//! does. With `w = tan(pi * fc / fs)` and `n = 1 / (1 + w)`, each stage
//! computes `y = b0 * x + b1 * x' - a1 * y'` (`x'` and `y'` the previous
//! input and output) with `a1 = n * (w - 1)`, `b0 = n` and `b1 = -n` for the
//! high-pass and `b0 = b1 = n * w` for the low-pass. The result is rounded
//! to the nearest integer and limited to the 16-bit range.
//!
//! The arithmetic is `f64` and the coefficients use `f64::tan`, so filtered
//! output may differ in the last bit between platforms; no golden hash is
//! taken of it (design D12).

use crate::units::SampleRate;

/// The high-pass cut-off in Hz.
pub const HIGH_PASS_HZ: f64 = 1000.0;
/// The low-pass cut-off in Hz.
pub const LOW_PASS_HZ: f64 = 4000.0;

/// One first-order filter stage and its state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FirstOrder {
    b0: f64,
    b1: f64,
    a1: f64,
    /// The previous input.
    x1: f64,
    /// The previous output.
    y1: f64,
}

/// `tan(pi * cutoff / rate)`, the prewarped cut-off of both stages.
fn prewarp(cutoff: f64, rate: SampleRate) -> f64 {
    (core::f64::consts::PI * cutoff / f64::from(rate.hz())).tan()
}

impl FirstOrder {
    /// A high-pass stage with cut-off `cutoff` Hz at `rate`.
    pub fn high_pass(cutoff: f64, rate: SampleRate) -> Self {
        let w = prewarp(cutoff, rate);
        let n = 1.0 / (1.0 + w);
        Self::with(n, -n, n * (w - 1.0))
    }

    /// A low-pass stage with cut-off `cutoff` Hz at `rate`.
    pub fn low_pass(cutoff: f64, rate: SampleRate) -> Self {
        let w = prewarp(cutoff, rate);
        let n = 1.0 / (1.0 + w);
        Self::with(n * w, n * w, n * (w - 1.0))
    }

    const fn with(b0: f64, b1: f64, a1: f64) -> Self {
        Self {
            b0,
            b1,
            a1,
            x1: 0.0,
            y1: 0.0,
        }
    }

    /// The coefficients `(b0, b1, a1)`.
    pub const fn coefficients(&self) -> (f64, f64, f64) {
        (self.b0, self.b1, self.a1)
    }

    /// Forgets the previous input and output.
    pub const fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }

    /// Filters one sample.
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 - self.a1 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// The output filter: high-pass 1000 Hz, then low-pass 4000 Hz, rounded
/// and limited to `i16`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutputFilter {
    high_pass: FirstOrder,
    low_pass: FirstOrder,
}

impl OutputFilter {
    /// The filter for `rate`, with its coefficients computed once.
    pub fn new(rate: SampleRate) -> Self {
        Self {
            high_pass: FirstOrder::high_pass(HIGH_PASS_HZ, rate),
            low_pass: FirstOrder::low_pass(LOW_PASS_HZ, rate),
        }
    }

    /// The high-pass stage.
    pub const fn high_pass(&self) -> &FirstOrder {
        &self.high_pass
    }

    /// The low-pass stage.
    pub const fn low_pass(&self) -> &FirstOrder {
        &self.low_pass
    }

    /// Forgets both stages' state, as when the filter is switched on.
    pub const fn reset(&mut self) {
        self.high_pass.reset();
        self.low_pass.reset();
    }

    /// Filters one sample, before rounding and limiting.
    pub fn process_unrounded(&mut self, sample: i16) -> f64 {
        self.low_pass
            .process(self.high_pass.process(f64::from(sample)))
    }

    /// Filters one sample: rounded to the nearest integer (halves away from
    /// zero) and limited to -32768..=32767.
    pub fn process(&mut self, sample: i16) -> i16 {
        let y = self
            .process_unrounded(sample)
            .round()
            .clamp(f64::from(i16::MIN), f64::from(i16::MAX));
        // Clamped to the i16 range above; a NaN cannot arise from finite
        // coefficients and inputs, and would convert to 0.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the value is clamped to the i16 range"
        )]
        let y = y as i16;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coefficients_at_44100() {
        let rate = SampleRate::DEFAULT;
        // The published 10-digit values to 1e-9 (task 7.4); the closed form
        // of the spec to 1e-12 (design D12).
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        let exact = |a: f64, b: f64| (a - b).abs() < 1e-12;

        let w_high = prewarp(HIGH_PASS_HZ, rate);
        let w_low = prewarp(LOW_PASS_HZ, rate);
        assert!(close(w_high, 0.071_358_680_9), "{w_high}");
        assert!(close(w_low, 0.292_923_173_7), "{w_low}");

        let filter = OutputFilter::new(rate);
        let (b0, b1, a1) = filter.high_pass().coefficients();
        assert!(close(a1, -0.866_788_439_5), "{a1}");
        let n = 1.0 / (1.0 + w_high);
        assert!(exact(a1, n * (w_high - 1.0)));
        assert!(exact(b0, n));
        assert!(exact(b1, -n));

        let (b0, b1, a1) = filter.low_pass().coefficients();
        assert!(close(a1, -0.546_882_321_1), "{a1}");
        let n = 1.0 / (1.0 + w_low);
        assert!(exact(a1, n * (w_low - 1.0)));
        assert!(exact(b0, n * w_low));
        assert!(exact(b1, n * w_low));
    }

    #[test]
    fn constant_input_decays_to_zero() {
        let mut filter = OutputFilter::new(SampleRate::DEFAULT);
        let out: Vec<i16> = (0..44_100).map(|_| filter.process(16_383)).collect();
        assert_ne!(out.first(), Some(&0));
        assert!(out.iter().skip(1_000).all(|&y| y == 0));
        // The same for a negative value.
        filter.reset();
        let last = (0..44_100).map(|_| filter.process(-16_383)).last();
        assert_eq!(last, Some(0));
    }

    #[test]
    fn two_khz_sine_within_3_db() {
        let rate = SampleRate::DEFAULT;
        let mut filter = OutputFilter::new(rate);
        let amplitude = 10_000.0;
        let mut peak: f64 = 0.0;
        for i in 0..44_100_u32 {
            let t = f64::from(i) / f64::from(rate.hz());
            let x = (amplitude * (2.0 * core::f64::consts::PI * 2000.0 * t).sin()).round();
            #[expect(clippy::cast_possible_truncation, reason = "|x| <= 10000 fits an i16")]
            let y = filter.process(x as i16);
            // Measure after the filter has settled.
            if i >= 4_410 {
                peak = peak.max(f64::from(y).abs());
            }
        }
        let gain_db = 20.0 * (peak / amplitude).log10();
        assert!(gain_db.abs() < 3.0, "gain {gain_db} dB");
    }

    #[test]
    fn full_amplitude_79_hz_never_overflows() {
        // Divisor 15000 (the bag break) is 79 Hz: the lowest effect tone
        // and the largest steps the high-pass sees per period.
        let rate = SampleRate::DEFAULT;
        let mut filter = OutputFilter::new(rate);
        let mut osc = crate::osc::SquareOsc::new(rate);
        let tone = crate::voice::Tone {
            hz: crate::units::Hertz(79),
            peak: crate::voice::FULL_PEAK,
        };
        for _ in 0..44_100 {
            let y = filter.process_unrounded(osc.next_sample(tone)).round();
            assert!(
                (f64::from(i16::MIN)..=f64::from(i16::MAX)).contains(&y),
                "{y}"
            );
        }
        // And at the lowest and highest supported rates.
        for hz in [SampleRate::MIN_HZ, SampleRate::MAX_HZ] {
            let Ok(rate) = SampleRate::new(hz) else {
                continue;
            };
            let mut filter = OutputFilter::new(rate);
            let mut osc = crate::osc::SquareOsc::new(rate);
            for _ in 0..hz {
                let y = filter.process_unrounded(osc.next_sample(tone)).round();
                assert!(y.abs() <= f64::from(i16::MAX), "{hz} Hz: {y}");
            }
        }
    }
}
