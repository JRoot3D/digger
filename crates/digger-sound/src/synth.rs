//! The pure sound core: inputs in, one voice per tick out (design D1, D7).
//!
//! [`Synth`] owns every piece of sound state. It has no queue and no
//! thread: a driver applies the inputs that arrived since the last tick
//! with [`Synth::apply`], in the order they were sent, and then calls
//! [`Synth::tick`]. Nothing here allocates or fails.

use crate::command::{Input, SoundCommand, SoundControl, SoundEvent, Tune};
use crate::config::SoundConfig;
use crate::effects::{Effects, EffectsTick};
use crate::jingle::LevelDone;
use crate::music::{MusicPlayer, MusicTick, TuneTables};
use crate::units::{Divisor, Level, Volume};
use crate::voice::{TickTrace, Voice};

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

/// The sound engine's state and its tick.
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
            // The output filter is added with the sample synthesis; until
            // then the switch has nothing to act on.
            SoundControl::SetFilter(_) => {}
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
    pub fn tick(&mut self) -> TickTrace {
        if self.paused {
            return TickTrace {
                voice: Voice::Silent,
                output: Voice::Silent,
                effect_offer: None,
                music_level: self.music.level(),
            };
        }
        let (voice, effect_offer) = self.advance();
        TickTrace {
            voice,
            output: if self.sound_on { voice } else { Voice::Silent },
            effect_offer,
            music_level: self.music.level(),
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

    #[test]
    fn initial_music_level_is_8() {
        assert_eq!(INITIAL_MUSIC_LEVEL.get(), 8);
    }
}
