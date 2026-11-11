//! The deterministic Digger simulation.
//!
//! This crate owns the game state, the pixel-exact indexed framebuffer, the
//! sprite layer, levels, scoring and the recording codecs.
//!
//! **No-I/O rule:** `digger-core` performs no I/O and depends on no clock,
//! threads or platform crates. It never touches files, windows, audio
//! devices or the system time. Given the same configuration, seeds and
//! inputs it produces the same results on every platform; the frontend owns
//! time and everything that talks to the outside world.
//!
//! # Rendering and sprite layer
//!
//! The picture is game state: a [`Framebuffer`](video::Framebuffer) of
//! 640x400 palette indices addressed in 320x200 logical pixels
//! ([`Px`](geom::Px)). The original game finds walls by reading pixels back
//! (the [`probe`](video::Framebuffer::probe)), so the picture is drawn
//! exactly whether or not anyone looks at it. The art comes from the
//! embedded [`assets`]: 120 images named by [`ImageId`](assets::ImageId),
//! the font, and the title picture.
//!
//! Moving objects go through the [`SpriteLayer`](sprite::SpriteLayer): 18
//! slots in five classes, drawn in slot order over saved backgrounds. A
//! [`move_to`](sprite::SpriteLayer::move_to) returns the
//! [`Collisions`](sprite::Collisions) of that move, the game's only source
//! of collisions. The layer does not own the picture; each drawing
//! operation takes the framebuffer, so both can be fields of one game value.
//! See the [`video`] and [`sprite`] modules for the rules, and
//! `docs/compat-quirks.md` for the reproduced quirks of the original.
//!
//! ```
//! use digger_core::assets::ImageId;
//! use digger_core::geom::Px;
//! use digger_core::sprite::{SpriteClass, SpriteId, SpriteLayer};
//! use digger_core::video::{Framebuffer, LevelPlan, ProbeBits};
//!
//! let mut fb = Framebuffer::new();
//! fb.draw_background(LevelPlan::new(1).unwrap());
//! let mut layer = SpriteLayer::new();
//!
//! // Place a bag and a monster, then move the digger onto both.
//! let bag = SpriteId::bag(0).unwrap();
//! layer.reset(bag, ImageId::BAG_STILL);
//! layer.place(&mut fb, bag, Px::new(100, 100));
//! let nobbin = SpriteId::monster(0).unwrap();
//! layer.reset(nobbin, ImageId::NOBBIN_1);
//! layer.place(&mut fb, nobbin, Px::new(112, 100));
//!
//! let digger = SpriteId::digger(0).unwrap();
//! layer.reset(digger, ImageId::new(1).unwrap());
//! layer.place(&mut fb, digger, Px::new(40, 100));
//! let hits = layer.move_to(&mut fb, digger, Px::new(106, 100)); // x becomes 104
//! assert_eq!(layer.position(digger), Px::new(104, 100));
//! assert_eq!(hits.first(SpriteClass::Bag), Some(bag));
//! assert_eq!(hits.of(SpriteClass::Monster).collect::<Vec<_>>(), [nobbin]);
//! assert!(hits.first(SpriteClass::Fireball).is_none());
//!
//! // The probe sees the background's earth, and the black top line as empty.
//! assert!(!fb.probe(Px::new(200, 150)).is_empty());
//! assert!(fb.probe(Px::new(200, 2)).is_empty());
//! assert_eq!(fb.probe(Px::new(320, 0)), ProbeBits::OUTSIDE);
//! ```

#![forbid(unsafe_code)]

pub mod assets;
mod error;
pub mod geom;
mod sim;
mod sound_cmd;
pub mod sprite;
pub mod video;

#[cfg(any(test, feature = "testkit"))]
pub use sim::testkit;
pub use sim::{
    BagId, CUSTOM_LEVELS_LEN, CellPos, ConfigError, Difficulty, DiggerId, Direction, FrameLength,
    GameConfig, GameMode, GameRng, GauntletSeconds, IdOutOfRange, LevelError, LevelLayout,
    LevelNumber, LevelSet, Located, Modulus, MonsterSlot, Penalty, PenaltySite, PlayerId, SlotId,
    StartLevel, locate, reverse_of, round_seed, within_limits,
};
pub use sound_cmd::FireballId;

pub use error::{AssetError, Error};

#[cfg(test)]
mod tests {
    #[test]
    fn crate_builds() {
        assert_eq!(env!("CARGO_PKG_NAME"), "digger-core");
    }
}
