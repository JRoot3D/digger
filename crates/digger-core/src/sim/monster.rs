//! Monsters: slots, spawning, decisions, moves, transformations and deaths
//! (design D1).
//!
//! So far only what the fireball needs exists: the six slots with their
//! occupancy, life, kind and position, and the removal of a monster
//! (research 02 §4.1, §4.7). Task group 12 adds the rest of the slot state,
//! spawning and movement.

#[cfg(any(test, feature = "testkit"))]
use crate::assets::ImageId;
use crate::geom::Px;

use super::Ctx;
use super::ids::MonsterSlot;

/// One monster slot (`mondat[]` with its object).
///
/// The values stay in the slot after the monster is removed (design D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Monster {
    /// Whether the slot is in use (`flag`).
    pub(crate) occupied: bool,
    /// Whether the monster is alive (`mop.alive`).
    pub(crate) alive: bool,
    /// Whether the monster is a nobbin rather than a hobbin (`mop.nobf`).
    pub(crate) nobbin: bool,
    /// The position (`mop.pos.x`, `mop.pos.y`).
    pub(crate) position: Px,
}

impl Monster {
    /// Whether the slot is in use (hashed as `flag`).
    pub const fn occupied(&self) -> bool {
        self.occupied
    }

    /// Whether the monster is alive (hashed as `alive`).
    pub const fn alive(&self) -> bool {
        self.alive
    }

    /// Whether the monster is a nobbin (hashed as `nobf`).
    pub const fn nobbin(&self) -> bool {
        self.nobbin
    }

    /// The position (hashed as `x`, `y`).
    pub const fn position(&self) -> Px {
        self.position
    }

    /// A living nobbin at `at` in an occupied slot.
    #[cfg(any(test, feature = "testkit"))]
    pub(crate) const fn nobbin_at(at: Px) -> Self {
        Self {
            occupied: true,
            alive: true,
            nobbin: true,
            position: at,
        }
    }
}

impl Ctx<'_> {
    /// Removes the monster in `slot` if the slot is in use: the slot is
    /// freed, the monster dies and its sprite is erased (research 02 §4.7,
    /// `killmon`). The bonus-mode rise of the round's total (quirk Q31) is
    /// added with bonus mode.
    pub(crate) fn remove_monster(&mut self, slot: MonsterSlot) {
        let monster = slot.of_mut(&mut self.sim.monsters);
        if !monster.occupied {
            return;
        }
        monster.occupied = false;
        monster.alive = false;
        self.gfx.erase(slot.sprite());
    }

    /// Puts a living nobbin into `slot` at `at` and places its sprite, for
    /// the test kit until spawning exists (task group 12).
    #[cfg(any(test, feature = "testkit"))]
    pub(crate) fn put_nobbin(&mut self, slot: MonsterSlot, at: Px) {
        *slot.of_mut(&mut self.sim.monsters) = Monster::nobbin_at(at);
        self.gfx.erase(slot.sprite());
        self.gfx.place(slot.sprite(), ImageId::NOBBIN_1, at);
    }
}
