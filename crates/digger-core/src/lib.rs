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

#![forbid(unsafe_code)]

pub mod assets;
mod error;
pub mod geom;
pub mod video;

pub use error::{AssetError, Error};

#[cfg(test)]
mod tests {
    #[test]
    fn crate_builds() {
        assert_eq!(env!("CARGO_PKG_NAME"), "digger-core");
    }
}
