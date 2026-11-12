//! Music: the tune tables (design D11).
//!
//! - [`tables`]: decoding of the embedded `assets/music.bin` into
//!   [`TuneTables`].

pub mod tables;

pub use tables::{Note, TableError, TuneTables};
