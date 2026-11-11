//! The penalty counter that turns drawing and scoring work into monster
//! delays (design D6).
//!
//! The original slowed down on busy ticks; the remastered game emulates
//! that with a counter that is reset at the top of every tick and raised by
//! each dig step, by certain sprite draws and three times by every scoring
//! call. At the end of a main-phase tick an excess over 8 delays some
//! monster slots (scoring spec, "Penalty counter", "Penalty overflow delays
//! monsters"; research 01 §4, research 02 §6; compat-quirks Q01, Q02).
//!
//! The only way to raise the counter is [`Penalty::charge`] with a
//! [`PenaltySite`]. Test builds also count the charges per site, so a
//! scenario can assert which sites fired, not just the total.

use super::ids::MonsterSlot;

/// The counter value above which the excess delays monsters (Q02).
const OVERFLOW_THRESHOLD: u16 = 8;
/// The largest excess that counts; more delays no further slots (Q02).
const OVERFLOW_CAP: u16 = 6;

/// Defines [`PenaltySite`] with its count and list of variants.
macro_rules! penalty_sites {
    ($($(#[$meta:meta])* $name:ident,)+) => {
        /// One place in the game that raises the penalty counter by 1
        /// (scoring spec, "Penalty table for diggers and fireballs",
        /// "Penalty table for monsters and bags"; design D6).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum PenaltySite {
            $($(#[$meta])* $name,)+
        }

        impl PenaltySite {
            /// Every site, in declaration order.
            pub const ALL: [Self; [$(PenaltySite::$name),+].len()] = [$(Self::$name),+];
            /// The number of sites.
            pub const COUNT: usize = Self::ALL.len();

            /// The site's position in [`Self::ALL`].
            pub const fn index(self) -> usize {
                self as usize
            }
        }
    };
}

penalty_sites! {
    /// An earth dig step, by a digger, by a hobbin on every move (even
    /// without a direction) or by a falling bag.
    DigStep,
    /// A digger's move draw, also when it stands still.
    DiggerDraw,
    /// A digger's redraw during a push pause.
    DiggerPushPause,
    /// A digger's redraw after a blocked or failed push.
    DiggerBlockedRedraw,
    /// A fireball's flight draw.
    FireballFlight,
    /// One frame of a fireball explosion.
    ExplosionFrame,
    /// A tick on which a dead digger is carried by a bag.
    CarriedDigger,
    /// A tick of a digger's death arc.
    DeathArc,
    /// A tombstone frame.
    TombstoneFrame,
    /// An emerald redraw.
    EmeraldRedraw,
    /// An emerald eaten, by a digger or a hobbin.
    EmeraldEaten,
    /// A monster's move draw.
    MonsterDraw,
    /// A monster overlapped by a moving monster.
    MonsterOverlap,
    /// A monster's redraw after a blocked move.
    MonsterBlockedRedraw,
    /// A tick on which a dead monster is carried by a bag.
    CarriedMonster,
    /// A frame of a bag's gold opening.
    GoldFrame,
    /// A wobbling bag's redraw.
    WobbleFrame,
    /// A bag landing.
    BagLanding,
    /// The redraw of a falling bag pushed sideways.
    FallingBagPushed,
    /// A falling bag's step draw.
    FallStepDraw,
    /// A pushed bag's slide-step draw.
    SlideStepDraw,
    /// A bag restored after a failed push (up to two per push).
    PushRestore,
    /// Gold collected.
    GoldCollected,
    /// One of the three charges of every scoring call, even of 0 points.
    ScoreCall,
}

/// The penalty counter of the current tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Penalty {
    value: u16,
    /// Charges per site since the last reset (design D6).
    #[cfg(any(test, feature = "testkit"))]
    per_site: [u16; PenaltySite::COUNT],
}

impl Penalty {
    /// Sets the counter to 0, as at the top of every tick.
    #[allow(dead_code, reason = "called by the main-phase tick (task group 7)")]
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Raises the counter by 1 for `site`.
    #[allow(dead_code, reason = "called by the systems of later task groups")]
    pub(crate) fn charge(&mut self, site: PenaltySite) {
        // A tick charges a few hundred times at most; saturation is only a
        // guard against a runaway counter.
        self.value = self.value.saturating_add(1);
        #[cfg(any(test, feature = "testkit"))]
        if let Some(count) = self.per_site.get_mut(site.index()) {
            *count = count.saturating_add(1);
        }
        #[cfg(not(any(test, feature = "testkit")))]
        let _ = site;
    }

    /// The counter's value.
    pub const fn value(self) -> u16 {
        self.value
    }

    /// How often `site` charged since the last reset.
    #[cfg(any(test, feature = "testkit"))]
    pub fn count_of(self, site: PenaltySite) -> u16 {
        self.per_site.get(site.index()).copied().unwrap_or(0)
    }

    /// The monster slots the counter's overflow delays by one tick.
    ///
    /// **Quirk Q02:** with n = min(counter − 8, 6), slots 1 to n − 1 are
    /// delayed, whether or not they are occupied; slot 0 is never delayed
    /// and an excess of exactly 1 delays nothing (research 01 §4,
    /// research 02 §6, §9 item 1).
    pub fn overflow_slots(self) -> impl Iterator<Item = MonsterSlot> {
        let excess = self
            .value
            .saturating_sub(OVERFLOW_THRESHOLD)
            .min(OVERFLOW_CAP);
        MonsterSlot::ALL
            .into_iter()
            .take(usize::from(excess))
            .skip(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_value(value: u16) -> Penalty {
        let mut penalty = Penalty::default();
        for _ in 0..value {
            penalty.charge(PenaltySite::DiggerDraw);
        }
        penalty
    }

    #[test]
    fn penalty_resets_and_counts_per_site() {
        let mut penalty = Penalty::default();
        assert_eq!(penalty.value(), 0);
        // A moving digger eating an emerald (scoring spec, "Digger eats an
        // emerald"): one dig step, one draw, one emerald, three for the score.
        penalty.charge(PenaltySite::DigStep);
        penalty.charge(PenaltySite::DiggerDraw);
        penalty.charge(PenaltySite::EmeraldEaten);
        for _ in 0..3 {
            penalty.charge(PenaltySite::ScoreCall);
        }
        assert_eq!(penalty.value(), 6);
        assert_eq!(penalty.count_of(PenaltySite::DigStep), 1);
        assert_eq!(penalty.count_of(PenaltySite::DiggerDraw), 1);
        assert_eq!(penalty.count_of(PenaltySite::EmeraldEaten), 1);
        assert_eq!(penalty.count_of(PenaltySite::ScoreCall), 3);
        let total: u16 = PenaltySite::ALL.iter().map(|&s| penalty.count_of(s)).sum();
        assert_eq!(total, penalty.value());
        assert_eq!(penalty.count_of(PenaltySite::MonsterDraw), 0);

        penalty.reset();
        assert_eq!(penalty.value(), 0);
        assert!(PenaltySite::ALL.iter().all(|&s| penalty.count_of(s) == 0));
    }

    #[test]
    fn every_site_of_design_d6_exists_once() {
        assert_eq!(PenaltySite::COUNT, 24);
        for (index, site) in PenaltySite::ALL.into_iter().enumerate() {
            assert_eq!(site.index(), index);
        }
    }

    #[test]
    fn quirk_q02_overflow_table() {
        let slots = |value| -> Vec<u8> {
            with_value(value)
                .overflow_slots()
                .map(MonsterSlot::number)
                .collect()
        };
        assert_eq!(slots(0), [] as [u8; 0]);
        assert_eq!(slots(8), [] as [u8; 0]);
        assert_eq!(slots(9), [] as [u8; 0]);
        assert_eq!(slots(10), [1]);
        assert_eq!(slots(12), [1, 2, 3]);
        assert_eq!(slots(14), [1, 2, 3, 4, 5]);
        assert_eq!(slots(20), [1, 2, 3, 4, 5]);
        // Slot 0 is exempt however large the counter.
        assert_eq!(slots(30), [1, 2, 3, 4, 5]);
    }
}
