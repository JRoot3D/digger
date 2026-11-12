//! Digger sound effects and music.
//!
//! This crate holds the sound-effect and music state machines and the sample
//! synthesis. It is pure: sound commands go in and mono 16-bit PCM samples
//! come out. It opens no audio device, reads no clock and never feeds back
//! into the simulation.
//!
//! # Data flow
//!
//! ```text
//!   game thread                         |  audio thread (device callback)
//!                                       |
//!   Game::step ── drain_sound() ─┐      |
//!                                ├─> SoundHandle ══ input ring (4096) ══> AudioSource::fill
//!   frontend keys/settings ──────┘      |                                   │ before each tick:
//!     (SoundControl)                    |                                   │  apply all inputs
//!                                       |                                   │  Synth::tick
//!   SoundHandle::poll_event <══ event ring (16) ═══════════════════════════ │  report events
//!   SoundHandle::status     <── sound/music flags (atomics) ─────────────── │  publish switches
//!                                       |                                   ▼
//!                                       |                       i16 samples ──> device
//! ```
//!
//! [`sound_link`] creates one engine and splits it into a [`SoundHandle`],
//! which stays on the game thread and never blocks, and an [`AudioSource`],
//! which is moved to the audio thread and owns the [`Synth`]. A frame loop
//! sends every command the simulation produced in that frame and the
//! frontend's controls through the [`SoundSink`] trait, which
//! [`SoundHandle`], [`ManualDriver`] and [`NullDriver`] all implement, so
//! it is written once.
//!
//! # Drivers
//!
//! - [`ManualDriver`] runs the engine on the calling thread, tick by tick
//!   or sample by sample. Tests and tools use it.
//! - [`NullDriver`] accepts every command and control and discards it.
//!   A frontend uses it when no audio device can be opened.
//! - [`sound_link`] is the real-time pair for an audio device.
//!
//! A frontend that runs the simulation faster than real time (fast-forward,
//! replays at maximum speed) sends the commands of those frames to a
//! [`NullDriver`] rather than to its [`SoundHandle`], because the handle's
//! input ring would fill and drop commands, possibly a stop that leaves an
//! effect running. When it returns to real time it sends
//! [`SoundCommand::StopAll`] to the handle, so that nothing started before
//! the fast-forward keeps sounding.
//!
//! # Events are informational
//!
//! The engine reports [`SoundEvent::LevelDoneFinished`] and
//! [`SoundEvent::DirgeFinished`]. They are for presentation only: the
//! simulation waits fixed frame counts of its own and has no input through
//! which an event could reach it. Nothing has to read them; unread events
//! are dropped once 16 are waiting, and never affect the output.
//!
//! # Example
//!
//! ```
//! use digger_sound::{ManualDriver, SoundCommand, SoundConfig, SoundSink, TuneTables};
//!
//! let tables = TuneTables::embedded().expect("embedded tables decode");
//! let config = SoundConfig { filter: false, ..SoundConfig::default() };
//! let mut driver = ManualDriver::new(config, tables);
//!
//! // One frame's commands, then one tick's worth of samples (606 at
//! // 44100 Hz): the gold effect starts on the tick before the last sample.
//! driver.send(SoundCommand::Gold);
//! let samples = driver.render(606);
//! assert!(samples[..605].iter().all(|&s| s == 0));
//! assert_ne!(samples[605], 0);
//!
//! // A level-done jingle cut short by a stop-all reports its finish.
//! driver.send(SoundCommand::LevelDone);
//! driver.advance_ticks(10);
//! driver.send(SoundCommand::StopAll);
//! driver.advance_ticks(1);
//! assert_eq!(driver.events(), [digger_sound::SoundEvent::LevelDoneFinished]);
//! ```
//!
//! # Modules
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
//! - [`osc`]: the square-wave generator, [`SquareOsc`], and the jingle's
//!   [`Gate`].
//! - [`filter`]: the optional [`OutputFilter`] (high-pass 1 kHz, then
//!   low-pass 4 kHz).
//! - [`script`]: the text [`Script`]s that list commands at sample
//!   offsets, rendered by the golden tests and `cargo xtask render-sound`.
//! - [`synth`]: the pure core, [`Synth`], which applies inputs, ticks and
//!   renders samples.
//! - [`driver`]: [`ManualDriver`], [`NullDriver`] and the [`SoundSink`]
//!   sending interface.
//! - [`link`]: the thread boundary, [`sound_link`], [`SoundHandle`] and
//!   [`AudioSource`].

#![forbid(unsafe_code)]

pub mod command;
pub mod config;
pub mod driver;
pub mod effects;
pub mod filter;
pub mod jingle;
pub mod link;
pub mod music;
pub mod noise;
pub mod osc;
pub mod script;
pub mod synth;
pub mod units;
pub mod voice;

pub use command::{
    EmeraldStep, FireballId, Input, SoundCommand, SoundControl, SoundEvent, Tempo, Tune,
};
pub use config::{ConfigError, SoundConfig};
pub use driver::{ManualDriver, NullDriver, SoundSink};
pub use filter::{FirstOrder, OutputFilter};
pub use jingle::LevelDone;
pub use link::{AudioSource, EVENT_CAPACITY, INPUT_CAPACITY, SoundHandle, SoundStatus, sound_link};
pub use music::{MusicPlayer, Note, TableError, TuneParams, TuneTables};
pub use noise::NoiseRng;
pub use osc::{Gate, SquareOsc};
pub use script::{Script, ScriptEntry, ScriptError};
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
