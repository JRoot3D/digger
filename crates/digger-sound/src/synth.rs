//! The pure sound core: inputs in, one voice per tick, samples out (design
//! D1, D5, D6, D7).
//!
//! [`Synth`] owns every piece of sound state. It has no queue and no
//! thread: a driver applies the inputs that arrived since the last tick
//! with [`Synth::apply`], in the order they were sent, and then calls
//! [`Synth::tick`], or lets [`Synth::render_with`] do both at the right
//! sample positions. Nothing here allocates or fails.
//!
//! # Ticks within the sample stream
//!
//! A tick lasts [`SampleRate::samples_per_tick`] samples (606 at 44100 Hz)
//! and is counted from the sample stream only. The first tick runs
//! immediately before output sample N-1, the next before 2N-1, and so on.
//! [`Synth::render`] produces whole runs of samples between two ticks, so
//! splitting a render into chunks of any size gives the same samples.
//!
//! # Output stage
//!
//! Each tick turns its [`TickTrace::output`] voice into a [`Tone`] with
//! [`Voice::tone`] (divisor gate, music clamp, level 1 silent, Nyquist
//! guard). Each sample is then produced in this order:
//!
//! 1. **Oscillator.** An effect tone is played by the effect oscillator, a
//!    music tone by the music oscillator, and the jingle's carrier by the
//!    music oscillator switched on and off by the jingle gate ([`osc`]).
//!    The raw sample is `+peak`, `-peak` or, when nothing sounds or the
//!    gate is off, 0. An oscillator whose tone is not sounding on a tick is
//!    reset to phase 0; one that keeps sounding keeps its phase across
//!    pitch changes.
//! 2. **Filter**, when it is on: high-pass 1000 Hz, then low-pass 4000 Hz,
//!    rounded to the nearest integer and limited to `i16` ([`filter`]).
//!    Its state is reset when it is switched on.
//! 3. **Master volume**, last: `sample * volume / 100`, truncated toward
//!    zero ([`apply_volume`]).
//!
//! [`osc`]: crate::osc
//! [`filter`]: crate::filter

use crate::command::{Input, SoundCommand, SoundControl, SoundEvent, Tune};
use crate::config::SoundConfig;
use crate::effects::{Effects, EffectsTick};
use crate::filter::OutputFilter;
use crate::jingle::LevelDone;
use crate::music::{MusicPlayer, MusicTick, TuneTables};
use crate::osc::{Gate, SquareOsc};
use crate::units::{Divisor, Hertz, Level, SampleRate, Volume};
use crate::voice::{TickTrace, Tone, Voice};

pub use crate::music::INITIAL_MUSIC_LEVEL;

/// Events produced by ticks and not yet collected by the driver.
///
/// Each event can occur at most once per tick, so a flag per kind is
/// enough and nothing is allocated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct PendingEvents {
    level_done: bool,
    dirge: bool,
}

/// What the oscillators play until the next tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Sound {
    /// Nothing: every sample is 0.
    Silent,
    /// An effect tone, on the effect oscillator.
    Effect(Tone),
    /// A music tone, on the music oscillator.
    Music(Tone),
    /// The jingle's carrier on the music oscillator, gated by the beat.
    Jingle {
        /// The carrier tone.
        carrier: Tone,
        /// The beat frequency of the gate; 0 Hz is ungated.
        beat: Hertz,
    },
}

impl Sound {
    /// What `voice` sounds like at `rate`.
    fn of(voice: Voice, rate: SampleRate) -> Self {
        match (voice, voice.tone(rate)) {
            (_, None) | (Voice::Silent, _) => Self::Silent,
            (Voice::Effect(_), Some(tone)) => Self::Effect(tone),
            (Voice::Music { .. }, Some(tone)) => Self::Music(tone),
            (Voice::Jingle { beat, .. }, Some(carrier)) => Self::Jingle { carrier, beat },
        }
    }
}

/// The sample generator: oscillators, filter and the position of the next
/// tick in the sample stream.
#[derive(Debug, Clone)]
struct Output {
    rate: SampleRate,
    /// Samples to produce before the next tick runs.
    until_tick: u32,
    sound: Sound,
    effect_osc: SquareOsc,
    music_osc: SquareOsc,
    gate: Gate,
    filter: OutputFilter,
    filter_on: bool,
}

impl Output {
    fn new(config: SoundConfig) -> Self {
        let rate = config.sample_rate;
        Self {
            rate,
            // The first tick runs before sample N-1.
            until_tick: rate.samples_per_tick().saturating_sub(1),
            sound: Sound::Silent,
            effect_osc: SquareOsc::new(rate),
            music_osc: SquareOsc::new(rate),
            gate: Gate::new(rate),
            filter: OutputFilter::new(rate),
            filter_on: config.filter,
        }
    }

    /// Switches the filter; switching it on clears its state.
    const fn set_filter(&mut self, on: bool) {
        if on && !self.filter_on {
            self.filter.reset();
        }
        self.filter_on = on;
    }

    /// Takes `voice` as what sounds until the next tick, resetting every
    /// oscillator it does not use.
    fn set_voice(&mut self, voice: Voice) {
        self.sound = Sound::of(voice, self.rate);
        if !matches!(self.sound, Sound::Effect(_)) {
            self.effect_osc.reset();
        }
        if !matches!(self.sound, Sound::Music(_) | Sound::Jingle { .. }) {
            self.music_osc.reset();
        }
        if !matches!(self.sound, Sound::Jingle { .. }) {
            self.gate.reset();
        }
    }

    /// The next raw sample from the oscillators.
    fn next_raw(&mut self) -> i16 {
        match self.sound {
            Sound::Silent => 0,
            Sound::Effect(tone) => self.effect_osc.next_sample(tone),
            Sound::Music(tone) => self.music_osc.next_sample(tone),
            Sound::Jingle { carrier, beat } => {
                let sample = self.music_osc.next_sample(carrier);
                if self.gate.next_on(beat) { sample } else { 0 }
            }
        }
    }

    /// The next output sample: oscillator, then filter, then volume.
    fn next_sample(&mut self, volume: Volume) -> i16 {
        let raw = self.next_raw();
        let filtered = if self.filter_on {
            self.filter.process(raw)
        } else {
            raw
        };
        apply_volume(filtered, volume)
    }
}

/// The sound engine's state, its tick and its sample output.
#[derive(Debug, Clone)]
pub struct Synth {
    config: SoundConfig,
    tables: TuneTables,
    effects: Effects,
    music: MusicPlayer,
    /// The music switch. Tunes other than the dirge advance and sound only
    /// while it is on (design D4, fixed quirk SF5).
    music_on: bool,
    /// The sound switch. While it is off the output is silent but every
    /// effect, tune and the jingle advance as usual (fixed quirks SF3,
    /// SF4).
    sound_on: bool,
    /// The pause set by the simulation's pause and resume commands. While
    /// it is set nothing advances and nothing sounds. It is separate from
    /// the jingle's freeze of effects and music.
    paused: bool,
    /// The master volume, applied last to every output sample (fixed
    /// quirk SF2).
    volume: Volume,
    /// The level-done jingle, while it plays.
    jingle: Option<LevelDone>,
    events: PendingEvents,
    output: Output,
}

impl Synth {
    /// A silent engine with the settings of `config`, playing tunes from
    /// `tables`.
    pub fn new(config: SoundConfig, tables: TuneTables) -> Self {
        Self {
            config,
            tables,
            effects: Effects::new(),
            music: MusicPlayer::new(),
            music_on: config.music_on,
            sound_on: config.sound_on,
            paused: false,
            volume: config.volume,
            jingle: None,
            events: PendingEvents::default(),
            output: Output::new(config),
        }
    }

    /// The configuration the engine was created with.
    pub fn config(&self) -> &SoundConfig {
        &self.config
    }

    /// The tune tables the engine plays from.
    pub fn tables(&self) -> &TuneTables {
        &self.tables
    }

    /// Whether sound is switched on (not muted).
    pub const fn sound_on(&self) -> bool {
        self.sound_on
    }

    /// Whether music is switched on.
    pub const fn music_on(&self) -> bool {
        self.music_on
    }

    /// Whether the simulation has paused the sound.
    pub const fn paused(&self) -> bool {
        self.paused
    }

    /// The master volume.
    pub const fn volume(&self) -> Volume {
        self.volume
    }

    /// Whether the output filter is on.
    pub const fn filter_on(&self) -> bool {
        self.output.filter_on
    }

    /// Applies one command or control. It takes effect on the next
    /// [`Synth::tick`], before anything advances on that tick.
    ///
    /// Commands are applied while paused too, so a stop-all during a pause
    /// leaves nothing to resume.
    pub fn apply(&mut self, input: Input) {
        match input {
            Input::Game(command) => self.apply_command(command),
            Input::Control(control) => self.apply_control(control),
        }
    }

    /// Applies one control from the frontend.
    fn apply_control(&mut self, control: SoundControl) {
        match control {
            SoundControl::ToggleSound => {
                self.sound_on = !self.sound_on;
                // Switching sound on also switches music on (kept quirk
                // SQ10).
                if self.sound_on {
                    self.music_on = true;
                }
            }
            SoundControl::ToggleMusic => self.music_on = !self.music_on,
            SoundControl::SetVolume(volume) => self.volume = volume,
            SoundControl::SetFilter(on) => self.output.set_filter(on),
        }
    }

    /// Applies one command from the simulation.
    fn apply_command(&mut self, command: SoundCommand) {
        match command {
            SoundCommand::StopAll => {
                self.effects.stop_all();
                self.music.stop();
                if self.jingle.take().is_some() {
                    self.events.level_done = true;
                }
            }
            SoundCommand::Music { tune, tempo } => {
                self.music.play(tune, tempo);
                if tune == Tune::Dirge {
                    self.effects.stop_death();
                }
            }
            SoundCommand::MusicOff => self.music.stop(),
            // Pausing when paused, or resuming when not, changes nothing.
            SoundCommand::Pause => self.paused = true,
            SoundCommand::Resume => self.paused = false,
            // A new jingle replaces one that is still playing.
            SoundCommand::LevelDone => self.jingle = Some(LevelDone::new()),
            command => self.effects.apply(command),
        }
    }

    /// Advances the engine by one tick and reports what sounds on it.
    ///
    /// While the level-done jingle plays, it alone advances and sounds:
    /// effects and music are frozen, and the music level is set to 50 (kept
    /// quirk SQ6). Otherwise the tune advances (while music is on, or
    /// always for the dirge), then the effects; any effect offer displaces
    /// the music for this tick, and the death effect's first tick stops
    /// the tune.
    ///
    /// While paused, nothing advances and the tick is silent. While sound
    /// is off, everything advances as usual and the trace's
    /// [`TickTrace::voice`] shows what would sound, but its
    /// [`TickTrace::output`] is silent.
    ///
    /// The trace's output voice is also what [`Synth::render`] plays until
    /// the next tick.
    pub fn tick(&mut self) -> TickTrace {
        let trace = if self.paused {
            TickTrace {
                voice: Voice::Silent,
                output: Voice::Silent,
                effect_offer: None,
                music_level: self.music.level(),
            }
        } else {
            let (voice, effect_offer) = self.advance();
            TickTrace {
                voice,
                output: if self.sound_on { voice } else { Voice::Silent },
                effect_offer,
                music_level: self.music.level(),
            }
        };
        self.output.set_voice(trace.output);
        trace
    }

    /// Fills `out` with the next samples, running [`Synth::tick`] before
    /// samples N-1, 2N-1, ... of the stream (N samples per tick), with no
    /// inputs applied in between.
    pub fn render(&mut self, out: &mut [i16]) {
        self.render_with(out, |_| {});
    }

    /// Fills `out` with the next samples, as [`Synth::render`], calling
    /// `before_tick` immediately before each tick so that a driver can
    /// apply the inputs that arrived since the previous one and collect
    /// the previous tick's events.
    ///
    /// The position in the stream is kept between calls, so rendering in
    /// chunks of any size gives the same samples as one call. Samples
    /// between two ticks are produced as one run.
    pub fn render_with<F: FnMut(&mut Self)>(&mut self, out: &mut [i16], mut before_tick: F) {
        let mut rest = out;
        while !rest.is_empty() {
            if self.output.until_tick == 0 {
                before_tick(self);
                self.tick();
                self.output.until_tick = self.output.rate.samples_per_tick();
            }
            let run = usize::try_from(self.output.until_tick)
                .unwrap_or(usize::MAX)
                .min(rest.len());
            let (now, later) = rest.split_at_mut(run);
            let volume = self.volume;
            for sample in now.iter_mut() {
                *sample = self.output.next_sample(volume);
            }
            // run <= until_tick, so it fits a u32 and the subtraction
            // cannot underflow.
            let run = u32::try_from(run).unwrap_or(self.output.until_tick);
            self.output.until_tick -= run;
            rest = later;
        }
    }

    /// Advances the jingle, or the music and the effects, by one tick and
    /// returns the chosen voice and the effects' offer.
    fn advance(&mut self) -> (Voice, Option<Divisor>) {
        if let Some(jingle) = self.jingle.as_mut() {
            let step = jingle.tick();
            self.music.set_level(Level::MAX);
            if step.done {
                self.jingle = None;
                self.events.level_done = true;
            }
            return (step.voice, None);
        }

        let music = if self.music_on || self.music.tune() == Some(Tune::Dirge) {
            self.music.tick(&self.tables)
        } else {
            MusicTick::default()
        };
        self.events.dirge |= music.dirge_finished;

        let EffectsTick {
            offer: effect_offer,
            stop_music,
        } = self.effects.tick();
        if stop_music {
            self.music.stop();
        }

        let voice = match (effect_offer, music.sounding) {
            (Some(divisor), _) => Voice::Effect(divisor),
            (None, Some(divisor)) => Voice::Music {
                divisor,
                level: self.music.level(),
            },
            (None, None) => Voice::Silent,
        };
        (voice, effect_offer)
    }

    /// Takes the events produced by the ticks since the last call, in a
    /// fixed order (level done, then dirge).
    pub fn take_events(&mut self) -> impl Iterator<Item = SoundEvent> + use<> {
        let PendingEvents { level_done, dirge } = core::mem::take(&mut self.events);
        [
            level_done.then_some(SoundEvent::LevelDoneFinished),
            dirge.then_some(SoundEvent::DirgeFinished),
        ]
        .into_iter()
        .flatten()
    }
}

/// Scales an output sample by the master volume: `sample * volume / 100`,
/// truncated toward zero (research 04 §4.3; fixed quirk SF2).
///
/// This is the last step of the output stage, after the optional filter.
pub const fn apply_volume(sample: i16, volume: Volume) -> i16 {
    let scaled = sample as i32 * volume.percent() as i32 / 100;
    // |scaled| <= |sample| because the volume is at most 100 %.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the volume is at most 100 %, so the result fits an i16"
    )]
    let scaled = scaled as i16;
    scaled
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SoundCommand;

    #[expect(
        clippy::expect_used,
        reason = "the embedded tables are tested to decode"
    )]
    fn synth() -> Synth {
        let tables = TuneTables::embedded().expect("embedded tables decode");
        Synth::new(SoundConfig::default(), tables)
    }

    #[test]
    fn empty_synth_is_silent() {
        let mut synth = synth();
        for tick in 1..=1000 {
            let trace = synth.tick();
            assert_eq!(trace.voice, Voice::Silent, "tick {tick}");
            assert_eq!(trace.effect_offer, None, "tick {tick}");
            assert_eq!(trace.music_level, INITIAL_MUSIC_LEVEL, "tick {tick}");
        }
        assert_eq!(synth.take_events().count(), 0);
    }

    #[test]
    fn stop_all_silences_a_running_effect() {
        let mut synth = synth();
        synth.apply(SoundCommand::Break.into());
        assert_eq!(synth.tick().voice, Voice::Effect(Divisor(15_000)));
        synth.apply(SoundCommand::StopAll.into());
        assert_eq!(synth.tick().voice, Voice::Silent);
    }

    #[test]
    fn half_volume_peak() {
        let half = Volume::new(50);
        assert_eq!(apply_volume(16_383, half), 8191);
        assert_eq!(apply_volume(-16_383, half), -8191);
        assert_eq!(apply_volume(i16::MIN, half), -16_384);
        assert_eq!(apply_volume(3, Volume::new(33)), 0);
        assert_eq!(apply_volume(-3, Volume::new(50)), -1);
    }

    #[test]
    fn zero_volume_is_silent() {
        for sample in [i16::MIN, -16_383, -1, 0, 1, 2340, 16_383, i16::MAX] {
            assert_eq!(apply_volume(sample, Volume::MUTE), 0, "{sample}");
        }
    }

    #[test]
    fn volume_above_100_clamps() {
        for percent in [100, 101, 150, 255] {
            let volume = Volume::new(percent);
            assert_eq!(volume, Volume::MAX);
            for sample in [i16::MIN, -16_383, 0, 16_383, i16::MAX] {
                assert_eq!(apply_volume(sample, volume), sample, "{percent} {sample}");
            }
        }
    }

    #[test]
    fn set_volume_is_applied_at_the_next_tick() {
        let mut synth = synth();
        assert_eq!(synth.volume(), Volume::MAX);
        synth.apply(SoundControl::SetVolume(Volume::new(200)).into());
        assert_eq!(synth.volume(), Volume::MAX);
        synth.apply(SoundControl::SetVolume(Volume::new(40)).into());
        assert_eq!(synth.volume().percent(), 40);
    }

    /// A configuration with the filter off, so samples are exact.
    fn unfiltered() -> SoundConfig {
        SoundConfig {
            filter: false,
            ..SoundConfig::default()
        }
    }

    #[expect(
        clippy::expect_used,
        reason = "the embedded tables are tested to decode"
    )]
    fn new_driver(config: SoundConfig) -> crate::ManualDriver {
        let tables = TuneTables::embedded().expect("embedded tables decode");
        crate::ManualDriver::new(config, tables)
    }

    /// The samples of tick `n` (counted from 1) at 44100 Hz.
    fn tick_samples(samples: &[i16], n: usize) -> &[i16] {
        let start = 605 + 606 * (n - 1);
        samples.get(start..start + 606).unwrap_or(&[])
    }

    #[test]
    fn first_tick_precedes_sample_605() {
        use crate::SoundSink;
        let mut driver = new_driver(unfiltered());
        driver.send(SoundCommand::Break);
        let samples = driver.render(1_212);
        assert!(samples.iter().take(605).all(|&s| s == 0));
        assert_eq!(samples.get(605), Some(&16_383));

        // A click sent 100 samples after a tick starts 506 samples later,
        // at the next tick.
        let mut driver = new_driver(unfiltered());
        driver.render(705);
        driver.send(SoundCommand::EmeraldClick);
        let samples = driver.render(507);
        assert!(samples.iter().take(506).all(|&s| s == 0));
        assert_eq!(samples.get(506), Some(&16_383));
    }

    #[test]
    fn break_peak_is_16383() {
        use crate::SoundSink;
        let mut driver = new_driver(unfiltered());
        driver.send(SoundCommand::Break);
        let samples = driver.render(605 + 606 * 4);
        for n in 1..=3 {
            let tick = tick_samples(&samples, n);
            assert_eq!(tick.len(), 606);
            assert!(
                tick.iter().all(|&s| s == 16_383 || s == -16_383),
                "tick {n}"
            );
            // 79 Hz: one level change at most per 279 samples, so both
            // levels appear within a tick.
            assert!(
                tick.contains(&16_383) && tick.contains(&-16_383),
                "tick {n}"
            );
        }
        assert!(tick_samples(&samples, 4).iter().all(|&s| s == 0));
    }

    #[test]
    fn normal_tune_sustain_peak_is_2340() {
        use crate::SoundSink;
        use crate::command::Tempo;
        let music = SoundCommand::Music {
            tune: Tune::Main,
            tempo: Tempo::Standard,
        };
        let mut traced = new_driver(unfiltered());
        traced.send(music);
        let trace = traced.advance_ticks(40);
        let sustain: Vec<usize> = trace
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                matches!(t.output, Voice::Music { divisor, level }
                    if level.get() == 8 && crate::voice::is_audible(divisor))
            })
            .map(|(i, _)| i + 1)
            .collect();
        // A two-unit note sustains at level 8 for eight ticks.
        assert!(sustain.len() >= 8, "sustain ticks {sustain:?}");

        let mut rendered = new_driver(unfiltered());
        rendered.send(music);
        let samples = rendered.render(605 + 606 * 40);
        for n in sustain {
            let tick = tick_samples(&samples, n);
            assert!(tick.iter().all(|&s| s == 2_340 || s == -2_340), "tick {n}");
            assert!(tick.contains(&2_340) && tick.contains(&-2_340), "tick {n}");
        }
    }

    #[test]
    fn silence_is_zero() {
        use crate::SoundSink;
        let mut driver = new_driver(unfiltered());
        assert!(driver.render(44_100).iter().all(|&s| s == 0));
        // A finished effect returns to exactly 0, as does a stop-all.
        // Two ticks' worth covers the click's one tick wherever the render
        // position is.
        driver.send(SoundCommand::EmeraldClick);
        driver.render(1_212);
        assert!(driver.render(6_060).iter().all(|&s| s == 0));
        driver.send(SoundCommand::BonusPulseStart);
        driver.render(6_060);
        driver.send(SoundCommand::StopAll);
        driver.render(1_212);
        assert!(driver.render(6_060).iter().all(|&s| s == 0));
        // With the filter on, silence is 0 too.
        let mut driver = new_driver(SoundConfig::default());
        assert!(driver.render(44_100).iter().all(|&s| s == 0));
    }

    /// Renders a mixed script into chunks of `chunk` samples.
    fn render_in_chunks(config: SoundConfig, chunk: usize, total: usize) -> Vec<i16> {
        use crate::command::Tempo;
        let tables = TuneTables::embedded().unwrap_or_else(|_| unreachable!());
        let mut synth = Synth::new(config, tables);
        for command in [
            SoundCommand::Music {
                tune: Tune::Main,
                tempo: Tempo::Standard,
            },
            SoundCommand::Gold,
            SoundCommand::WobbleStart,
        ] {
            synth.apply(command.into());
        }
        synth.apply(SoundControl::SetVolume(Volume::new(70)).into());
        let mut out = vec![0; total];
        for part in out.chunks_mut(chunk) {
            synth.render(part);
        }
        out
    }

    #[test]
    fn render_is_chunk_size_independent() {
        let total = 44_100 * 3;
        for config in [unfiltered(), SoundConfig::default()] {
            let whole = render_in_chunks(config, total, total);
            assert!(whole.iter().any(|&s| s != 0));
            for chunk in [1, 7, 512, 4096] {
                assert!(
                    render_in_chunks(config, chunk, total) == whole,
                    "chunk {chunk}, filter {}",
                    config.filter
                );
            }
        }
    }

    #[test]
    fn jingle_is_gated_by_its_beat() {
        use crate::SoundSink;
        let mut driver = new_driver(unfiltered());
        driver.send(SoundCommand::LevelDone);
        let samples = driver.render(605 + 606 * 21);
        // The first note: 515 Hz carrier (divisor 2315), 8 Hz beat, so on
        // for 2757 samples and off for the next 2756.
        let note: Vec<i16> = samples.iter().skip(605).copied().collect();
        assert!(
            note.iter()
                .take(2_757)
                .all(|&s| s == 16_383 || s == -16_383)
        );
        assert!(note.iter().skip(2_757).take(2_756).all(|&s| s == 0));
        assert_ne!(note.get(5_513), Some(&0));
    }

    #[test]
    fn filter_toggle() {
        use crate::SoundSink;
        let script = |driver: &mut crate::ManualDriver| {
            driver.send(SoundCommand::BonusPulseStart);
            driver.render(606 * 10)
        };
        let raw = script(&mut new_driver(unfiltered()));
        let filtered = script(&mut new_driver(SoundConfig::default()));
        assert_ne!(raw, filtered);
        assert!(raw.iter().all(|&s| s == 0 || s.abs() == 16_383));

        // Switching the filter off gives the raw wave from the next tick.
        let mut toggled = new_driver(SoundConfig::default());
        assert!(toggled.synth().filter_on());
        toggled.control(SoundControl::SetFilter(false));
        assert_eq!(script(&mut toggled), raw);
        assert!(!toggled.synth().filter_on());

        // Switching it on starts from a clean state: the same samples as a
        // filter that was on from the start.
        let mut off_on = new_driver(unfiltered());
        off_on.control(SoundControl::SetFilter(true));
        assert_eq!(script(&mut off_on), filtered);

        // Switching on after the filter was used and switched off clears
        // the state left from before.
        let mut reused = new_driver(SoundConfig::default());
        reused.send(SoundCommand::Gold);
        reused.render(606 * 3);
        reused.control(SoundControl::SetFilter(false));
        reused.render(606);
        reused.send(SoundCommand::StopAll);
        reused.control(SoundControl::SetFilter(true));
        reused.render(606);
        let mut fresh = new_driver(unfiltered());
        fresh.render(606 * 5);
        fresh.send(SoundCommand::StopAll);
        fresh.control(SoundControl::SetFilter(true));
        fresh.render(606);
        let a = script(&mut reused);
        let b = script(&mut fresh);
        assert_eq!(a, b);
    }

    #[test]
    fn initial_music_level_is_8() {
        assert_eq!(INITIAL_MUSIC_LEVEL.get(), 8);
    }
}
