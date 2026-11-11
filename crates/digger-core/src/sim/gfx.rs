//! The picture and the sprite layer the simulation draws into, with the
//! entity-draw helpers (design D4).
//!
//! Collision results exist only as the value a sprite move returns: every
//! helper that moves a sprite hands back the [`Collisions`] of that move,
//! so game logic reads the collisions of its own draw and never a global
//! list (research 02 §1, research 03 §7.4).

use crate::assets::ImageId;
use crate::geom::Px;
use crate::sound_cmd::FireballId;
use crate::sprite::{Collisions, SpriteId, SpriteLayer};
use crate::video::Framebuffer;

use super::ids::{BagId, DiggerId, MonsterSlot};

/// The framebuffer and the sprite layer drawn over it.
///
/// Entity structs do not know about the picture; drawing an entity is a
/// call here that names the entity by its typed id.
#[derive(Debug, Clone)]
pub(crate) struct Gfx {
    fb: Framebuffer,
    sprites: SpriteLayer,
}

#[allow(
    dead_code,
    reason = "the systems of later task groups draw through these"
)]
impl Gfx {
    /// A black picture and a sprite layer with every sprite disabled.
    pub(crate) fn new() -> Self {
        Self {
            fb: Framebuffer::new(),
            sprites: SpriteLayer::new(),
        }
    }

    /// The picture.
    pub(crate) const fn fb(&self) -> &Framebuffer {
        &self.fb
    }

    /// The picture, for static drawing (background, tunnels, text).
    pub(crate) const fn fb_mut(&mut self) -> &mut Framebuffer {
        &mut self.fb
    }

    /// The sprite layer.
    pub(crate) const fn sprites(&self) -> &SpriteLayer {
        &self.sprites
    }

    /// Sets the sprite's current and pending image and disables it, without
    /// touching the picture (quirk S6).
    pub(crate) fn reset(&mut self, sprite: SpriteId, image: ImageId) {
        self.sprites.reset(sprite, image);
    }

    /// Sets the image the sprite shows from its next place or move.
    pub(crate) fn set_pending(&mut self, sprite: SpriteId, image: ImageId) {
        self.sprites.set_pending(sprite, image);
    }

    /// Places the sprite at `at` showing `image`; a place computes no
    /// collisions.
    pub(crate) fn place(&mut self, sprite: SpriteId, image: ImageId, at: Px) {
        self.sprites.set_pending(sprite, image);
        self.sprites.place(&mut self.fb, sprite, at);
    }

    /// Moves the sprite to `at` showing `image` and returns the collisions
    /// of this move.
    pub(crate) fn move_to(&mut self, sprite: SpriteId, image: ImageId, at: Px) -> Collisions {
        self.sprites.set_pending(sprite, image);
        self.sprites.move_to(&mut self.fb, sprite, at)
    }

    /// Empties the collision lists, as a draw that draws nothing does
    /// (research 02 §1 rule 4).
    pub(crate) fn clear_collisions(&mut self) {
        self.sprites.clear_collisions();
    }

    /// Erases the sprite if it is enabled.
    pub(crate) fn erase(&mut self, sprite: SpriteId) {
        self.sprites.erase(&mut self.fb, sprite);
    }

    /// Draws `image` into the background under every sprite.
    pub(crate) fn draw_static(&mut self, image: ImageId, at: Px) {
        self.sprites.draw_static(&mut self.fb, image, at);
    }

    /// Moves digger `id`'s sprite; returns the collisions of the move.
    pub(crate) fn move_digger(&mut self, id: DiggerId, image: ImageId, at: Px) -> Collisions {
        self.move_to(id.sprite(), image, at)
    }

    /// Moves monster `slot`'s sprite; returns the collisions of the move.
    pub(crate) fn move_monster(&mut self, slot: MonsterSlot, image: ImageId, at: Px) -> Collisions {
        self.move_to(slot.sprite(), image, at)
    }

    /// Moves bag `id`'s sprite; returns the collisions of the move.
    pub(crate) fn move_bag(&mut self, id: BagId, image: ImageId, at: Px) -> Collisions {
        self.move_to(id.sprite(), image, at)
    }

    /// Moves fireball `id`'s sprite; returns the collisions of the move.
    pub(crate) fn move_fireball(&mut self, id: FireballId, image: ImageId, at: Px) -> Collisions {
        self.move_to(id.sprite(), image, at)
    }

    /// Moves the bonus sprite; returns the collisions of the move.
    pub(crate) fn move_bonus(&mut self, image: ImageId, at: Px) -> Collisions {
        self.move_to(SpriteId::BONUS, image, at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprite::SpriteClass;

    fn bag(i: u8) -> BagId {
        BagId::new(i).unwrap_or_else(|| unreachable!())
    }

    fn monster(i: u8) -> MonsterSlot {
        MonsterSlot::new(i).unwrap_or_else(|| unreachable!())
    }

    #[test]
    fn collisions_report_lowest_slot_first_per_class() {
        let mut gfx = Gfx::new();
        // Two bags overlapping each other, placed higher slot first, and two
        // monsters likewise.
        gfx.place(bag(3).sprite(), ImageId::BAG_STILL, Px::new(100, 50));
        gfx.place(bag(1).sprite(), ImageId::BAG_STILL, Px::new(108, 52));
        gfx.place(monster(4).sprite(), ImageId::NOBBIN_1, Px::new(100, 60));
        gfx.place(monster(2).sprite(), ImageId::NOBBIN_1, Px::new(104, 62));
        let digger = DiggerId::ALL[0];
        gfx.place(digger.sprite(), ImageId::BLANK, Px::new(200, 150));
        let hits = gfx.move_digger(digger, ImageId::BAG_STILL, Px::new(104, 56));
        assert_eq!(gfx.sprites().position(digger.sprite()), Px::new(104, 56));
        assert_eq!(hits.first(SpriteClass::Bag), Some(bag(1).sprite()));
        assert_eq!(
            hits.of(SpriteClass::Bag).collect::<Vec<_>>(),
            [bag(1).sprite(), bag(3).sprite()]
        );
        assert_eq!(hits.first(SpriteClass::Monster), Some(monster(2).sprite()));
        assert_eq!(
            hits.of(SpriteClass::Monster).collect::<Vec<_>>(),
            [monster(2).sprite(), monster(4).sprite()]
        );
        assert!(
            hits.first(SpriteClass::Digger).is_none(),
            "a move never hits itself"
        );
        // The value handed back is the layer's record of that move.
        assert_eq!(hits, gfx.sprites().collisions());

        // A bag move reports the other bag and the monsters it touches.
        let hits = gfx.move_bag(bag(3), ImageId::BAG_STILL, Px::new(100, 50));
        assert_eq!(hits.first(SpriteClass::Bag), Some(bag(1).sprite()));
        assert!(hits.contains(digger.sprite()));
    }

    #[test]
    fn erase_and_place_leave_no_collisions_of_their_own() {
        let mut gfx = Gfx::new();
        gfx.place(bag(0).sprite(), ImageId::BAG_STILL, Px::new(40, 40));
        let digger = DiggerId::ALL[1];
        let hits = gfx.move_digger(digger, ImageId::BAG_STILL, Px::new(44, 40));
        assert_eq!(hits.first(SpriteClass::Bag), Some(bag(0).sprite()));
        gfx.erase(bag(0).sprite());
        assert!(!gfx.sprites().is_enabled(bag(0).sprite()));
        let hits = gfx.move_digger(digger, ImageId::BAG_STILL, Px::new(48, 40));
        assert!(hits.is_empty(), "an erased sprite is not collided with");
    }
}
