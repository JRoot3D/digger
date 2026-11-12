//! What sounds on one tick (design D5).
//!
//! Every tick the engine reduces its effects, music and jingle to a single
//! [`Voice`]. Whether that voice is audible, at which frequency and at which
//! peak amplitude is decided in one place, [`Voice::tone`], so the rules
//! that shape the original's sound (the divisor gate, the music pitch
//! clamp, "level 1 is silent") are isolated there.

use crate::units::{Divisor, Hertz, Level, SampleRate};

/// The lowest divisor that produces a tone (research 04 §1.2, §2).
pub const MIN_AUDIBLE_DIVISOR: Divisor = Divisor(41);
/// The highest divisor that produces a tone; 0x4000 and above are silent
/// (kept quirk SQ1; research 04 §1.2).
pub const MAX_AUDIBLE_DIVISOR: Divisor = Divisor(0x3FFF);
/// The lowest divisor music plays at: lower divisors are raised to this,
/// so music never goes above 1193 Hz (research 04 §1.3).
pub const MIN_MUSIC_DIVISOR: Divisor = Divisor(1000);
/// The peak sample value of a full-amplitude tone (effects, the jingle and
/// music at level 50; research 04 §4.1).
pub const FULL_PEAK: i16 = 16_383;

/// The single sound source chosen for one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Voice {
    /// Nothing sounds.
    Silent,
    /// An effect tone at full amplitude; silent when its divisor is outside
    /// 41..=16383.
    Effect(Divisor),
    /// A music tone at a level of the envelope.
    Music {
        /// The note's divisor, before the clamp to 1000.
        divisor: Divisor,
        /// The envelope level, 1 (silent) to 50 (full amplitude).
        level: Level,
    },
    /// The level-done jingle: a full-amplitude carrier switched on and off
    /// at a beat frequency.
    Jingle {
        /// The carrier's divisor.
        carrier: Divisor,
        /// The beat frequency; 0 Hz means the carrier is never gated off.
        beat: Hertz,
    },
}

/// An audible square-wave tone: its frequency and peak sample value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tone {
    /// The tone's frequency.
    pub hz: Hertz,
    /// The peak sample value; the wave swings between `+peak` and `-peak`.
    pub peak: i16,
}

/// Whether `divisor` produces a tone at all: greater than 40 and less than
/// 16384 (research 04 §1.2, §2).
pub const fn is_audible(divisor: Divisor) -> bool {
    divisor.0 >= MIN_AUDIBLE_DIVISOR.0 && divisor.0 <= MAX_AUDIBLE_DIVISOR.0
}

/// The peak sample value of a music tone at `level`:
/// `((level - 1) * 32767 / 49) / 2` in integers, so level 50 gives 16383
/// and level 1 gives 0 (research 04 §3.3, §4.1).
pub const fn music_peak(level: Level) -> i16 {
    let scaled = (level.get() as u32 - 1) * 32_767 / 49 / 2;
    // At most 49 * 32767 / 49 / 2 = 16383, which fits an i16.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "scaled <= 16383 because level <= 50"
    )]
    let peak = scaled as i16;
    peak
}

impl Voice {
    /// The tone this voice produces at `rate`, or `None` when it is silent.
    ///
    /// A voice is silent when:
    /// - it is [`Voice::Silent`];
    /// - its divisor (after the music clamp) is outside 41..=16383;
    /// - it is music at level 1;
    /// - its frequency is at or above half the sample rate (a guard that
    ///   the tones of the game never reach at supported rates).
    ///
    /// Music divisors below 1000 are raised to 1000 first. The jingle's
    /// carrier is not clamped; its beat is applied by the oscillator, not
    /// here.
    pub fn tone(self, rate: SampleRate) -> Option<Tone> {
        let (divisor, peak) = match self {
            Self::Silent => return None,
            Self::Effect(divisor)
            | Self::Jingle {
                carrier: divisor, ..
            } => (divisor, FULL_PEAK),
            Self::Music { divisor, level } => (divisor.max(MIN_MUSIC_DIVISOR), music_peak(level)),
        };
        if !is_audible(divisor) || peak == 0 {
            return None;
        }
        let hz = divisor.hertz();
        if u64::from(hz.get()) * 2 >= u64::from(rate.hz()) {
            return None;
        }
        Some(Tone { hz, peak })
    }
}

/// What one tick produced: the voice and the state behind it, for tests
/// and tools that check timing without looking at samples (design D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TickTrace {
    /// The voice chosen for this tick.
    pub voice: Voice,
    /// The divisor the highest-priority effect offered, if any effect
    /// offered one (even a silent one).
    pub effect_offer: Option<Divisor>,
    /// The music envelope level after this tick.
    pub music_level: Level,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(n: u8) -> Level {
        Level::saturating(n)
    }

    #[test]
    fn divisor_gate_edges() {
        assert!(!is_audible(Divisor(40)));
        assert!(is_audible(Divisor(41)));
        assert!(is_audible(Divisor(16383)));
        assert!(!is_audible(Divisor(16384)));
        assert!(!is_audible(Divisor(0)));
        assert!(!is_audible(Divisor(0x7D00)));

        // Divisor 41 is 29101 Hz, so it needs a rate above 58202 Hz to pass
        // the Nyquist guard as well.
        let high = SampleRate::new(192_000).ok();
        let tone = |d| high.and_then(|rate| Voice::Effect(Divisor(d)).tone(rate));
        assert_eq!(tone(40), None);
        assert_eq!(
            tone(41),
            Some(Tone {
                hz: Hertz(29_101),
                peak: FULL_PEAK
            })
        );
        assert_eq!(
            tone(16383),
            Some(Tone {
                hz: Hertz(72),
                peak: FULL_PEAK
            })
        );
        assert_eq!(tone(16384), None);
    }

    #[test]
    fn nyquist_guard() {
        let rate = SampleRate::DEFAULT;
        // 1193181 / 54 = 22096 Hz >= 22050 Hz: silenced.
        assert_eq!(Voice::Effect(Divisor(54)).tone(rate), None);
        // 1193181 / 55 = 21694 Hz < 22050 Hz: sounds.
        assert_eq!(
            Voice::Effect(Divisor(55)).tone(rate).map(|t| t.hz),
            Some(Hertz(21_694))
        );
    }

    #[test]
    fn music_divisor_clamped_to_1000() {
        let rate = SampleRate::DEFAULT;
        let hz = |d| {
            Voice::Music {
                divisor: Divisor(d),
                level: Level::MAX,
            }
            .tone(rate)
            .map(|t| t.hz.get())
        };
        assert_eq!(hz(41), Some(1193));
        assert_eq!(hz(500), Some(1193));
        assert_eq!(hz(999), Some(1193));
        assert_eq!(hz(1000), Some(1193));
        assert_eq!(hz(1001), Some(1191));
        assert_eq!(hz(2280), Some(523));
        // A rest stays silent.
        assert_eq!(hz(0x7D00), None);
        // The effect voice is not clamped.
        assert_eq!(
            Voice::Effect(Divisor(500)).tone(rate).map(|t| t.hz.get()),
            Some(2386)
        );
        // Neither is the jingle's carrier.
        assert_eq!(
            Voice::Jingle {
                carrier: Divisor(515),
                beat: Hertz(8)
            }
            .tone(rate),
            Some(Tone {
                hz: Hertz(2316),
                peak: FULL_PEAK
            })
        );
    }

    #[test]
    fn peak_table_for_all_levels() {
        for (n, peak) in [
            (50, 16_383),
            (35, 11_368),
            (20, 6_352),
            (8, 2_340),
            (2, 334),
            (1, 0),
        ] {
            assert_eq!(music_peak(level(n)), peak, "level {n}");
        }
        // Every level from 2 to 50 is audible with a rising peak; level 1
        // is silent.
        let rate = SampleRate::DEFAULT;
        let mut previous = 0;
        for n in 1..=50 {
            let tone = Voice::Music {
                divisor: Divisor(2280),
                level: level(n),
            }
            .tone(rate);
            if n == 1 {
                assert_eq!(tone, None);
            } else {
                let peak = tone.map_or(0, |t| t.peak);
                assert!(peak > previous, "level {n}");
                assert_eq!(peak, music_peak(level(n)));
                previous = peak;
            }
        }
        assert_eq!(previous, FULL_PEAK);
    }

    #[test]
    fn silent_voice_has_no_tone() {
        assert_eq!(Voice::Silent.tone(SampleRate::DEFAULT), None);
    }
}
