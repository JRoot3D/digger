//! The pure sound core: inputs in, one voice per tick out (design D1, D7).
//!
//! [`Synth`] owns every piece of sound state. It has no queue and no
//! thread: a driver applies the inputs that arrived since the last tick
//! with [`Synth::apply`], in the order they were sent, and then calls
//! [`Synth::tick`]. Nothing here allocates or fails.

use crate::command::{Input, SoundEvent};
use crate::config::SoundConfig;
use crate::effects::{Effects, EffectsTick};
use crate::music::TuneTables;
use crate::units::Level;
use crate::voice::{TickTrace, Voice};

/// The music level the engine starts with (kept quirk SQ6; research 04
/// §1.3).
pub const INITIAL_MUSIC_LEVEL: Level = Level::saturating(8);

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
    music_level: Level,
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
            music_level: INITIAL_MUSIC_LEVEL,
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
            Input::Game(command) => self.effects.apply(command),
            // The tunes, the jingle, pause and the controls are added by
            // the later task groups; until then they are ignored.
            Input::Control(_) => {}
        }
    }

    /// Advances the engine by one tick and reports what sounds on it.
    pub fn tick(&mut self) -> TickTrace {
        let EffectsTick {
            offer: effect_offer,
            stop_music,
        } = self.effects.tick();
        // The death effect stops the tune on its first tick; the music
        // player that acts on this is added with the tunes (task group 5).
        let _ = stop_music;
        let voice = effect_offer.map_or(Voice::Silent, Voice::Effect);
        TickTrace {
            voice,
            effect_offer,
            music_level: self.music_level,
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
