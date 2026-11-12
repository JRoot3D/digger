//! The sound effects and the order in which they are heard (design D3).
//!
//! Each effect is a small state machine kept in an `Option` slot of
//! [`Effects`]: `None` is "inactive", starting an effect constructs a fresh
//! state and stopping it drops the state. Once per tick every active
//! effect advances and returns a [`Step`]; the offers are folded in the
//! original's updater order, so the last effect in that order that offers
//! a divisor is heard (kept quirk SQ3).

use crate::units::Divisor;

/// What one effect did on one tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Step {
    /// The divisor the effect offers on this tick, if any.
    pub offer: Option<Divisor>,
    /// Whether the effect has finished. An effect may offer a divisor on
    /// the tick on which it finishes (kept quirk SQ5).
    pub done: bool,
}

impl Step {
    /// A tick on which the effect offers `divisor` and keeps running.
    pub const fn offer(divisor: Divisor) -> Self {
        Self {
            offer: Some(divisor),
            done: false,
        }
    }

    /// A tick on which the effect offers nothing and keeps running.
    pub const fn quiet() -> Self {
        Self {
            offer: None,
            done: false,
        }
    }
}

/// One effect's state machine.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the effect slots are added in task group 4")
)]
trait Effect {
    /// Advances the effect by one tick.
    fn tick(&mut self) -> Step;
}

/// Advances the effect in `slot`, if one is active, ends it when it reports
/// done and returns its offer.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the effect slots are added in task group 4")
)]
fn advance<E: Effect>(slot: &mut Option<E>) -> Option<Divisor> {
    let step = slot.as_mut()?.tick();
    if step.done {
        *slot = None;
    }
    step.offer
}

/// Folds the offers of one tick: a later offer replaces an earlier one, an
/// absent offer replaces nothing.
fn fold(offers: impl IntoIterator<Item = Option<Divisor>>) -> Option<Divisor> {
    offers
        .into_iter()
        .fold(None, |heard, offer| offer.or(heard))
}

/// All effect slots.
///
/// Every slot starts empty. The effects themselves are added slot by slot;
/// the order in which [`Effects::tick`] folds their offers is fixed here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Effects {
    /// A stand-in effect for tests of the command path, used until the
    /// real effects exist.
    #[cfg(test)]
    pub(crate) probe: Option<probe::Probe>,
}

impl Effects {
    /// All slots empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ends every effect (the stop-all command).
    ///
    /// Only the effect slots are cleared; state that a stop-all keeps, such
    /// as the noise generator, is not touched.
    pub fn stop_all(&mut self) {
        *self = Self {
            #[cfg(test)]
            probe: None,
        };
    }

    /// Advances every active effect by one tick and returns the divisor
    /// that is heard, if any effect offers one.
    ///
    /// The slots are advanced in the original's updater order: emerald
    /// scale, wobble, death, break, gold, emerald click, explosion,
    /// fireball flight, eat monster, fall, extra life, bonus siren. The
    /// last offer wins, which gives the spec's priority from highest to
    /// lowest: bonus siren, extra life, bag fall, eat monster, fireball
    /// flight, explosion, emerald click, gold, bag break, death, wobble,
    /// emerald scale. An effect that offers nothing on a tick does not
    /// block a lower one, and every active effect advances whether or not
    /// it is heard (research 04 §1.3).
    pub fn tick(&mut self) -> Option<Divisor> {
        fold([
            #[cfg(test)]
            advance(&mut self.probe),
        ])
    }
}

/// A test-only effect that offers one divisor on every tick.
#[cfg(test)]
pub(crate) mod probe {
    use super::{Effect, Step};
    use crate::units::Divisor;

    /// Offers its divisor on every tick until stopped.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct Probe(pub(crate) Divisor);

    impl Effect for Probe {
        fn tick(&mut self) -> Step {
            Step::offer(self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An effect that offers on odd ticks and is done after `len` ticks.
    struct Blink {
        divisor: Divisor,
        tick: u32,
        len: u32,
    }

    impl Effect for Blink {
        fn tick(&mut self) -> Step {
            self.tick += 1;
            Step {
                offer: (self.tick % 2 == 1).then_some(self.divisor),
                done: self.tick == self.len,
            }
        }
    }

    #[test]
    fn last_offer_wins_and_absent_offers_do_not_block() {
        let (a, b) = (Some(Divisor(100)), Some(Divisor(200)));
        assert_eq!(fold([]), None);
        assert_eq!(fold([a, b]), b);
        assert_eq!(fold([b, a]), a);
        assert_eq!(fold([a, None]), a);
        assert_eq!(fold([None, b, None]), b);
    }

    #[test]
    fn done_effect_is_dropped_after_its_last_offer() {
        let mut slot = Some(Blink {
            divisor: Divisor(1000),
            tick: 0,
            len: 3,
        });
        assert_eq!(advance(&mut slot), Some(Divisor(1000)));
        assert_eq!(advance(&mut slot), None);
        assert!(slot.is_some());
        // The third tick both offers and finishes.
        assert_eq!(advance(&mut slot), Some(Divisor(1000)));
        assert!(slot.is_none());
        assert_eq!(advance(&mut slot), None);
    }

    #[test]
    fn empty_effects_offer_nothing() {
        let mut effects = Effects::new();
        for _ in 0..100 {
            assert_eq!(effects.tick(), None);
        }
    }

    #[test]
    fn stop_all_empties_every_slot() {
        let mut effects = Effects::new();
        effects.probe = Some(probe::Probe(Divisor(500)));
        assert_eq!(effects.tick(), Some(Divisor(500)));
        effects.stop_all();
        assert_eq!(effects, Effects::new());
        assert_eq!(effects.tick(), None);
    }
}
