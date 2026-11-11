//! Sound commands emitted by the simulation and their fixed-capacity queue
//! (design D8).

use crate::sim::index_id;
use crate::sprite::SpriteId;

index_id!(
    /// One of the two fireballs, 0 or 1: fireball `i` belongs to digger `i`
    /// (research 02 §3.6).
    FireballId,
    2: [0, 1]
);

impl FireballId {
    /// The fireball's sprite, slot 14 + index.
    pub const fn sprite(self) -> SpriteId {
        match SpriteId::fireball(self.number()) {
            Some(id) => id,
            None => unreachable!(),
        }
    }
}
