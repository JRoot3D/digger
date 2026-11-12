//! Per-tune parameters and the note envelope (design D4; research 04 §3.2,
//! §3.3).
//!
//! Upstream keeps the envelope stage in an integer (`musicstage` 0, 1, 2)
//! and the parameters in loose statics set by a `switch`. Here the
//! parameters of each tune are one constant [`TuneParams`] from
//! [`params`], the stage is the enum [`EnvelopeStage`], and one tick of
//! the envelope is the pure function [`envelope_step`].

use crate::command::{Tempo, Tune};
use crate::units::Level;

/// How long a note is held before its release starts (research 04 §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GateRule {
    /// A fixed number of duration units for every note, whatever its
    /// length: the normal tune's staccato of 2 units (kept quirk SQ7).
    FixedUnits(u16),
    /// The note's duration less one unit.
    DurationLessOneUnit,
}

/// The constants that shape every note of one tune (research 04 §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TuneParams {
    /// Added to the level each tick of the attack, up to 50.
    pub attack: u8,
    /// The level the decay stops at.
    pub sustain: u8,
    /// Subtracted from the level each tick of the decay.
    pub decay: u8,
    /// Subtracted from the level each tick of the release, down to 1.
    pub release: u8,
    /// Ticks per duration unit at the standard tempo.
    pub standard_ticks: u16,
    /// Ticks per duration unit at the quick tempo: 0.7 of the standard,
    /// truncated (kept quirk SQ8).
    pub quick_ticks: u16,
    /// When the release starts.
    pub gate: GateRule,
}

/// The parameters of `tune`: attack, sustain, decay and release of 50, 8,
/// 15, 1 for the normal tune, 20, 20, 10, 4 for bonus, 50, 25, 5, 1 for
/// the dirge and 28, 26, 8, 4 for battle; 6, 3, 10 and 2 ticks per unit at
/// the standard tempo and 4, 2, 7 and 1 at the quick tempo.
pub const fn params(tune: Tune) -> TuneParams {
    match tune {
        Tune::Main => TuneParams {
            attack: 50,
            sustain: 8,
            decay: 15,
            release: 1,
            standard_ticks: 6,
            quick_ticks: 4,
            gate: GateRule::FixedUnits(2),
        },
        Tune::Bonus => TuneParams {
            attack: 20,
            sustain: 20,
            decay: 10,
            release: 4,
            standard_ticks: 3,
            quick_ticks: 2,
            gate: GateRule::DurationLessOneUnit,
        },
        Tune::Dirge => TuneParams {
            attack: 50,
            sustain: 25,
            decay: 5,
            release: 1,
            standard_ticks: 10,
            quick_ticks: 7,
            gate: GateRule::DurationLessOneUnit,
        },
        Tune::LastLife => TuneParams {
            attack: 28,
            sustain: 26,
            decay: 8,
            release: 4,
            standard_ticks: 2,
            quick_ticks: 1,
            gate: GateRule::DurationLessOneUnit,
        },
    }
}

impl TuneParams {
    /// The number of ticks one duration unit lasts at `tempo`.
    pub const fn ticks_per_unit(self, tempo: Tempo) -> u16 {
        match tempo {
            Tempo::Standard => self.standard_ticks,
            Tempo::Quick => self.quick_ticks,
        }
    }

    /// The timing of a note of `units` duration units at `tempo`.
    pub const fn note_timing(self, units: u16, tempo: Tempo) -> NoteTiming {
        let per_unit = self.ticks_per_unit(tempo) as u32;
        let countdown = units as u32 * per_unit;
        let gate = match self.gate {
            GateRule::FixedUnits(n) => n as u32 * per_unit,
            GateRule::DurationLessOneUnit => countdown.saturating_sub(per_unit),
        };
        NoteTiming { countdown, gate }
    }
}

/// How long a note lasts and when its release starts, in ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NoteTiming {
    /// The ticks after the note's first (loading) tick: the note occupies
    /// `countdown + 1` ticks (kept quirk SQ7).
    pub countdown: u32,
    /// The tick of the note, counted from 1, from which the release runs.
    pub gate: u32,
}

impl NoteTiming {
    /// The number of ticks the note occupies.
    pub const fn length(self) -> u32 {
        self.countdown + 1
    }
}

/// The stage of the note envelope (upstream's `musicstage` 0, 1, 2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum EnvelopeStage {
    /// The level rises by the attack step until it reaches 50.
    #[default]
    Attack,
    /// The level falls by the decay step down to the sustain level.
    Decay,
    /// The level falls by the release step down to 1.
    Release,
}

/// One tick of the envelope: the stage and level after the tick.
///
/// `gated` is whether the note's tick count has reached its gate length;
/// from then on the stage is [`EnvelopeStage::Release`] whatever it was.
/// An attack that reaches 50 moves to the decay, which starts on the next
/// tick (research 04 §3.3).
pub const fn envelope_step(
    params: TuneParams,
    stage: EnvelopeStage,
    level: Level,
    gated: bool,
) -> (EnvelopeStage, Level) {
    let stage = if gated { EnvelopeStage::Release } else { stage };
    let now = level.get();
    match stage {
        EnvelopeStage::Attack => {
            if now as u16 + params.attack as u16 >= Level::MAX.get() as u16 {
                (EnvelopeStage::Decay, Level::MAX)
            } else {
                (stage, Level::saturating(now + params.attack))
            }
        }
        EnvelopeStage::Decay => {
            if now as i16 - params.decay as i16 <= params.sustain as i16 {
                (stage, Level::saturating(params.sustain))
            } else {
                (stage, Level::saturating(now - params.decay))
            }
        }
        EnvelopeStage::Release => {
            if now as i16 - params.release as i16 <= Level::MIN.get() as i16 {
                (stage, Level::MIN)
            } else {
                (stage, Level::saturating(now - params.release))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The levels on the ticks of one note of `units` units of `tune` at
    /// the standard tempo, starting from `start`.
    fn note_levels(tune: Tune, units: u16, start: u8) -> Vec<u8> {
        let params = params(tune);
        let timing = params.note_timing(units, Tempo::Standard);
        let mut stage = EnvelopeStage::Attack;
        let mut level = Level::saturating(start);
        (1..=timing.length())
            .map(|tick| {
                (stage, level) = envelope_step(params, stage, level, tick >= timing.gate);
                level.get()
            })
            .collect()
    }

    #[test]
    fn tempo_table() {
        let tunes = [Tune::Main, Tune::Bonus, Tune::Dirge, Tune::LastLife];
        let at = |tempo| tunes.map(|tune| params(tune).ticks_per_unit(tempo));
        assert_eq!(at(Tempo::Standard), [6, 3, 10, 2]);
        assert_eq!(at(Tempo::Quick), [4, 2, 7, 1]);
        // The quick tempo is 0.7 of the standard one, truncated.
        for tune in tunes {
            let p = params(tune);
            assert_eq!(p.quick_ticks, p.standard_ticks * 7 / 10, "{tune:?}");
        }
    }

    #[test]
    fn note_timing_per_tune() {
        // Normal: 2 units = 12 + 1 ticks, gate fixed at 2 units.
        let main = params(Tune::Main);
        assert_eq!(
            main.note_timing(2, Tempo::Standard),
            NoteTiming {
                countdown: 12,
                gate: 12
            }
        );
        assert_eq!(main.note_timing(4, Tempo::Standard).gate, 12);
        assert_eq!(main.note_timing(2, Tempo::Standard).length(), 13);
        // Bonus: 2 units = 6 + 1 ticks, gate one unit less.
        let bonus = params(Tune::Bonus);
        assert_eq!(bonus.note_timing(2, Tempo::Standard).length(), 7);
        assert_eq!(bonus.note_timing(4, Tempo::Standard).gate, 9);
        // Quick dirge: 7 ticks per unit.
        assert_eq!(params(Tune::Dirge).note_timing(1, Tempo::Quick).length(), 8);
    }

    #[test]
    fn normal_tune_two_unit_note_levels() {
        assert_eq!(
            note_levels(Tune::Main, 2, 6),
            [50, 35, 20, 8, 8, 8, 8, 8, 8, 8, 8, 7, 6]
        );
    }

    #[test]
    fn bonus_four_unit_note_levels() {
        assert_eq!(
            note_levels(Tune::Bonus, 4, 8),
            [28, 48, 50, 40, 30, 20, 20, 20, 16, 12, 8, 4, 1]
        );
    }

    #[test]
    fn battle_four_unit_note_levels() {
        assert_eq!(
            note_levels(Tune::LastLife, 4, 8),
            [36, 50, 42, 34, 26, 22, 18, 14, 10]
        );
    }

    #[test]
    fn attack_reaches_the_maximum_then_decays() {
        let main = params(Tune::Main);
        let (stage, level) =
            envelope_step(main, EnvelopeStage::Attack, Level::saturating(6), false);
        assert_eq!((stage, level.get()), (EnvelopeStage::Decay, 50));
        let (stage, level) = envelope_step(main, stage, level, false);
        assert_eq!((stage, level.get()), (EnvelopeStage::Decay, 35));
    }

    #[test]
    fn release_starts_at_the_gate_and_stops_at_1() {
        let main = params(Tune::Main);
        let (stage, level) = envelope_step(main, EnvelopeStage::Decay, Level::saturating(8), true);
        assert_eq!((stage, level.get()), (EnvelopeStage::Release, 7));
        let (_, level) = envelope_step(main, EnvelopeStage::Release, Level::MIN, false);
        assert_eq!(level, Level::MIN);
        // Release takes over from an unfinished attack, too.
        let bonus = params(Tune::Bonus);
        let (stage, level) =
            envelope_step(bonus, EnvelopeStage::Attack, Level::saturating(3), true);
        assert_eq!((stage, level), (EnvelopeStage::Release, Level::MIN));
    }
}
