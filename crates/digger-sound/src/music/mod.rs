//! Music: the tune tables, the per-tune parameters and envelope, and the
//! tune player (design D4, D11).
//!
//! - [`tables`]: decoding of the embedded `assets/music.bin` into
//!   [`TuneTables`].
//! - [`envelope`]: [`TuneParams`] per tune and the note envelope.
//! - [`player`]: [`MusicPlayer`], which plays one tune at a time.

pub mod envelope;
pub mod player;
pub mod tables;

pub use envelope::{EnvelopeStage, GateRule, NoteTiming, TuneParams, envelope_step, params};
pub use player::{INITIAL_MUSIC_LEVEL, MusicPlayer, MusicTick};
pub use tables::{Note, TableError, TuneTables};
