//! The five collision lists produced by a move.

use super::id::{SlotSet, SpriteClass, SpriteId};

/// The collision lists of one move: per class, the enabled sprites other
/// than the moved one whose rectangle overlapped the moved sprite's new
/// rectangle (research 03 §7.4; research 02 §1).
///
/// The value is a set of slots. Reading it per class gives that class's list
/// in ascending slot order: [`first`](Self::first) is the head of the list,
/// [`of`](Self::of) the whole list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Collisions(SlotSet);

impl Collisions {
    /// All five lists empty.
    pub const EMPTY: Self = Self(SlotSet::EMPTY);

    /// The lists holding exactly the slots of `set`.
    pub(super) const fn from_set(set: SlotSet) -> Self {
        Self(set)
    }

    /// The list of `class`, in ascending slot order.
    pub fn of(self, class: SpriteClass) -> impl Iterator<Item = SpriteId> {
        class.slots().filter(move |&id| self.0.contains(id))
    }

    /// The lowest slot in the list of `class`, if the list is not empty.
    pub fn first(self, class: SpriteClass) -> Option<SpriteId> {
        self.of(class).next()
    }

    /// Whether `id` is in its class's list.
    pub const fn contains(self, id: SpriteId) -> bool {
        self.0.contains(id)
    }

    /// Whether all five lists are empty.
    pub const fn is_empty(self) -> bool {
        self.0.is_empty()
    }

    /// Every listed slot, class by class, each class in ascending order;
    /// since classes are slot ranges this is ascending slot order.
    pub fn iter(self) -> impl Iterator<Item = SpriteId> {
        self.0.iter()
    }
}
