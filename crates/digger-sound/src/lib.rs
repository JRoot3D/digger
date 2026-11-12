//! Digger sound effects and music.
//!
//! This crate holds the sound-effect and music state machines and the sample
//! synthesis. It is pure: sound commands go in and PCM samples come out. It
//! opens no audio device and never feeds back into the simulation.
//!
//! - [`command`]: the [`SoundCommand`]s the simulation emits (re-exported
//!   from `digger-core`), the frontend's [`SoundControl`]s and the
//!   engine's [`SoundEvent`]s.
//! - [`config`]: the [`SoundConfig`] an engine is created with.
//! - [`units`]: the newtypes the engine counts in.

#![forbid(unsafe_code)]

pub mod command;
pub mod config;
pub mod units;

pub use command::{EmeraldStep, FireballId, SoundCommand, SoundControl, SoundEvent, Tempo, Tune};
pub use config::{ConfigError, SoundConfig};
pub use units::{Divisor, Hertz, Level, SampleRate, SoundTicks, Volume};

#[cfg(test)]
mod tests {
    #[test]
    fn crate_builds() {
        assert_eq!(env!("CARGO_PKG_NAME"), "digger-sound");
    }
}
