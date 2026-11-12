//! Property tests of the square-wave generator (task 7.3; design D6, D12;
//! spec "Square-wave tone shape").
//!
//! For any supported sample rate and any whole-Hz frequency below half the
//! rate, the oscillator must give exactly `2 * hz` sign changes in any whole
//! second, half periods within one sample of `rate / (2 * hz)`, only the two
//! peak values, and no extra transition when the frequency changes.

use digger_sound::{Hertz, SampleRate, SquareOsc, Tone};
use proptest::prelude::*;

/// A supported rate and a frequency with `2 * hz < rate`.
fn rate_and_hz() -> impl Strategy<Value = (u32, u32)> {
    (SampleRate::MIN_HZ..=SampleRate::MAX_HZ).prop_flat_map(|rate| (Just(rate), 1..=(rate - 1) / 2))
}

/// The oscillator for `rate`, which the strategy keeps in range.
fn osc(rate: u32) -> SquareOsc {
    SquareOsc::new(SampleRate::new(rate).unwrap_or(SampleRate::DEFAULT))
}

/// `n` samples of `tone`.
fn samples(osc: &mut SquareOsc, tone: Tone, n: u32) -> Vec<i16> {
    (0..n).map(|_| osc.next_sample(tone)).collect()
}

/// The number of sign changes between consecutive samples.
fn sign_changes(samples: &[i16]) -> usize {
    samples
        .windows(2)
        .filter(|w| matches!(w, [a, b] if (*a > 0) != (*b > 0)))
        .count()
}

/// The lengths of the runs of equal samples, without the first and the
/// last run (which may be cut off by the window).
fn inner_runs(samples: &[i16]) -> Vec<u32> {
    let mut runs = Vec::new();
    let mut length = 0_u32;
    let mut previous = None;
    for &sample in samples {
        if previous.is_some_and(|p| p != sample) {
            runs.push(length);
            length = 0;
        }
        length += 1;
        previous = Some(sample);
    }
    if !runs.is_empty() {
        runs.remove(0);
    }
    runs
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn one_second_has_exactly_two_transitions_per_hz(
        (rate, hz) in rate_and_hz(),
        warm_up in 0_u32..50_000,
        peak in 1_i16..=16_383,
    ) {
        let tone = Tone { hz: Hertz(hz), peak };
        let mut osc = osc(rate);
        samples(&mut osc, tone, warm_up);
        // rate + 1 samples hold `rate` consecutive pairs: one whole second.
        let second = samples(&mut osc, tone, rate + 1);
        prop_assert_eq!(sign_changes(&second), 2 * hz as usize);
    }

    #[test]
    fn half_periods_are_within_one_sample(
        (rate, hz) in rate_and_hz(),
        peak in 1_i16..=16_383,
    ) {
        let tone = Tone { hz: Hertz(hz), peak };
        let mut osc = osc(rate);
        let second = samples(&mut osc, tone, rate);
        // |length - rate / (2 hz)| < 1, in integers.
        for length in inner_runs(&second) {
            let error = (i64::from(length) * 2 * i64::from(hz) - i64::from(rate)).abs();
            prop_assert!(error < 2 * i64::from(hz), "run of {} samples", length);
        }
    }

    #[test]
    fn samples_are_only_plus_or_minus_peak(
        (rate, hz) in rate_and_hz(),
        peak in 1_i16..=16_383,
        n in 1_u32..20_000,
    ) {
        let tone = Tone { hz: Hertz(hz), peak };
        let mut osc = osc(rate);
        for sample in samples(&mut osc, tone, n) {
            prop_assert!(sample == peak || sample == -peak, "{}", sample);
        }
    }

    #[test]
    fn frequency_change_adds_no_transition(
        (rate, hz) in rate_and_hz(),
        other in 1_u32..4_000,
        before in 1_u32..20_000,
    ) {
        let other = other.min((rate - 1) / 2);
        let (a, b) = (
            Tone { hz: Hertz(hz), peak: 16_383 },
            Tone { hz: Hertz(other), peak: 16_383 },
        );
        let mut changed = osc(rate);
        let mut continued = osc(rate);
        let pre = samples(&mut changed, a, before);
        samples(&mut continued, a, before);
        // The first sample at the new frequency is the one the old
        // frequency would have produced next, so the change itself adds
        // no transition, and the phase is unchanged by it.
        prop_assert_eq!(changed.phase(), continued.phase());
        let post = samples(&mut changed, b, rate);
        prop_assert_eq!(post.first().copied(), Some(continued.next_sample(a)));
        // The half period in progress is completed at the new frequency:
        // the run spanning the change is no longer than a half period of
        // the slower tone, rounded up.
        let last = pre.last().copied();
        let tail = pre.iter().rev().take_while(|&&s| Some(s) == last).count();
        let head = post.iter().take_while(|&&s| Some(s) == last).count();
        let slower = hz.min(other);
        let limit = rate.div_ceil(2 * slower) as usize + 1;
        prop_assert!(tail + head <= limit, "run {} + {} > {}", tail, head, limit);
    }
}
