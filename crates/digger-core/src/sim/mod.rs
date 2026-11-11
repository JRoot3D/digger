//! The game simulation: one owning game value advanced one frame at a time
//! (design D1–D4).
//!
//! Every system lives in its own module below; the public names are
//! re-exported from the crate root.
//!
//! # No allocation
//!
//! Nothing in the simulation allocates on the heap once the game value
//! exists (design D9). The heap types, methods and macros listed in the
//! workspace's `clippy.toml` are denied here; the only local exceptions are
//! the game's constructor and the test kit. Unit tests are exempt: the
//! deny applies to the library build, which `cargo clippy --all-targets`
//! checks alongside the test build.

#![cfg_attr(
    not(test),
    deny(
        clippy::disallowed_types,
        clippy::disallowed_methods,
        clippy::disallowed_macros
    )
)]

mod bags;
mod bonus;
mod config;
mod digger;
mod draw;
mod field;
mod fireball;
mod gauntlet;
mod geom;
mod gfx;
mod ids;
mod input;
mod level;
mod monster;
mod observer;
mod penalty;
mod rng;
mod score;
mod state;
mod status;
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
mod timing;

pub use config::{ConfigError, GameConfig, GameMode, GauntletSeconds, StartLevel};
pub use field::{Cell, EmeraldMap, Field};
pub use geom::{CellPos, Direction, Located, locate, reverse_of, within_limits};
pub(crate) use ids::index_id;
pub use ids::{BagId, DiggerId, IdOutOfRange, MonsterSlot, PlayerId, SlotId};
pub use input::{Controls, FrameInput, Movement};
pub use level::{CUSTOM_LEVELS_LEN, Difficulty, LevelError, LevelLayout, LevelNumber, LevelSet};
pub use observer::{NoopObserver, Observer};
pub use penalty::{Penalty, PenaltySite};
pub use rng::{GameRng, Modulus, round_seed};
pub use score::{BonusEffect, ScoreEvent, ScoreId, Scores};
pub use state::StateView;
pub use timing::FrameLength;
