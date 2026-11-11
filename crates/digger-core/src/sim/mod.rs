//! The game simulation: one owning game value advanced one frame at a time
//! (design D1–D4).
//!
//! Every system lives in its own module below; the public names are
//! re-exported from the crate root.

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

pub use geom::{CellPos, Direction, Located, locate, reverse_of, within_limits};
pub(crate) use ids::index_id;
pub use ids::{BagId, DiggerId, IdOutOfRange, MonsterSlot, PlayerId, SlotId};
