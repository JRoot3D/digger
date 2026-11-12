//! Command scripts: a small text format that lists what to send to the
//! engine and at which sample (design D12).
//!
//! The golden-render tests and `cargo xtask render-sound` read the same
//! scripts, so a render reviewed by ear is the render whose hash is kept.
//!
//! # Format
//!
//! One entry per line: a sample offset (a decimal number; `_` may separate
//! digits) and a command. Blank lines and everything after `#` are
//! ignored. Offsets never decrease. The last entry is `end`, whose offset
//! is the length of the render in samples.
//!
//! An entry at offset `s` is sent before sample `s` is rendered, so it
//! takes effect at the first tick at or after sample `s`.
//!
//! | Command | Sends |
//! |---|---|
//! | `stop-all` | stop-all |
//! | `fall-start`, `fall-stop` | bag fall |
//! | `wobble-start`, `wobble-stop` | bag wobble |
//! | `break` | bag break |
//! | `fire-start <0\|1>`, `fire-stop <0\|1>` | fireball flight |
//! | `explode <0\|1>` | fireball explosion |
//! | `bonus-start`, `bonus-stop` | bonus siren |
//! | `emerald-click` | emerald click |
//! | `emerald <0..7>` | emerald scale step |
//! | `gold` | gold |
//! | `eat-monster` | eat-monster |
//! | `death` | death |
//! | `extra-life` | extra life |
//! | `level-done` | level-done jingle |
//! | `music <normal\|bonus\|dirge\|battle> [standard\|quick]` | play-music (standard tempo if omitted) |
//! | `music-off` | stop-music |
//! | `pause`, `resume` | pause, resume |
//! | `toggle-sound`, `toggle-music` | the frontend's sound and music switches |
//! | `volume <0..100>` | the master volume (larger values clamp to 100) |
//! | `filter <on\|off>` | the output filter |
//! | `end` | nothing; ends the script |
//!
//! ```
//! use digger_sound::{Script, SoundConfig, TuneTables};
//!
//! let script: Script = "0 gold\n1212 end\n".parse().expect("valid script");
//! let tables = TuneTables::embedded().expect("embedded tables decode");
//! let config = SoundConfig { filter: false, ..SoundConfig::default() };
//! let samples = script.render(config, tables);
//! assert_eq!(samples.len(), 1212);
//! ```

use core::fmt;
use core::str::FromStr;

use crate::command::{EmeraldStep, FireballId, Input, SoundCommand, SoundControl, Tempo, Tune};
use crate::config::SoundConfig;
use crate::driver::{ManualDriver, SoundSink};
use crate::music::TuneTables;
use crate::units::Volume;

/// One entry of a script: an input and the sample before which it is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScriptEntry {
    /// The sample before which the input is sent.
    pub offset: u64,
    /// The command or control sent.
    pub input: Input,
}

/// A parsed command script: inputs at sample offsets and a total length.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Script {
    entries: Vec<ScriptEntry>,
    length: u64,
}

impl Script {
    /// The inputs, in the order they are sent.
    pub fn entries(&self) -> &[ScriptEntry] {
        &self.entries
    }

    /// The length of the render in samples (the offset of `end`).
    pub const fn length(&self) -> u64 {
        self.length
    }

    /// Renders the script through a [`ManualDriver`] created with `config`
    /// and `tables`, and returns all [`Script::length`] samples.
    ///
    /// # Panics
    ///
    /// If the length does not fit in memory on this platform (a `usize`).
    #[expect(
        clippy::expect_used,
        reason = "a script longer than the address space cannot be rendered"
    )]
    pub fn render(&self, config: SoundConfig, tables: TuneTables) -> Vec<i16> {
        let length = usize::try_from(self.length).expect("script length fits in memory");
        let mut out = vec![0_i16; length];
        let mut driver = ManualDriver::new(config, tables);
        let mut done = 0_usize;
        for entry in &self.entries {
            // Offsets are at most `length`, so they fit as well.
            let offset = usize::try_from(entry.offset).unwrap_or(length);
            if let Some(chunk) = out.get_mut(done..offset) {
                driver.render_into(chunk);
                done = offset;
            }
            match entry.input {
                Input::Game(command) => driver.send(command),
                Input::Control(control) => driver.control(control),
            }
        }
        if let Some(chunk) = out.get_mut(done..) {
            driver.render_into(chunk);
        }
        out
    }
}

/// Why a script could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ScriptError {
    /// A line has no command after its offset.
    MissingCommand {
        /// The 1-based line number.
        line: usize,
    },
    /// The offset is not a decimal number.
    BadOffset {
        /// The 1-based line number.
        line: usize,
    },
    /// The offset is smaller than the one before it.
    OffsetDecreases {
        /// The 1-based line number.
        line: usize,
    },
    /// The command name is not one of the script commands.
    UnknownCommand {
        /// The 1-based line number.
        line: usize,
        /// The unrecognised name.
        name: String,
    },
    /// The command's arguments are missing, extra or out of range.
    BadArgument {
        /// The 1-based line number.
        line: usize,
    },
    /// An entry follows `end`.
    AfterEnd {
        /// The 1-based line number.
        line: usize,
    },
    /// The script has no `end` entry.
    MissingEnd,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCommand { line } => write!(f, "line {line}: no command after the offset"),
            Self::BadOffset { line } => write!(f, "line {line}: the offset is not a number"),
            Self::OffsetDecreases { line } => {
                write!(
                    f,
                    "line {line}: the offset is smaller than the previous one"
                )
            }
            Self::UnknownCommand { line, name } => {
                write!(f, "line {line}: unknown command {name:?}")
            }
            Self::BadArgument { line } => {
                write!(f, "line {line}: missing, extra or out-of-range argument")
            }
            Self::AfterEnd { line } => write!(f, "line {line}: entry after `end`"),
            Self::MissingEnd => write!(f, "the script has no `end` entry"),
        }
    }
}

impl std::error::Error for ScriptError {}

impl FromStr for Script {
    type Err = ScriptError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut entries = Vec::new();
        let mut length = None;
        let mut previous = 0_u64;
        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let content = raw.split('#').next().unwrap_or_default();
            let mut words = content.split_whitespace();
            let Some(offset) = words.next() else {
                continue;
            };
            if length.is_some() {
                return Err(ScriptError::AfterEnd { line });
            }
            let offset = parse_offset(offset).ok_or(ScriptError::BadOffset { line })?;
            if offset < previous {
                return Err(ScriptError::OffsetDecreases { line });
            }
            previous = offset;
            let name = words.next().ok_or(ScriptError::MissingCommand { line })?;
            let args: Vec<&str> = words.collect();
            if name == "end" {
                if !args.is_empty() {
                    return Err(ScriptError::BadArgument { line });
                }
                length = Some(offset);
                continue;
            }
            let input = parse_input(name, &args, line)?;
            entries.push(ScriptEntry { offset, input });
        }
        let length = length.ok_or(ScriptError::MissingEnd)?;
        Ok(Self { entries, length })
    }
}

/// A decimal number with optional `_` separators between digits.
fn parse_offset(word: &str) -> Option<u64> {
    if word.starts_with('_') || word.ends_with('_') {
        return None;
    }
    let digits: String = word.chars().filter(|&c| c != '_').collect();
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The input named by `name` with arguments `args`.
fn parse_input(name: &str, args: &[&str], line: usize) -> Result<Input, ScriptError> {
    let bad = ScriptError::BadArgument { line };
    let none = |command: SoundCommand| {
        if args.is_empty() {
            Ok(Input::Game(command))
        } else {
            Err(bad.clone())
        }
    };
    let one = || match args {
        [arg] => Ok(*arg),
        _ => Err(bad.clone()),
    };
    let number = || -> Result<u8, ScriptError> { one()?.parse().map_err(|_| bad.clone()) };
    let fireball =
        || -> Result<FireballId, ScriptError> { FireballId::new(number()?).ok_or(bad.clone()) };
    match name {
        "stop-all" => none(SoundCommand::StopAll),
        "fall-start" => none(SoundCommand::FallStart),
        "fall-stop" => none(SoundCommand::FallStop),
        "wobble-start" => none(SoundCommand::WobbleStart),
        "wobble-stop" => none(SoundCommand::WobbleStop),
        "break" => none(SoundCommand::Break),
        "fire-start" => Ok(Input::Game(SoundCommand::Fire(fireball()?))),
        "fire-stop" => Ok(Input::Game(SoundCommand::FireStop(fireball()?))),
        "explode" => Ok(Input::Game(SoundCommand::Explode(fireball()?))),
        "bonus-start" => none(SoundCommand::BonusPulseStart),
        "bonus-stop" => none(SoundCommand::BonusPulseStop),
        "emerald-click" => none(SoundCommand::EmeraldClick),
        "emerald" => {
            let step = EmeraldStep::new(number()?).ok_or(bad.clone())?;
            Ok(Input::Game(SoundCommand::EmeraldNote(step)))
        }
        "gold" => none(SoundCommand::Gold),
        "eat-monster" => none(SoundCommand::EatMonster),
        "death" => none(SoundCommand::DiggerDeath),
        "extra-life" => none(SoundCommand::ExtraLife),
        "level-done" => none(SoundCommand::LevelDone),
        "music" => {
            let (tune, tempo) = match args {
                [tune] => (*tune, "standard"),
                [tune, tempo] => (*tune, *tempo),
                _ => return Err(bad),
            };
            let tune = match tune {
                "normal" => Tune::Main,
                "bonus" => Tune::Bonus,
                "dirge" => Tune::Dirge,
                "battle" => Tune::LastLife,
                _ => return Err(bad),
            };
            let tempo = match tempo {
                "standard" => Tempo::Standard,
                "quick" => Tempo::Quick,
                _ => return Err(bad),
            };
            Ok(Input::Game(SoundCommand::Music { tune, tempo }))
        }
        "music-off" => none(SoundCommand::MusicOff),
        "pause" => none(SoundCommand::Pause),
        "resume" => none(SoundCommand::Resume),
        "toggle-sound" | "toggle-music" => {
            if !args.is_empty() {
                return Err(bad);
            }
            Ok(Input::Control(if name == "toggle-sound" {
                SoundControl::ToggleSound
            } else {
                SoundControl::ToggleMusic
            }))
        }
        "volume" => {
            let percent: u32 = one()?.parse().map_err(|_| bad.clone())?;
            let percent = u8::try_from(percent.min(100)).map_err(|_| bad.clone())?;
            Ok(Input::Control(SoundControl::SetVolume(Volume::new(
                percent,
            ))))
        }
        "filter" => match one()? {
            "on" => Ok(Input::Control(SoundControl::SetFilter(true))),
            "off" => Ok(Input::Control(SoundControl::SetFilter(false))),
            _ => Err(bad),
        },
        _ => Err(ScriptError::UnknownCommand {
            line,
            name: name.to_owned(),
        }),
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Script, ScriptError> {
        text.parse()
    }

    #[test]
    fn parses_every_command() {
        let script = parse(
            "# all commands\n\
             0 stop-all\n0 fall-start\n0 fall-stop\n0 wobble-start\n0 wobble-stop\n\
             0 break\n0 fire-start 0\n0 fire-stop 1\n0 explode 1\n0 bonus-start\n\
             0 bonus-stop\n0 emerald-click\n0 emerald 7\n0 gold\n0 eat-monster\n\
             0 death\n0 extra-life\n0 level-done\n0 music normal\n\
             1_000 music dirge quick   # trailing comment\n\
             1000 music-off\n1000 pause\n1000 resume\n1000 toggle-sound\n\
             1000 toggle-music\n1000 volume 250\n1000 filter off\n\n2000 end\n",
        )
        .expect("valid script");
        assert_eq!(script.length(), 2000);
        assert_eq!(script.entries().len(), 27);
        let dirge = script.entries().get(19).expect("entry 19");
        assert_eq!(dirge.offset, 1000);
        assert_eq!(
            dirge.input,
            Input::Game(SoundCommand::Music {
                tune: Tune::Dirge,
                tempo: Tempo::Quick
            })
        );
        assert_eq!(
            script.entries().get(25).map(|e| e.input),
            Some(Input::Control(SoundControl::SetVolume(Volume::MAX)))
        );
    }

    #[test]
    fn rejects_bad_scripts() {
        assert_eq!(parse("0 gold\n"), Err(ScriptError::MissingEnd));
        assert_eq!(
            parse("x gold\n1 end"),
            Err(ScriptError::BadOffset { line: 1 })
        );
        assert_eq!(
            parse("5 gold\n4 end"),
            Err(ScriptError::OffsetDecreases { line: 2 })
        );
        assert_eq!(parse("5\n"), Err(ScriptError::MissingCommand { line: 1 }));
        assert_eq!(
            parse("1 end\n2 gold"),
            Err(ScriptError::AfterEnd { line: 2 })
        );
        assert_eq!(
            parse("0 emerald 8\n1 end"),
            Err(ScriptError::BadArgument { line: 1 })
        );
        assert_eq!(
            parse("0 fire-start\n1 end"),
            Err(ScriptError::BadArgument { line: 1 })
        );
        assert_eq!(
            parse("0 gold 1\n1 end"),
            Err(ScriptError::BadArgument { line: 1 })
        );
        assert_eq!(
            parse("0 music jazz\n1 end"),
            Err(ScriptError::BadArgument { line: 1 })
        );
        assert_eq!(
            parse("0 boom\n1 end"),
            Err(ScriptError::UnknownCommand {
                line: 1,
                name: "boom".to_owned()
            })
        );
    }

    #[test]
    fn entry_takes_effect_at_the_next_tick() {
        let tables = TuneTables::embedded().expect("embedded tables decode");
        let config = SoundConfig {
            filter: false,
            ..SoundConfig::default()
        };
        // Sent after the first tick (before sample 605): silent until the
        // second tick, before sample 1211.
        let samples = parse("606 break\n1212 end")
            .expect("valid script")
            .render(config, tables);
        assert_eq!(samples.len(), 1212);
        assert!(samples.get(..1211).expect("prefix").iter().all(|&s| s == 0));
        assert_eq!(samples.get(1211).map(|s| s.unsigned_abs()), Some(16383));
    }
}
