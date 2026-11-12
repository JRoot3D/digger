//! Drivers: ways to feed a [`Synth`] without an audio device (design D10).
//!
//! - [`ManualDriver`] runs the engine tick by tick on the calling thread.
//!   Tests and tools use it to check timing on [`TickTrace`]s.
//! - [`SoundSink`] is the sending interface shared by every driver, so a
//!   frontend's frame loop is written once.

use crate::command::{Input, SoundCommand, SoundControl, SoundEvent};
use crate::config::SoundConfig;
use crate::music::TuneTables;
use crate::synth::Synth;
use crate::voice::TickTrace;

/// Something the simulation's sound commands and the frontend's controls
/// can be sent to.
pub trait SoundSink {
    /// Sends a command from the simulation.
    fn send(&mut self, command: SoundCommand);
    /// Sends a control from the frontend.
    fn control(&mut self, control: SoundControl);
}

/// A driver that advances the engine only when asked, with no queue and no
/// thread.
///
/// Commands and controls sent between two ticks are kept in order and
/// applied at the start of the next tick, as a real driver does.
#[derive(Debug, Clone)]
pub struct ManualDriver {
    synth: Synth,
    pending: Vec<Input>,
    events: Vec<SoundEvent>,
}

impl ManualDriver {
    /// A driver for a new engine with the settings of `config`, playing
    /// tunes from `tables`.
    pub fn new(config: SoundConfig, tables: TuneTables) -> Self {
        Self {
            synth: Synth::new(config, tables),
            pending: Vec::new(),
            events: Vec::new(),
        }
    }

    /// The engine this driver runs.
    pub fn synth(&self) -> &Synth {
        &self.synth
    }

    /// Runs `n` ticks and returns what each one produced.
    ///
    /// Inputs sent before the call are applied, in order, at the start of
    /// the first of these ticks.
    pub fn advance_ticks(&mut self, n: usize) -> Vec<TickTrace> {
        (0..n).map(|_| self.tick()).collect()
    }

    /// Renders the next `n` samples, ticking before samples N-1, 2N-1, ...
    /// of the stream (N samples per tick).
    ///
    /// Inputs sent before the call are applied, in order, immediately
    /// before the first tick that falls within these samples. Ticks run by
    /// [`ManualDriver::advance_ticks`] do not move the sample position.
    pub fn render(&mut self, n: usize) -> Vec<i16> {
        let mut out = vec![0; n];
        self.render_into(&mut out);
        out
    }

    /// Fills `out` with the next samples, as [`ManualDriver::render`].
    pub fn render_into(&mut self, out: &mut [i16]) {
        let pending = &mut self.pending;
        let events = &mut self.events;
        self.synth.render_with(out, |synth| {
            for input in pending.drain(..) {
                synth.apply(input);
            }
            events.extend(synth.take_events());
        });
        self.events.extend(self.synth.take_events());
    }

    /// Takes the events reported since the last call, oldest first.
    pub fn events(&mut self) -> Vec<SoundEvent> {
        core::mem::take(&mut self.events)
    }

    /// One tick: apply the pending inputs, advance, collect events.
    fn tick(&mut self) -> TickTrace {
        for input in self.pending.drain(..) {
            self.synth.apply(input);
        }
        let trace = self.synth.tick();
        self.events.extend(self.synth.take_events());
        trace
    }
}

impl SoundSink for ManualDriver {
    fn send(&mut self, command: SoundCommand) {
        self.pending.push(Input::Game(command));
    }

    fn control(&mut self, control: SoundControl) {
        self.pending.push(Input::Control(control));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Divisor;
    use crate::voice::Voice;

    #[expect(
        clippy::expect_used,
        reason = "the embedded tables are tested to decode"
    )]
    fn driver() -> ManualDriver {
        let tables = TuneTables::embedded().expect("embedded tables decode");
        ManualDriver::new(SoundConfig::default(), tables)
    }

    #[test]
    fn commands_apply_in_order_at_next_tick() {
        let mut driver = driver();

        // Start then stop before one tick: nothing is left running.
        driver.send(SoundCommand::FallStart);
        driver.send(SoundCommand::FallStop);
        assert!(
            driver
                .advance_ticks(5)
                .iter()
                .all(|t| t.effect_offer.is_none() && t.voice == Voice::Silent)
        );

        // Stop then start: the start, sent last, wins, and the fall sounds
        // on the third tick of its cycle.
        driver.send(SoundCommand::FallStop);
        driver.send(SoundCommand::FallStart);
        let offers: Vec<_> = driver
            .advance_ticks(3)
            .iter()
            .map(|t| t.effect_offer)
            .collect();
        assert_eq!(offers, [None, None, Some(Divisor(1000))]);
    }

    #[test]
    fn commands_wait_for_the_next_tick() {
        let mut driver = driver();
        assert_eq!(driver.advance_ticks(1)[..], [silent()]);
        driver.send(SoundCommand::EmeraldClick);
        // Nothing has ticked yet, so nothing sounds until the next tick,
        // which applies the command before advancing.
        let traces = driver.advance_ticks(1);
        assert_eq!(
            traces.first().map(|t| t.voice),
            Some(Voice::Effect(Divisor(1000)))
        );
        driver.send(SoundCommand::StopAll);
        assert_eq!(driver.advance_ticks(1)[..], [silent()]);
    }

    #[test]
    fn controls_are_accepted() {
        let mut driver = driver();
        let sink: &mut dyn SoundSink = &mut driver;
        sink.control(SoundControl::ToggleMusic);
        sink.send(SoundCommand::FallStart);
        let traces = driver.advance_ticks(1);
        assert_eq!(traces.len(), 1);
        assert!(!driver.synth().music_on());
        assert_eq!(driver.events(), []);
    }

    fn silent() -> TickTrace {
        TickTrace {
            voice: Voice::Silent,
            output: Voice::Silent,
            effect_offer: None,
            music_level: crate::synth::INITIAL_MUSIC_LEVEL,
        }
    }
}
