//! The pure sound core: inputs in, one voice per tick out (design D1, D7).
//!
//! [`Synth`] owns every piece of sound state. It has no queue and no
//! thread: a driver applies the inputs that arrived since the last tick
//! with [`Synth::apply`], in the order they were sent, and then calls
//! [`Synth::tick`]. Nothing here allocates or fails.

use crate::command::{Input, SoundCommand, SoundEvent, Tune};
use crate::config::SoundConfig;
use crate::effects::{Effects, EffectsTick};
use crate::jingle::LevelDone;
use crate::music::{MusicPlayer, MusicTick, TuneTables};
use crate::units::Level;
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

    /// Applies one command or control. It takes effect on the next
    /// [`Synth::tick`], before anything advances on that tick.
    pub fn apply(&mut self, input: Input) {
        match input {
            Input::Game(command) => self.apply_command(command),
            // Pause and the controls are added by the next task group;
            // until then they are ignored.
            Input::Control(_) => {}
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
    pub fn tick(&mut self) -> TickTrace {
        if let Some(jingle) = self.jingle.as_mut() {
            let step = jingle.tick();
            self.music.set_level(Level::MAX);
            if step.done {
                self.jingle = None;
                self.events.level_done = true;
            }
            return TickTrace {
                voice: step.voice,
                effect_offer: None,
                music_level: self.music.level(),
            };
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
        TickTrace {
            voice,
            effect_offer,
            music_level: self.music.level(),
        }
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
        assert_eq!(synth.tick().voice, Voice::Effect(crate::Divisor(15_000)));
        synth.apply(SoundCommand::StopAll.into());
        assert_eq!(synth.tick().voice, Voice::Silent);
    }

    #[test]
    fn initial_music_level_is_8() {
        assert_eq!(INITIAL_MUSIC_LEVEL.get(), 8);
    }
}
