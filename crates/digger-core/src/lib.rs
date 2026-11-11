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
//! # The stepping interface
//!
//! A game is one owning [`Game`] value, created from a [`GameConfig`] and a
//! base seed and advanced by its caller exactly one frame per
//! [`step`](Game::step). Each step takes that frame's [`FrameInput`]: the
//! five controls of each of the two control slots (a control counts as on
//! if it is held now or was pressed at any time since the previous frame)
//! and a pause and a quit request. It returns a [`StepOutcome`] saying
//! whether the frame was a [tick](FrameKind::Tick) — the simulation ran
//! once and the tick count rose by 1 — or a [waiting](FrameKind::Wait) or
//! [paused](FrameKind::Paused) frame, and carrying a [`GameSummary`] from
//! the frame the game finishes on.
//!
//! After each step the caller drains the frame's [`SoundCommand`]s with
//! [`drain_sound`](Game::drain_sound) and shows the
//! [`framebuffer`](Game::framebuffer). [`state`](Game::state) gives a
//! read-only [`StateView`] of the simulation, and
//! [`step_observed`](Game::step_observed) lets an [`Observer`] see and
//! replace round seeds, layouts, controls and movement samples, which is
//! what recording and playback are built on.
//!
//! The caller owns time: it decides when to step, for example every
//! [`FrameLength`] at the chosen speed, or as fast as possible for replays
//! and tests. The results never depend on it.
//!
//! ```
//! use digger_core::video::Framebuffer;
//! use digger_core::{FinishReason, FrameInput, Game, GameConfig, SoundCommand};
//!
//! fn present(_picture: &Framebuffer) {}
//! fn play(_command: SoundCommand) {}
//!
//! let config = GameConfig::default(); // one player, level 1
//! let base_seed = 2026;
//! let mut input = FrameInput::NONE;
//! let mut frames = 0;
//!
//! let mut game = Game::new(config, base_seed);
//! let summary = loop {
//!     input.slots[0].right = frames % 50 < 25;
//!     input.quit = frames == 400; // a frontend sets this from a key
//!     let outcome = game.step(&input); // exactly one frame
//!     for cmd in game.drain_sound() {
//!         play(cmd); // sound commands produced this frame
//!     }
//!     present(game.framebuffer()); // 640x400 indices + palette
//!     frames += 1;
//!     if let Some(summary) = outcome.finished {
//!         break summary;
//!     }
//! };
//! assert_eq!(summary.reason, FinishReason::Quit);
//! assert_eq!(summary.ticks, game.state().ticks());
//! ```
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
    BagId, BonusEffect, CUSTOM_LEVELS_LEN, Cell, CellPos, ConfigError, Controls, DeathStage,
    Difficulty, Digger, DiggerId, Direction, EmeraldMap, Field, FinishReason, FrameInput,
    FrameKind, FrameLength, Game, GameConfig, GameMode, GameRng, GameSummary, GauntletSeconds,
    IdOutOfRange, LevelError, LevelLayout, LevelNumber, LevelSet, Located, Modulus, MonsterSlot,
    Movement, NoopObserver, Observer, Penalty, PenaltySite, PlayerId, PlayerResult, ScoreEvent,
    ScoreId, ScoreSummary, Scores, SlotId, StartLevel, StateView, StepOutcome, locate, reverse_of,
    round_seed, within_limits,
};
pub use sound_cmd::{EmeraldStep, FireballId, SoundCommand, Tempo, Tune};

pub use error::{AssetError, Error};

#[cfg(test)]
mod tests {
    #[test]
    fn crate_builds() {
        assert_eq!(env!("CARGO_PKG_NAME"), "digger-core");
    }
}
