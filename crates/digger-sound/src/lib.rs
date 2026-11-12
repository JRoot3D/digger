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
//! - [`music`]: the tune tables, decoded from the embedded asset into
//!   [`TuneTables`], the per-tune [`TuneParams`] and note envelope, and
//!   the [`MusicPlayer`].
//! - [`jingle`]: the level-done jingle, [`LevelDone`].
//! - [`units`]: the newtypes the engine counts in.
//! - [`voice`]: the single [`Voice`] chosen on each tick, the rules that
//!   decide whether it is audible, and the [`TickTrace`] a tick reports.
//! - [`noise`]: the engine's own [`NoiseRng`].
//! - [`effects`]: the effect slots and the order in which they are heard.
//! - [`synth`]: the pure core, [`Synth`], which applies inputs and ticks.
//! - [`driver`]: [`ManualDriver`], which runs the engine without a device,
//!   and the [`SoundSink`] sending interface.

#![forbid(unsafe_code)]

pub mod command;
pub mod config;
pub mod driver;
pub mod effects;
pub mod jingle;
pub mod music;
pub mod noise;
pub mod synth;
pub mod units;
pub mod voice;

pub use command::{
    EmeraldStep, FireballId, Input, SoundCommand, SoundControl, SoundEvent, Tempo, Tune,
};
pub use config::{ConfigError, SoundConfig};
pub use driver::{ManualDriver, SoundSink};
pub use jingle::LevelDone;
pub use music::{MusicPlayer, Note, TableError, TuneParams, TuneTables};
pub use noise::NoiseRng;
pub use synth::Synth;
pub use units::{Divisor, Hertz, Level, SampleRate, SoundTicks, Volume};
pub use voice::{TickTrace, Tone, Voice};

#[cfg(test)]
mod tests {
    #[test]
    fn crate_builds() {
        assert_eq!(env!("CARGO_PKG_NAME"), "digger-sound");
    }
}
