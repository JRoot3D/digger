//! Gold bags: wobble, fall, push, chain push, gold and the round-end tidy
//! (design D1).
//!
//! So far only what the fireball needs exists: the seven bags with their
//! existence and position (research 02 §5.1). Task group 11 adds the rest of
//! the bag state, creation from the level and every bag motion.

#[cfg(any(test, feature = "testkit"))]
use crate::assets::ImageId;
use crate::geom::Px;

#[cfg(any(test, feature = "testkit"))]
use super::Ctx;
#[cfg(any(test, feature = "testkit"))]
use super::ids::BagId;

/// One gold bag (`bagdat[]`).
///
/// The values stay in the record after the bag is removed (design D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Bag {
    /// Whether the bag exists (`exist`).
    pub(crate) exists: bool,
    /// The position (`x`, `y`).
    pub(crate) position: Px,
}

impl Bag {
    /// Whether the bag exists (hashed as `exist`).
    pub const fn exists(&self) -> bool {
        self.exists
    }

    /// The position (hashed as `x`, `y`).
    pub const fn position(&self) -> Px {
        self.position
    }
}

#[cfg(any(test, feature = "testkit"))]
impl Ctx<'_> {
    /// Puts resting bag `id` at `at` and places its sprite, for the test
    /// kit until bags are created from the level (task group 11).
    pub(crate) fn put_bag(&mut self, id: BagId, at: Px) {
        *id.of_mut(&mut self.sim.bags) = Bag {
            exists: true,
            position: at,
        };
        self.gfx.erase(id.sprite());
        self.gfx.place(id.sprite(), ImageId::BAG_STILL, at);
    }
}
