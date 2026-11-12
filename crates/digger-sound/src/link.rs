//! The thread boundary between the game and the audio device (design D8).
//!
//! [`sound_link`] splits one engine into two halves:
//!
//! - [`SoundHandle`] stays on the game (frontend) thread. It sends the
//!   simulation's commands and the frontend's controls, reads the engine's
//!   events and the state of the switches, and never blocks.
//! - [`AudioSource`] is moved to the audio thread (the device callback).
//!   It owns the [`Synth`] and fills sample buffers with
//!   [`AudioSource::fill`]; it never allocates, locks or blocks.
//!
//! The halves are joined by two wait-free single-producer single-consumer
//! rings and two atomic flags:
//!
//! - inputs (commands and controls), game to audio, capacity
//!   [`INPUT_CAPACITY`]; an input sent while the ring is full is dropped
//!   and counted in [`SoundStatus::dropped`];
//! - events, audio to game, capacity [`EVENT_CAPACITY`]; an event reported
//!   while the ring is full is dropped, so unread events cost nothing;
//! - the sound and music switches, written by the audio side whenever it
//!   applies inputs, because "switching sound on also switches music on" is
//!   decided there.
//!
//! Every piece of engine state is a field of the one [`Synth`] that the
//! audio thread owns, so nothing is shared between the threads except
//! through these rings and flags (fixed quirk SF8).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rtrb::{Consumer, Producer, RingBuffer};

use crate::command::{Input, SoundCommand, SoundControl, SoundEvent};
use crate::config::{ConfigError, SoundConfig};
use crate::driver::SoundSink;
use crate::music::TuneTables;
use crate::synth::Synth;

/// How many inputs can wait for the audio thread (upstream's queue
/// length).
pub const INPUT_CAPACITY: usize = 4096;

/// How many events can wait for the game thread.
pub const EVENT_CAPACITY: usize = 16;

/// What the frontend can read about the engine without waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SoundStatus {
    /// Whether sound is switched on (not muted).
    pub sound_on: bool,
    /// Whether music is switched on.
    pub music_on: bool,
    /// How many inputs were dropped because the input ring was full.
    pub dropped: u32,
}

/// The switches, published by the audio side for the game side.
#[derive(Debug)]
struct Switches {
    sound_on: AtomicBool,
    music_on: AtomicBool,
}

impl Switches {
    fn new(config: SoundConfig) -> Self {
        Self {
            sound_on: AtomicBool::new(config.sound_on),
            music_on: AtomicBool::new(config.music_on),
        }
    }

    fn publish(&self, synth: &Synth) {
        self.sound_on.store(synth.sound_on(), Ordering::Release);
        self.music_on.store(synth.music_on(), Ordering::Release);
    }
}

/// Creates an engine with the settings of `config`, playing tunes from
/// `tables`, and splits it into its game-side and audio-side halves.
///
/// The tables are copied here, on the calling thread, so the audio thread
/// never allocates. The configuration's fields are validated when they are
/// built, so no current configuration is rejected; the `Result` leaves
/// room for settings that can only be checked together.
pub fn sound_link(
    config: SoundConfig,
    tables: &TuneTables,
) -> Result<(SoundHandle, AudioSource), ConfigError> {
    let (inputs, input_rx) = RingBuffer::new(INPUT_CAPACITY);
    let (event_tx, events) = RingBuffer::new(EVENT_CAPACITY);
    let switches = Arc::new(Switches::new(config));
    let handle = SoundHandle {
        inputs,
        events,
        switches: Arc::clone(&switches),
        dropped: 0,
    };
    let source = AudioSource {
        synth: Synth::new(config, tables.clone()),
        inputs: input_rx,
        events: event_tx,
        switches,
        applied: 0,
    };
    Ok((handle, source))
}

/// The game-side half of a linked engine: sends inputs, reads events and
/// status. Nothing it does blocks.
#[derive(Debug)]
pub struct SoundHandle {
    inputs: Producer<Input>,
    events: Consumer<SoundEvent>,
    switches: Arc<Switches>,
    dropped: u32,
}

impl SoundHandle {
    /// Queues one input for the next tick. If the input ring is full the
    /// input is dropped and counted.
    fn push(&mut self, input: Input) {
        if self.inputs.push(input).is_err() {
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    /// Takes the oldest event the engine has reported and not yet been
    /// read, if any. Events are informational; nothing has to read them.
    pub fn poll_event(&mut self) -> Option<SoundEvent> {
        self.events.pop().ok()
    }

    /// The switches as the audio side last applied them, and the number of
    /// inputs dropped so far.
    pub fn status(&self) -> SoundStatus {
        SoundStatus {
            sound_on: self.switches.sound_on.load(Ordering::Acquire),
            music_on: self.switches.music_on.load(Ordering::Acquire),
            dropped: self.dropped,
        }
    }
}

impl SoundSink for SoundHandle {
    fn send(&mut self, command: SoundCommand) {
        self.push(Input::Game(command));
    }

    fn control(&mut self, control: SoundControl) {
        self.push(Input::Control(control));
    }
}

/// The audio-side half of a linked engine. It owns the [`Synth`] and is
/// moved to the thread that pulls samples.
#[derive(Debug)]
pub struct AudioSource {
    synth: Synth,
    inputs: Consumer<Input>,
    events: Producer<SoundEvent>,
    switches: Arc<Switches>,
    applied: u64,
}

impl AudioSource {
    /// Fills `out` with the next mono samples.
    ///
    /// Before each tick that falls within `out`, every input waiting at
    /// that moment is applied in the order it was sent, the switches are
    /// published and the previous ticks' events are reported. The work per
    /// tick is bounded by the input ring's capacity; nothing allocates,
    /// locks or blocks.
    pub fn fill(&mut self, out: &mut [i16]) {
        let Self {
            synth,
            inputs,
            events,
            switches,
            applied,
        } = self;
        synth.render_with(out, |synth| {
            // Only what is waiting now: inputs pushed while draining wait
            // for the next tick, so a busy sender cannot stall this one.
            let waiting = inputs.slots();
            for _ in 0..waiting {
                let Ok(input) = inputs.pop() else { break };
                synth.apply(input);
                *applied += 1;
            }
            switches.publish(synth);
            report(synth, events);
        });
        report(&mut self.synth, &mut self.events);
    }

    /// The engine this source runs.
    pub fn synth(&self) -> &Synth {
        &self.synth
    }

    /// How many inputs have been taken from the input ring and applied.
    pub const fn applied(&self) -> u64 {
        self.applied
    }
}

/// Moves the engine's pending events to the event ring, dropping those that
/// do not fit.
fn report(synth: &mut Synth, events: &mut Producer<SoundEvent>) {
    for event in synth.take_events() {
        // A full ring means nobody is reading; the event is dropped.
        let _ = events.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::ManualDriver;
    use crate::units::SampleRate;

    #[expect(
        clippy::expect_used,
        reason = "the embedded tables are tested to decode"
    )]
    fn tables() -> TuneTables {
        TuneTables::embedded().expect("embedded tables decode")
    }

    #[expect(clippy::expect_used, reason = "a default link is always created")]
    fn link(config: SoundConfig) -> (SoundHandle, AudioSource) {
        sound_link(config, &tables()).expect("valid configuration")
    }

    fn samples_per_tick() -> usize {
        usize::try_from(SampleRate::DEFAULT.samples_per_tick()).unwrap_or(usize::MAX)
    }

    #[test]
    fn handle_to_source_round_trip() {
        let config = SoundConfig {
            filter: false,
            ..SoundConfig::default()
        };
        let (mut handle, mut source) = link(config);
        let mut driver = ManualDriver::new(config, tables());

        // A gold command sent through the handle sounds exactly as the
        // same command sent to a manual driver: 40 ticks of samples,
        // filled in odd-sized buffers.
        handle.send(SoundCommand::Gold);
        driver.send(SoundCommand::Gold);
        let total = 40 * samples_per_tick();
        let expected = driver.render(total);
        let mut actual = vec![0; total];
        for chunk in actual.chunks_mut(333) {
            source.fill(chunk);
        }
        assert_eq!(actual, expected);
        assert!(expected.iter().any(|&s| s != 0), "gold is audible");
        assert_eq!(source.applied(), 1);
        assert_eq!(handle.status().dropped, 0);
    }

    #[test]
    fn status_reflects_switches() {
        let (mut handle, mut source) = link(SoundConfig::default());
        let tick = samples_per_tick();
        let mut buffer = vec![0; tick];
        let initial = SoundStatus {
            sound_on: true,
            music_on: true,
            dropped: 0,
        };
        assert_eq!(handle.status(), initial);

        // Music off, then sound off: both read as off once a tick has run.
        handle.control(SoundControl::ToggleMusic);
        handle.control(SoundControl::ToggleSound);
        assert_eq!(handle.status(), initial, "nothing changes before a tick");
        source.fill(&mut buffer);
        assert_eq!(
            handle.status(),
            SoundStatus {
                sound_on: false,
                music_on: false,
                dropped: 0
            }
        );

        // Sound on again forces music on, within one tick.
        handle.control(SoundControl::ToggleSound);
        source.fill(&mut buffer);
        assert_eq!(handle.status(), initial);

        // A link created quiet reports that from the start.
        let (quiet, _) = link(SoundConfig {
            sound_on: false,
            ..SoundConfig::default()
        });
        assert!(!quiet.status().sound_on);
        assert!(quiet.status().music_on);
    }
    #[test]
    fn overflow_drops_and_counts() {
        let (mut handle, mut source) = link(SoundConfig::default());
        // 5000 sends while no samples are pulled: none blocks, 4096 are
        // kept and 904 are dropped and counted.
        for _ in 0..5000 {
            handle.send(SoundCommand::EmeraldClick);
        }
        assert_eq!(handle.status().dropped, 904);
        let mut buffer = vec![0; samples_per_tick()];
        source.fill(&mut buffer);
        assert_eq!(source.applied(), 4096);

        // Once drained, the ring accepts inputs again.
        handle.send(SoundCommand::StopAll);
        source.fill(&mut buffer);
        assert_eq!(source.applied(), 4097);
        assert_eq!(handle.status().dropped, 904);
    }

    #[test]
    fn unread_events_do_not_affect_output() {
        let config = SoundConfig {
            filter: false,
            ..SoundConfig::default()
        };
        let (mut handle, mut source) = link(config);
        let mut driver = ManualDriver::new(config, tables());
        let tick = samples_per_tick();

        // Twenty level-done jingles, each cut short by a stop-all after a
        // few ticks, report twenty events that nobody reads: the output
        // matches the manual driver's and the extra events are dropped.
        let mut actual = Vec::new();
        let mut expected = Vec::new();
        for _ in 0..20 {
            for command in [SoundCommand::LevelDone, SoundCommand::StopAll] {
                handle.send(command);
                driver.send(command);
                let mut buffer = vec![0; 3 * tick];
                source.fill(&mut buffer);
                actual.extend_from_slice(&buffer);
                expected.extend(driver.render(3 * tick));
            }
        }
        assert_eq!(actual, expected);
        assert_eq!(driver.events().len(), 20);

        let mut read = 0;
        while let Some(event) = handle.poll_event() {
            assert_eq!(event, SoundEvent::LevelDoneFinished);
            read += 1;
        }
        assert_eq!(read, EVENT_CAPACITY);

        // Once the ring has been read, new events are delivered again: a
        // jingle cut short within one tick reports its finish once.
        handle.send(SoundCommand::LevelDone);
        handle.send(SoundCommand::StopAll);
        let mut buffer = vec![0; tick];
        source.fill(&mut buffer);
        assert_eq!(handle.poll_event(), Some(SoundEvent::LevelDoneFinished));
        assert_eq!(handle.poll_event(), None);
    }
}
