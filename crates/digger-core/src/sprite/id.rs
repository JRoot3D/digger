//! Sprite slots and their classes.

use std::ops::RangeInclusive;

/// The number of sprite slots: 18 (research 03 §7.1).
pub(super) const SLOTS: usize = 18;

/// One of the 18 sprite slots, 0 to 17.
///
/// Slots are grouped into classes in a fixed order (research 03 §7.1;
/// research 02 §0): slot 0 is the bonus, 1–7 the bags, 8–13 the monsters,
/// 14–15 the fireballs and 16–17 the diggers. The slot number is also the
/// stacking order: a higher slot is drawn over a lower one.
///
/// The per-class constructors take the index within the class, counted from
/// 0, and return `None` when the class has no such slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpriteId(u8);

impl SpriteId {
    /// The number of sprite slots: 18.
    pub const COUNT: usize = SLOTS;

    /// The bonus sprite, slot 0.
    pub const BONUS: Self = Self(0);

    /// Slot `n`, or `None` for 18 and above.
    pub const fn new(n: u8) -> Option<Self> {
        if (n as usize) < SLOTS {
            Some(Self(n))
        } else {
            None
        }
    }

    /// Bag `i` (0 to 6), slot `1 + i`.
    pub const fn bag(i: u8) -> Option<Self> {
        SpriteClass::Bag.slot(i)
    }

    /// Monster `i` (0 to 5), slot `8 + i`.
    pub const fn monster(i: u8) -> Option<Self> {
        SpriteClass::Monster.slot(i)
    }

    /// The fireball of digger `i` (0 or 1), slot `14 + i`.
    pub const fn fireball(i: u8) -> Option<Self> {
        SpriteClass::Fireball.slot(i)
    }

    /// Digger `i` (0 or 1), slot `16 + i`.
    pub const fn digger(i: u8) -> Option<Self> {
        SpriteClass::Digger.slot(i)
    }

    /// Every slot in ascending order.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..=17).map(Self)
    }

    /// The slot number, 0 to 17.
    pub const fn number(self) -> u8 {
        self.0
    }

    /// The slot number as an index into per-slot storage.
    pub(super) const fn index(self) -> usize {
        self.0 as usize
    }

    /// The class this slot belongs to.
    pub const fn class(self) -> SpriteClass {
        match self.0 {
            0 => SpriteClass::Bonus,
            1..=7 => SpriteClass::Bag,
            8..=13 => SpriteClass::Monster,
            14..=15 => SpriteClass::Fireball,
            _ => SpriteClass::Digger,
        }
    }
}

/// The five sprite classes, in stacking order (research 03 §7.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpriteClass {
    /// Slot 0.
    Bonus,
    /// Slots 1–7.
    Bag,
    /// Slots 8–13.
    Monster,
    /// Slots 14–15.
    Fireball,
    /// Slots 16–17.
    Digger,
}

impl SpriteClass {
    /// The five classes in ascending order.
    pub const ALL: [Self; 5] = [
        Self::Bonus,
        Self::Bag,
        Self::Monster,
        Self::Fireball,
        Self::Digger,
    ];

    /// The first and last slot number of the class.
    const fn bounds(self) -> (u8, u8) {
        match self {
            Self::Bonus => (0, 0),
            Self::Bag => (1, 7),
            Self::Monster => (8, 13),
            Self::Fireball => (14, 15),
            Self::Digger => (16, 17),
        }
    }

    /// The slot numbers of the class, inclusive.
    pub const fn slot_numbers(self) -> RangeInclusive<u8> {
        let (first, last) = self.bounds();
        first..=last
    }

    /// The number of slots in the class.
    pub const fn slot_count(self) -> usize {
        let (first, last) = self.bounds();
        (last - first + 1) as usize
    }

    /// Slot `i` of the class, counted from 0, or `None` past its end.
    pub const fn slot(self, i: u8) -> Option<SpriteId> {
        let (first, last) = self.bounds();
        if i <= last - first {
            Some(SpriteId(first + i))
        } else {
            None
        }
    }

    /// The slots of the class in ascending order.
    pub fn slots(self) -> impl Iterator<Item = SpriteId> {
        self.slot_numbers().map(SpriteId)
    }
}

/// A set of sprite slots, one bit per slot; iteration is in ascending slot
/// order, which is the stacking order.
///
/// Quirk S7: stacking order is slot order, so a higher slot is always drawn
/// on top whichever sprite moved last (docs/compat-quirks.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(super) struct SlotSet(u32);

impl SlotSet {
    /// The empty set.
    pub(super) const EMPTY: Self = Self(0);

    /// The set holding only `id`.
    pub(super) const fn single(id: SpriteId) -> Self {
        Self(1 << id.0)
    }

    /// Whether `id` is in the set.
    pub(super) const fn contains(self, id: SpriteId) -> bool {
        self.0 & (1 << id.0) != 0
    }

    /// Adds `id`.
    pub(super) const fn insert(&mut self, id: SpriteId) {
        self.0 |= 1 << id.0;
    }

    /// Whether the set is empty.
    pub(super) const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The union of both sets.
    pub(super) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The lowest slot in the set, removed from it.
    pub(super) fn pop_first(&mut self) -> Option<SpriteId> {
        if self.0 == 0 {
            return None;
        }
        // Only bits 0 to 17 are ever set, so the index is a valid slot.
        let first = u8::try_from(self.0.trailing_zeros()).ok()?;
        self.0 &= self.0 - 1;
        SpriteId::new(first)
    }

    /// The members in ascending slot order.
    pub(super) fn iter(self) -> impl Iterator<Item = SpriteId> {
        let mut rest = self;
        std::iter::from_fn(move || rest.pop_first())
    }
}
