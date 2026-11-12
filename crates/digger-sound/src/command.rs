//! What goes into the sound engine and what comes out of it, besides
//! samples (design D2).
//!
//! - [`SoundCommand`] is what the simulation emits. It is defined in
//!   `digger-core`, which owns it, and re-exported here unchanged together
//!   with its supporting types [`FireballId`], [`EmeraldStep`], [`Tune`]
//!   and [`Tempo`].
//! - [`SoundControl`] is what the program's frontend sends: the switches,
//!   the master volume and the filter. It never appears in the
//!   simulation's output or in recordings.
//! - [`SoundEvent`] is what the engine reports back to the frontend. It is
//!   informational only and never an input of the simulation.
//!
//! None of these carries an acknowledgement id.

pub use digger_core::{EmeraldStep, FireballId, SoundCommand, Tempo, Tune};

use crate::units::Volume;

/// A control sent by the program's frontend (design D2).
///
/// Controls take effect at the next tick, in order with commands. Pause is
/// not a control: it is the simulation's [`SoundCommand::Pause`] and
/// [`SoundCommand::Resume`], so the pause state has a single source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundControl {
    /// Switches sound (the mute) on or off. Switching sound on also
    /// switches music on.
    ToggleSound,
    /// Switches music on or off.
    ToggleMusic,
    /// Sets the master volume.
    SetVolume(Volume),
    /// Switches the output filter on (`true`) or off (`false`).
    SetFilter(bool),
}

/// An event reported by the engine to the frontend (design D9).
///
/// Events are for presentation only; nothing in the simulation waits for
/// them, and unread events never affect the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundEvent {
    /// The level-done jingle has finished, or a stop-all cut it short.
    LevelDoneFinished,
    /// The dirge has played its audible part.
    DirgeFinished,
}

/// One item on the way into the engine: a simulation command or a
/// frontend control (design D2).
///
/// Both kinds share one queue so that they take effect in the order they
/// were sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Input {
    /// A command emitted by the simulation.
    Game(SoundCommand),
    /// A control sent by the frontend.
    Control(SoundControl),
}

impl From<SoundCommand> for Input {
    fn from(command: SoundCommand) -> Self {
        Self::Game(command)
    }
}

impl From<SoundControl> for Input {
    fn from(control: SoundControl) -> Self {
        Self::Control(control)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The spec name of a command ("Sound command boundary"), or `None`
    /// for a command this crate does not know.
    ///
    /// `SoundCommand` is `#[non_exhaustive]` in `digger-core`, so a match
    /// in another crate needs a wildcard arm; completeness is therefore
    /// checked by listing every variant below and asserting that each one
    /// is named and that there are exactly 22.
    fn spec_name(command: SoundCommand) -> Option<&'static str> {
        Some(match command {
            SoundCommand::StopAll => "stop-all",
            SoundCommand::FallStart => "fall-start",
            SoundCommand::FallStop => "fall-stop",
            SoundCommand::WobbleStart => "wobble-start",
            SoundCommand::WobbleStop => "wobble-stop",
            SoundCommand::Break => "break",
            SoundCommand::Fire(_) => "fire-start",
            SoundCommand::FireStop(_) => "fire-stop",
            SoundCommand::Explode(_) => "explode",
            SoundCommand::BonusPulseStart => "bonus-start",
            SoundCommand::BonusPulseStop => "bonus-stop",
            SoundCommand::EmeraldClick => "emerald-click",
            SoundCommand::EmeraldNote(_) => "emerald",
            SoundCommand::Gold => "gold",
            SoundCommand::EatMonster => "eat-monster",
            SoundCommand::DiggerDeath => "death",
            SoundCommand::ExtraLife => "extra-life",
            SoundCommand::LevelDone => "level-done",
            SoundCommand::Music {
                tune: Tune::Main | Tune::LastLife | Tune::Bonus | Tune::Dirge,
                tempo: Tempo::Standard | Tempo::Quick,
            } => "play-music",
            SoundCommand::MusicOff => "stop-music",
            SoundCommand::Pause => "pause",
            SoundCommand::Resume => "resume",
            _ => return None,
        })
    }

    #[test]
    fn command_boundary_is_complete() {
        let [fireball, _] = FireballId::ALL;
        let [step, ..] = EmeraldStep::ALL;
        let all = [
            SoundCommand::StopAll,
            SoundCommand::FallStart,
            SoundCommand::FallStop,
            SoundCommand::WobbleStart,
            SoundCommand::WobbleStop,
            SoundCommand::Break,
            SoundCommand::Fire(fireball),
            SoundCommand::FireStop(fireball),
            SoundCommand::Explode(fireball),
            SoundCommand::BonusPulseStart,
            SoundCommand::BonusPulseStop,
            SoundCommand::EmeraldClick,
            SoundCommand::EmeraldNote(step),
            SoundCommand::Gold,
            SoundCommand::EatMonster,
            SoundCommand::DiggerDeath,
            SoundCommand::ExtraLife,
            SoundCommand::LevelDone,
            SoundCommand::Music {
                tune: Tune::Main,
                tempo: Tempo::Standard,
            },
            SoundCommand::MusicOff,
            SoundCommand::Pause,
            SoundCommand::Resume,
        ];
        assert_eq!(all.len(), 22);
        let names = all.map(|command| spec_name(command).unwrap_or("unknown"));
        assert!(!names.contains(&"unknown"), "{names:?}");
        let mut distinct = names.to_vec();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), 22, "{names:?}");

        // The supporting types hold exactly the values design D2 names.
        assert_eq!(FireballId::ALL.map(FireballId::number), [0, 1]);
        assert_eq!(
            EmeraldStep::ALL.map(EmeraldStep::number),
            [0, 1, 2, 3, 4, 5, 6, 7]
        );
        assert_eq!(EmeraldStep::new(8), None);
        assert_eq!(Tempo::default(), Tempo::Standard);
        for tune in [Tune::Main, Tune::LastLife, Tune::Bonus, Tune::Dirge] {
            for tempo in [Tempo::Standard, Tempo::Quick] {
                assert_eq!(
                    spec_name(SoundCommand::Music { tune, tempo }),
                    Some("play-music")
                );
            }
        }
    }
}
