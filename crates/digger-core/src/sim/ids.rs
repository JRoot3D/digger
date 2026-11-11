//! Typed ids for players, diggers, control slots, monster slots and bags
//! (design D4).
//!
//! Entities are addressed by index, never by a reference held across a call.
//! Each id type holds a value that is in range by construction, so indexing
//! a per-entity array with it cannot fail. `FireballId` is defined with the
//! sound commands that name it (`sound_cmd.rs`) and follows the same pattern.

use crate::sound_cmd::FireballId;
use crate::sprite::SpriteId;

/// Defines an index newtype over `u8` holding `0..$count`; the literal list
/// spells out every number, in order.
macro_rules! index_id {
    ($(#[$meta:meta])* $name:ident, $count:literal: [$($n:literal),+]) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u8);

        impl $name {
            /// The number of distinct ids.
            pub const COUNT: usize = $count;

            /// Every id in ascending order.
            pub const ALL: [Self; $count] = [$(Self($n)),+];

            /// Id `n`, or `None` when `n` is not below
            #[doc = concat!(stringify!($count), ".")]
            pub const fn new(n: u8) -> Option<Self> {
                if (n as usize) < $count {
                    Some(Self(n))
                } else {
                    None
                }
            }

            /// The id's number.
            pub const fn number(self) -> u8 {
                self.0
            }

            /// The id as an index into per-entity storage; always below
            /// [`Self::COUNT`].
            pub const fn index(self) -> usize {
                self.0 as usize
            }

            /// The element of a per-entity array that belongs to this id.
            #[allow(dead_code, reason = "used by the systems of later task groups")]
            pub(crate) fn of<T>(self, items: &[T; $count]) -> &T {
                match items.get(self.index()) {
                    Some(item) => item,
                    None => unreachable!("an id is below its count"),
                }
            }

            /// The element of a per-entity array that belongs to this id,
            /// mutably.
            #[allow(dead_code, reason = "used by the systems of later task groups")]
            pub(crate) fn of_mut<T>(self, items: &mut [T; $count]) -> &mut T {
                match items.get_mut(self.index()) {
                    Some(item) => item,
                    None => unreachable!("an id is below its count"),
                }
            }
        }

        impl TryFrom<u8> for $name {
            type Error = $crate::sim::IdOutOfRange;

            fn try_from(n: u8) -> Result<Self, $crate::sim::IdOutOfRange> {
                Self::new(n).ok_or($crate::sim::IdOutOfRange {
                    value: n,
                    count: $count,
                })
            }
        }
    };
}
pub(crate) use index_id;

/// A number given for an id is not below the number of such ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IdOutOfRange {
    /// The rejected number.
    pub value: u8,
    /// The number of valid ids; valid numbers are below it.
    pub count: usize,
}

impl std::fmt::Display for IdOutOfRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "id {} is out of range: valid ids are 0 to {}",
            self.value,
            self.count.saturating_sub(1)
        )
    }
}

impl std::error::Error for IdOutOfRange {}

index_id!(
    /// One of the two players, 0 or 1; the second exists only in
    /// alternating two-player games, but its state is always kept
    /// (research 01 §2).
    PlayerId,
    2: [0, 1]
);

index_id!(
    /// One of the two diggers, 0 or 1; the second plays only in
    /// two-digger games (research 02 §3).
    DiggerId,
    2: [0, 1]
);

index_id!(
    /// One of the two control slots, 0 or 1. Slot 0 drives digger 0 and
    /// slot 1 drives digger 1 (research 05 §1.6).
    SlotId,
    2: [0, 1]
);

index_id!(
    /// One of the six monster slots, 0 to 5 (research 02 §4).
    MonsterSlot,
    6: [0, 1, 2, 3, 4, 5]
);

index_id!(
    /// One of the seven gold bags, 0 to 6 (research 02 §5).
    BagId,
    7: [0, 1, 2, 3, 4, 5, 6]
);

impl DiggerId {
    /// The digger's sprite, slot 16 + index.
    pub const fn sprite(self) -> SpriteId {
        match SpriteId::digger(self.0) {
            Some(id) => id,
            None => unreachable!(),
        }
    }

    /// The control slot that drives this digger.
    pub const fn slot(self) -> SlotId {
        SlotId(self.0)
    }

    /// The digger's fireball.
    pub const fn fireball(self) -> FireballId {
        match FireballId::new(self.0) {
            Some(id) => id,
            None => unreachable!(),
        }
    }
}

impl MonsterSlot {
    /// The monster's sprite, slot 8 + index.
    pub const fn sprite(self) -> SpriteId {
        match SpriteId::monster(self.0) {
            Some(id) => id,
            None => unreachable!(),
        }
    }

    /// The monster slot whose sprite is `sprite`, if it is a monster sprite.
    pub fn from_sprite(sprite: SpriteId) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.sprite() == sprite)
    }
}

impl BagId {
    /// The bag's sprite, slot 1 + index.
    pub const fn sprite(self) -> SpriteId {
        match SpriteId::bag(self.0) {
            Some(id) => id,
            None => unreachable!(),
        }
    }

    /// The bag whose sprite is `sprite`, if it is a bag sprite.
    pub fn from_sprite(sprite: SpriteId) -> Option<Self> {
        Self::ALL.into_iter().find(|bag| bag.sprite() == sprite)
    }
}

impl DiggerId {
    /// The digger whose sprite is `sprite`, if it is a digger sprite.
    pub fn from_sprite(sprite: SpriteId) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|digger| digger.sprite() == sprite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_reject_out_of_range() {
        assert!(PlayerId::new(1).is_some() && PlayerId::new(2).is_none());
        assert!(DiggerId::new(1).is_some() && DiggerId::new(2).is_none());
        assert!(SlotId::new(1).is_some() && SlotId::new(2).is_none());
        assert!(MonsterSlot::new(5).is_some() && MonsterSlot::new(6).is_none());
        assert!(BagId::new(6).is_some() && BagId::new(7).is_none());
        assert!(FireballId::new(1).is_some() && FireballId::new(2).is_none());
        assert!(BagId::new(255).is_none());
        assert_eq!(BagId::try_from(7), Err(IdOutOfRange { value: 7, count: 7 }));
        assert_eq!(
            IdOutOfRange { value: 7, count: 7 }.to_string(),
            "id 7 is out of range: valid ids are 0 to 6"
        );
        assert_eq!(MonsterSlot::try_from(5).map(MonsterSlot::index), Ok(5));
    }

    #[test]
    fn index_helpers_address_per_entity_arrays() {
        let mut bags = [0u32; BagId::COUNT];
        for bag in BagId::ALL {
            *bag.of_mut(&mut bags) = u32::from(bag.number()) * 10;
        }
        assert_eq!(bags, [0, 10, 20, 30, 40, 50, 60]);
        assert_eq!(BagId::new(3).map(|bag| *bag.of(&bags)), Some(30));
        assert_eq!(MonsterSlot::ALL.len(), 6);
        assert_eq!(
            PlayerId::ALL.map(PlayerId::index),
            [0, 1],
            "ALL is ascending"
        );
    }

    #[test]
    fn ids_map_to_their_sprite_slots() {
        let numbers = |sprites: &[SpriteId]| sprites.iter().map(|s| s.number()).collect::<Vec<_>>();
        assert_eq!(
            numbers(&BagId::ALL.map(BagId::sprite)),
            [1, 2, 3, 4, 5, 6, 7]
        );
        assert_eq!(
            numbers(&MonsterSlot::ALL.map(MonsterSlot::sprite)),
            [8, 9, 10, 11, 12, 13]
        );
        assert_eq!(numbers(&FireballId::ALL.map(FireballId::sprite)), [14, 15]);
        assert_eq!(numbers(&DiggerId::ALL.map(DiggerId::sprite)), [16, 17]);
        for sprite in SpriteId::all() {
            assert_eq!(
                BagId::from_sprite(sprite).map(BagId::sprite),
                (1..=7).contains(&sprite.number()).then_some(sprite)
            );
            assert_eq!(
                MonsterSlot::from_sprite(sprite).is_some(),
                (8..=13).contains(&sprite.number())
            );
            assert_eq!(
                DiggerId::from_sprite(sprite).is_some(),
                (16..=17).contains(&sprite.number())
            );
        }
        assert_eq!(DiggerId::ALL.map(DiggerId::slot), SlotId::ALL);
        assert_eq!(DiggerId::ALL.map(DiggerId::fireball), FireballId::ALL);
    }
}
