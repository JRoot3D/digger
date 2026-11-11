//! The sprite layer: 18 moving images over a static background, their
//! saved backgrounds and the collision lists.
//!
//! # Slots and stacking
//!
//! There are 18 slots ([`SpriteId`]) in five classes ([`SpriteClass`]):
//! bonus (0), bags (1–7), monsters (8–13), fireballs (14–15) and diggers
//! (16–17). Whenever one operation restores, draws or captures several
//! sprites it does so in ascending slot order, so a higher slot is always
//! drawn on top (research 03 §7.1, §7.3).
//!
//! # State of a sprite
//!
//! Each sprite has a position (x a multiple of 4), an enabled flag, a
//! current image, a pending image and an optional saved background. Its
//! rectangle is its position with the size of its *current* image. The
//! pending image becomes current only at the sprite's next
//! [`place`](SpriteLayer::place) or [`move_to`](SpriteLayer::move_to). A
//! saved background is kept until the sprite's next capture; erasing or
//! resetting does not discard it (research 03 §7.2, §7.6).
//!
//! # Redraw set
//!
//! Before anything is drawn near other sprites, the layer builds a redraw
//! set from a starting rectangle: every enabled sprite (other than the
//! starting slot) that overlaps the rectangle, then every enabled sprite
//! that overlaps a member, until nothing more is added. Members are
//! restored (their saved backgrounds written back) before the new
//! background is captured, so saved backgrounds never contain another
//! sprite's pixels, and are drawn again afterwards (research 03 §7.3).
//!
//! # Operations and their step order
//!
//! - [`reset`](SpriteLayer::reset): current and pending image set, sprite
//!   disabled; the picture, position and saved background are untouched
//!   (quirk S6).
//! - [`set_pending`](SpriteLayer::set_pending): pending image only.
//! - [`place`](SpriteLayer::place): (1) set the aligned position and make
//!   the pending image current; (2) build the redraw set from the new
//!   rectangle; (3) restore the set; (4) capture the background under the
//!   new rectangle; (5) enable; (6) draw the set and the sprite. No
//!   collision lists (quirk S4 when the sprite was already enabled).
//! - [`move_to`](SpriteLayer::move_to): (1) build the redraw set from the
//!   old rectangle, add the set from the new rectangle (aligned position,
//!   pending size) and the sprite itself; (2) restore the set, the sprite at
//!   its old position even when disabled (quirk S3); (3) enable at the new
//!   position with the pending image made current; (4) capture; (5) draw the
//!   set; (6) compute, store and return the collision lists (quirk S2).
//! - [`erase`](SpriteLayer::erase): nothing when disabled; otherwise (1)
//!   restore the saved background; (2) disable; (3) draw, without restoring
//!   or capturing, the redraw set from the sprite's rectangle.
//! - [`draw_static`](SpriteLayer::draw_static): (1) build the redraw set
//!   from the image's area at the unaligned x; (2) restore the set; (3) draw
//!   the image, masked, at the aligned x; (4) capture the set's backgrounds
//!   again; (5) draw the set. The image becomes background under every
//!   sprite (research 03 §7.5).
//! - [`draw_static_unprotected`](SpriteLayer::draw_static_unprotected):
//!   the masked draw at the aligned x only, for areas no sprite can overlap.
//!
//! # Call discipline
//!
//! The layer accepts any sequence of calls and never fails, but its
//! guarantee that the picture equals the static background with the
//! enabled sprites drawn over it in slot order holds only under the
//! original game's discipline: a disabled sprite is always placed before
//! it is moved, and an enabled sprite is never placed. Outside it the layer
//! reproduces the original's behaviour exactly (quirks S3, S4), stale
//! backgrounds included (research 03 §7.4, §7.6).
//!
//! # Collisions
//!
//! Only a move computes [`Collisions`]; they persist, and are what
//! [`collisions`](SpriteLayer::collisions) returns, until the next move or
//! [`clear_collisions`](SpriteLayer::clear_collisions). They depend only on
//! which sprites are enabled and on their current rectangles, not on
//! whether game logic considers a sprite alive (research 02 §1).
//!
//! # Example
//!
//! ```
//! use digger_core::assets::ImageId;
//! use digger_core::geom::Px;
//! use digger_core::sprite::{SpriteClass, SpriteId, SpriteLayer};
//! use digger_core::video::Framebuffer;
//!
//! let mut fb = Framebuffer::new();
//! let mut layer = SpriteLayer::new();
//! let bag = SpriteId::bag(0).unwrap();
//! let digger = SpriteId::digger(0).unwrap();
//! layer.reset(bag, ImageId::BAG_STILL);
//! layer.place(&mut fb, bag, Px::new(100, 50));
//! layer.reset(digger, ImageId::BLANK);
//! layer.place(&mut fb, digger, Px::new(40, 50));
//! let hits = layer.move_to(&mut fb, digger, Px::new(90, 50));
//! assert_eq!(hits.first(SpriteClass::Bag), Some(bag));
//! ```

mod collisions;
mod id;

pub use collisions::Collisions;
pub use id::{SpriteClass, SpriteId};

use id::{SLOTS, SlotSet};

use crate::assets::ImageId;
use crate::geom::{Px, Rect};
use crate::video::{Framebuffer, SavedRect};

/// One sprite slot's state (research 03 §7.2).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sprite {
    enabled: bool,
    pos: Px,
    image: ImageId,
    pending: ImageId,
    saved: Option<SavedRect>,
}

impl Sprite {
    /// Disabled at (0, 0) showing image 0, nothing saved.
    const INITIAL: Self = Self {
        enabled: false,
        pos: Px::new(0, 0),
        image: ImageId::BLANK,
        pending: ImageId::BLANK,
        saved: None,
    };

    /// The rectangle of the current image at the sprite's position.
    ///
    /// Quirk S5: other sprites collide with this, the size of the image
    /// last drawn, even when a pending image of another size is set.
    const fn rect(&self) -> Rect {
        Rect::of_image(self.pos, self.image.size())
    }
}

/// The 18 sprites, their saved backgrounds and the collision lists of the
/// most recent move. See the [module documentation](self) for the
/// operations and their exact step order.
///
/// The layer does not own the picture: every drawing operation takes the
/// [`Framebuffer`] it works on, so both can be fields of one game value.
/// No operation allocates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteLayer {
    /// On the heap: with their inline saved backgrounds the 18 records take
    /// about 18 KB, allocated once here and never again.
    sprites: Box<[Sprite; SLOTS]>,
    collisions: Collisions,
}

impl SpriteLayer {
    /// Every sprite disabled at logical (0, 0) with image 0 current and
    /// pending and no saved background; all collision lists empty
    /// (research 03 §7.2).
    pub fn new() -> Self {
        let Ok(sprites) = vec![Sprite::INITIAL; SLOTS].into_boxed_slice().try_into() else {
            unreachable!("a vector of SLOTS records converts to [Sprite; SLOTS]")
        };
        Self {
            sprites,
            collisions: Collisions::EMPTY,
        }
    }

    fn sprite(&self, id: SpriteId) -> &Sprite {
        match self.sprites.get(id.index()) {
            Some(sprite) => sprite,
            None => unreachable!("a SpriteId is below 18"),
        }
    }

    fn sprite_mut(&mut self, id: SpriteId) -> &mut Sprite {
        match self.sprites.get_mut(id.index()) {
            Some(sprite) => sprite,
            None => unreachable!("a SpriteId is below 18"),
        }
    }

    /// Whether the sprite is enabled, that is, on the picture as far as
    /// redrawing and collisions are concerned.
    pub fn is_enabled(&self, id: SpriteId) -> bool {
        self.sprite(id).enabled
    }

    /// The sprite's recorded position; x is a multiple of 4 once the sprite
    /// has been placed or moved.
    pub fn position(&self, id: SpriteId) -> Px {
        self.sprite(id).pos
    }

    /// The sprite's current image, the one on the picture.
    pub fn image(&self, id: SpriteId) -> ImageId {
        self.sprite(id).image
    }

    /// The image the sprite will show from its next place or move.
    pub fn pending_image(&self, id: SpriteId) -> ImageId {
        self.sprite(id).pending
    }

    /// The sprite's rectangle: its position with the size of its current
    /// image, `4w` logical columns by `h` rows (research 03 §7.2).
    pub fn rect(&self, id: SpriteId) -> Rect {
        self.sprite(id).rect()
    }

    /// The sprite's saved background, if it was ever captured.
    pub fn saved_background(&self, id: SpriteId) -> Option<&SavedRect> {
        self.sprite(id).saved.as_ref()
    }

    /// Whether the rectangles of the two sprites share a logical pixel,
    /// enabled or not; the test the redraw set and the collision lists use
    /// (research 03 §7.3).
    pub fn overlaps(&self, a: SpriteId, b: SpriteId) -> bool {
        self.rect(a).overlaps(self.rect(b))
    }

    /// The collision lists of the most recent move.
    pub const fn collisions(&self) -> Collisions {
        self.collisions
    }

    /// Empties all five collision lists without drawing anything
    /// (research 03 §7.4, the invalid digger frame path).
    pub const fn clear_collisions(&mut self) {
        self.collisions = Collisions::EMPTY;
    }

    /// Sets both the current and the pending image to `image` and disables
    /// the sprite. The picture, the position, the saved background and the
    /// collision lists are untouched (research 03 §7.3).
    pub fn reset(&mut self, id: SpriteId, image: ImageId) {
        // Quirk S6: an enabled sprite is forgotten without being erased; its
        // image stays on the picture.
        let sprite = self.sprite_mut(id);
        sprite.image = image;
        sprite.pending = image;
        sprite.enabled = false;
    }

    /// Sets the image the sprite shows from its next place or move. Nothing
    /// else changes (research 03 §7.3, §13 item 4).
    pub fn set_pending(&mut self, id: SpriteId, image: ImageId) {
        self.sprite_mut(id).pending = image;
    }

    /// The redraw set from `start`: every enabled sprite other than
    /// `start_slot` that overlaps `start`, then every enabled sprite that
    /// overlaps a member, until nothing is added (research 03 §7.3).
    ///
    /// Throughout, `start_slot` is taken to have the rectangle `start`
    /// whatever its recorded rectangle is; when enabled it enters the set by
    /// overlapping a member.
    fn redraw_set(&self, start: Rect, start_slot: Option<SpriteId>) -> SlotSet {
        let rect_of = |id: SpriteId| {
            if Some(id) == start_slot {
                start
            } else {
                self.rect(id)
            }
        };
        let mut set = SlotSet::EMPTY;
        let mut work = SlotSet::EMPTY;
        for id in SpriteId::all() {
            if self.is_enabled(id) && Some(id) != start_slot && rect_of(id).overlaps(start) {
                set.insert(id);
                work.insert(id);
            }
        }
        while let Some(member) = work.pop_first() {
            let area = rect_of(member);
            for id in SpriteId::all() {
                if id != member
                    && !set.contains(id)
                    && self.is_enabled(id)
                    && rect_of(id).overlaps(area)
                {
                    set.insert(id);
                    work.insert(id);
                }
            }
        }
        set
    }

    /// Writes back the saved background of every member, ascending, at its
    /// recorded position; members never captured are skipped.
    fn restore_set(&self, fb: &mut Framebuffer, set: SlotSet) {
        for id in set.iter() {
            let sprite = self.sprite(id);
            if let Some(saved) = &sprite.saved {
                fb.restore(sprite.pos, saved);
            }
        }
    }

    /// Draws every member's current image, masked, ascending.
    fn draw_set(&self, fb: &mut Framebuffer, set: SlotSet) {
        for id in set.iter() {
            let sprite = self.sprite(id);
            fb.blit_masked(sprite.image, sprite.pos);
        }
    }

    /// Captures the background under the sprite's current rectangle.
    fn capture(&mut self, fb: &Framebuffer, id: SpriteId) {
        let sprite = self.sprite_mut(id);
        sprite.saved = Some(fb.capture(sprite.pos, sprite.image.size()));
    }

    /// Places the sprite at `at` (x rounded down to a multiple of 4) with
    /// its pending image, saving the background under it; no collision
    /// lists are computed (research 03 §7.4).
    pub fn place(&mut self, fb: &mut Framebuffer, id: SpriteId, at: Px) {
        let sprite = self.sprite_mut(id);
        sprite.pos = at.aligned();
        sprite.image = sprite.pending;
        let rect = sprite.rect();
        // Quirk S4: an already enabled sprite is not erased at its old
        // position, and if it is in the redraw set its old saved background
        // is restored at the new position before the new capture.
        let set = self.redraw_set(rect, Some(id));
        self.restore_set(fb, set);
        self.capture(fb, id);
        self.sprite_mut(id).enabled = true;
        let mut drawn = set;
        drawn.insert(id);
        self.draw_set(fb, drawn);
    }

    /// Moves the sprite to `at` (x rounded down to a multiple of 4) with its
    /// pending image made current, and returns the collision lists of the
    /// move, which are also stored (research 03 §7.4).
    pub fn move_to(&mut self, fb: &mut Framebuffer, id: SpriteId, at: Px) -> Collisions {
        let sprite = self.sprite(id);
        let new_pos = at.aligned();
        let old_rect = sprite.rect();
        let new_rect = Rect::of_image(new_pos, sprite.pending.size());
        let set = self
            .redraw_set(old_rect, Some(id))
            .union(self.redraw_set(new_rect, Some(id)))
            .union(SlotSet::single(id));
        // Quirk S3: the moved sprite's saved background is restored at its
        // recorded position even when it is disabled (a stale background
        // after an erase); a sprite never captured restores nothing.
        self.restore_set(fb, set);
        let sprite = self.sprite_mut(id);
        sprite.enabled = true;
        sprite.pos = new_pos;
        sprite.image = sprite.pending;
        self.capture(fb, id);
        self.draw_set(fb, set);
        // Quirk S2: only a move produces collision lists, and they persist
        // until the next move.
        self.collisions = self.collisions_of(id);
        self.collisions
    }

    /// The enabled sprites other than `id` whose rectangles overlap `id`'s.
    fn collisions_of(&self, id: SpriteId) -> Collisions {
        let area = self.rect(id);
        let mut set = SlotSet::EMPTY;
        for other in SpriteId::all() {
            if other != id && self.is_enabled(other) && self.rect(other).overlaps(area) {
                set.insert(other);
            }
        }
        Collisions::from_set(set)
    }

    /// Removes an enabled sprite from the picture: restores its saved
    /// background, disables it and redraws the sprites around it. Does
    /// nothing for a disabled sprite (research 03 §7.4).
    pub fn erase(&mut self, fb: &mut Framebuffer, id: SpriteId) {
        let sprite = self.sprite(id);
        if !sprite.enabled {
            return;
        }
        if let Some(saved) = &sprite.saved {
            fb.restore(sprite.pos, saved);
        }
        let rect = sprite.rect();
        self.sprite_mut(id).enabled = false;
        let set = self.redraw_set(rect, Some(id));
        self.draw_set(fb, set);
    }

    /// Draws a static `image` at `at` beneath the sprites, so that it
    /// becomes part of every overlapping sprite's background
    /// (research 03 §7.5).
    ///
    /// The protected area is the image's rectangle at `at.x` as given; the
    /// image is drawn at `at.x` rounded down to a multiple of 4.
    pub fn draw_static(&mut self, fb: &mut Framebuffer, image: ImageId, at: Px) {
        // Quirk S1: the protected area uses the unaligned x, the drawing the
        // aligned one.
        let area = Rect::of_image(at, image.size());
        let set = self.redraw_set(area, None);
        self.restore_set(fb, set);
        fb.blit_masked(image, at.aligned());
        for id in set.iter() {
            self.capture(fb, id);
        }
        self.draw_set(fb, set);
    }

    /// Draws a static `image`, masked, at `at` with x rounded down to a
    /// multiple of 4, without restoring, capturing or redrawing any sprite
    /// (research 03 §7.5, §8.2). For areas no sprite can overlap.
    pub fn draw_static_unprotected(&self, fb: &mut Framebuffer, image: ImageId, at: Px) {
        fb.blit_masked(image, at.aligned());
    }
}

impl Default for SpriteLayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
