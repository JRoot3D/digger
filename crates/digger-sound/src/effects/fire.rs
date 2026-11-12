//! The fireball flight effect (research 04 §2).

/// The base divisor a flight starts from.
const START: u16 = 500;
/// The flight ends by itself once its base exceeds this.
const LAST: u16 = 30_000;

/// One fireball's flight: every second tick the base divisor grows by a
/// 55th of itself and sounds, with noise added by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fire {
    base: u16,
}

/// One fireball's flight effect and its two-tick cadence.
///
/// The cadence bit lives outside the `Option` because a fire-start for a
/// flying fireball resets only the base, not the rhythm (kept quirk SQ4).
/// A fire-stop, an explosion and the flight's own end reset both.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FireSlot {
    flight: Option<Fire>,
    /// Whether the next active tick is a growth tick.
    cadence: bool,
}

impl FireSlot {
    /// Starts or restarts the flight from base 500, keeping the cadence.
    pub(crate) fn start(&mut self) {
        self.flight = Some(Fire { base: START });
    }

    /// Ends the flight and resets the cadence.
    pub(crate) fn stop(&mut self) {
        *self = Self::default();
    }

    /// Whether the flight effect is active.
    pub(crate) const fn is_active(self) -> bool {
        self.flight.is_some()
    }

    /// Advances the flight by one tick and returns the new base on a tick
    /// on which it sounds.
    ///
    /// A flight whose base exceeds 30000 still sounds on that tick and
    /// then ends.
    pub(crate) fn tick(&mut self) -> Option<u16> {
        let fire = self.flight.as_mut()?;
        if !self.cadence {
            self.cadence = true;
            return None;
        }
        fire.base += fire.base / 55;
        let base = fire.base;
        if base > LAST {
            self.stop();
        } else {
            self.cadence = false;
        }
        Some(base)
    }
}
