//! The static schema of the configuration file: one entry per setting with
//! its key path, type, range and default (design D9). The file reader
//! checks values against it, the defaults are built from it and the file
//! writer and the manual's settings table follow its order.

use std::fmt;
use std::sync::LazyLock;

use super::{GameMode, ScaleMode, Speed};
use crate::input::{Action, KeyName, PadAction, PadButton};

/// The tables of the file, in the order they are written.
pub(crate) const SECTIONS: [&str; 5] = ["game", "sound", "video", "keys", "gamepad"];

/// Which setting an entry describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Field {
    /// `game.speed`.
    Speed,
    /// `game.mode`.
    Mode,
    /// `game.gauntlet_time`.
    GauntletTime,
    /// `game.start_level`.
    StartLevel,
    /// `game.unlimited_lives`.
    UnlimitedLives,
    /// `sound.enabled`.
    SoundEnabled,
    /// `sound.music`.
    Music,
    /// `sound.sample_rate`.
    SampleRate,
    /// `sound.buffer_size`.
    BufferSize,
    /// `sound.volume`.
    Volume,
    /// `video.fullscreen`.
    Fullscreen,
    /// `video.scale`.
    Scale,
    /// `video.reduce_flashing`.
    ReduceFlashing,
    /// `keys.<action>`.
    Key(Action),
    /// `gamepad.enabled`.
    PadEnabled,
    /// `gamepad.dead_zone`.
    DeadZone,
    /// `gamepad.<fire|pause|change_mode|quit>`.
    PadButtons(PadAction),
}

/// The type and range of a setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Kind {
    /// A whole number in `min..=max`.
    Integer {
        /// Smallest allowed value.
        min: i64,
        /// Largest allowed value.
        max: i64,
    },
    /// A number in `min..=max` (a whole number is accepted too).
    Float {
        /// Smallest allowed value.
        min: f64,
        /// Largest allowed value.
        max: f64,
    },
    /// `true` or `false`.
    Boolean,
    /// One of a fixed list of strings.
    Choice(&'static [&'static str]),
    /// A list of one or more key names.
    Keys,
    /// A list of gamepad button names, possibly empty.
    Buttons,
}

/// A value checked against its entry's [`Kind`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    /// For [`Kind::Integer`].
    Integer(i64),
    /// For [`Kind::Float`].
    Float(f64),
    /// For [`Kind::Boolean`].
    Boolean(bool),
    /// For [`Kind::Choice`]: the chosen name.
    Choice(&'static str),
    /// For [`Kind::Keys`].
    Keys(Vec<KeyName>),
    /// For [`Kind::Buttons`].
    Buttons(Vec<PadButton>),
}

impl fmt::Display for Value {
    /// The value as TOML text.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn list<T: fmt::Display>(f: &mut fmt::Formatter<'_>, items: &[T]) -> fmt::Result {
            f.write_str("[")?;
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "\"{item}\"")?;
            }
            f.write_str("]")
        }
        match self {
            Self::Integer(v) => write!(f, "{v}"),
            Self::Float(v) => {
                let text = v.to_string();
                if text.contains(['.', 'e', 'E', 'n', 'i']) {
                    f.write_str(&text)
                } else {
                    write!(f, "{text}.0")
                }
            }
            Self::Boolean(v) => write!(f, "{v}"),
            Self::Choice(name) => write!(f, "\"{name}\""),
            Self::Keys(keys) => list(f, keys),
            Self::Buttons(buttons) => list(f, buttons),
        }
    }
}

/// The default of a setting, as written in the specification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum DefaultValue {
    /// A whole number.
    Integer(i64),
    /// A number.
    Float(f64),
    /// A boolean.
    Boolean(bool),
    /// A choice name.
    Choice(&'static str),
    /// Key names.
    Keys(&'static [&'static str]),
    /// Button names.
    Buttons(&'static [&'static str]),
}

/// One setting of the configuration file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Entry {
    /// The table it lives in (`game`, `sound`, …).
    pub(crate) section: &'static str,
    /// Its name inside the table.
    pub(crate) name: &'static str,
    /// Which setting it is.
    pub(crate) field: Field,
    /// Its type and range.
    pub(crate) kind: Kind,
    /// Its default.
    pub(crate) default: DefaultValue,
    /// The comment written after it in the file (empty for none).
    pub(crate) comment: &'static str,
}

impl Entry {
    /// The full key path, `section.name`.
    pub(crate) fn path(&self) -> String {
        format!("{}.{}", self.section, self.name)
    }

    /// The default as a checked value.
    pub(crate) fn default_value(&self) -> Value {
        match self.default {
            DefaultValue::Integer(v) => Value::Integer(v),
            DefaultValue::Float(v) => Value::Float(v),
            DefaultValue::Boolean(v) => Value::Boolean(v),
            DefaultValue::Choice(name) => Value::Choice(name),
            DefaultValue::Keys(names) => {
                Value::Keys(names.iter().filter_map(|n| KeyName::parse(n)).collect())
            }
            DefaultValue::Buttons(names) => {
                Value::Buttons(names.iter().filter_map(|n| PadButton::parse(n)).collect())
            }
        }
    }

    /// Checks a value read from the file against this entry.
    ///
    /// # Errors
    ///
    /// The problem, worded to follow `<file>: <key path>: `, for example
    /// `999 is outside 8000–192000`.
    pub(crate) fn check(&self, raw: &toml::Value) -> Result<Value, String> {
        match self.kind {
            Kind::Integer { min, max } => {
                let v = raw
                    .as_integer()
                    .ok_or_else(|| expected("a whole number", raw))?;
                if (min..=max).contains(&v) {
                    Ok(Value::Integer(v))
                } else {
                    Err(format!("{v} is outside {min}–{max}"))
                }
            }
            Kind::Float { min, max } => {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a whole number given for a fraction is far below 2^52"
                )]
                let v = raw
                    .as_float()
                    .or_else(|| raw.as_integer().map(|v| v as f64))
                    .ok_or_else(|| expected("a number", raw))?;
                if (min..=max).contains(&v) {
                    Ok(Value::Float(v))
                } else {
                    Err(format!("{v} is outside {min}–{max}"))
                }
            }
            Kind::Boolean => raw
                .as_bool()
                .map(Value::Boolean)
                .ok_or_else(|| expected("true or false", raw)),
            Kind::Choice(names) => {
                let text = raw.as_str().ok_or_else(|| expected("a string", raw))?;
                names
                    .iter()
                    .find(|name| **name == text)
                    .map(|name| Value::Choice(name))
                    .ok_or_else(|| {
                        let list: Vec<String> =
                            names.iter().map(|name| format!("\"{name}\"")).collect();
                        format!("\"{text}\" is not one of {}", list.join(", "))
                    })
            }
            Kind::Keys => {
                let names = string_list(raw, "a list of key names")?;
                if names.is_empty() {
                    return Err("needs at least one key name".to_owned());
                }
                let mut keys = Vec::with_capacity(names.len());
                for name in names {
                    let key = KeyName::parse(name)
                        .ok_or_else(|| format!("unknown key name \"{name}\""))?;
                    if keys.contains(&key) {
                        return Err(format!("{key} is listed twice"));
                    }
                    keys.push(key);
                }
                Ok(Value::Keys(keys))
            }
            Kind::Buttons => {
                let names = string_list(raw, "a list of button names")?;
                let mut buttons = Vec::with_capacity(names.len());
                for name in names {
                    let button = PadButton::parse(name)
                        .ok_or_else(|| format!("unknown button name \"{name}\""))?;
                    if buttons.contains(&button) {
                        return Err(format!("{button} is listed twice"));
                    }
                    buttons.push(button);
                }
                Ok(Value::Buttons(buttons))
            }
        }
    }

    /// The range or choices in words, for the manual and the comments.
    pub(crate) fn range_text(&self) -> String {
        match self.kind {
            Kind::Integer { min, max } => format!("{min}–{max}"),
            Kind::Float { min, max } => format!("{min}–{max}"),
            Kind::Boolean => "true or false".to_owned(),
            Kind::Choice(names) => names.join(" | "),
            Kind::Keys => "list of key names".to_owned(),
            Kind::Buttons => "list of button names".to_owned(),
        }
    }
}

/// `expected <what>, found <type> <value>`.
fn expected(what: &str, raw: &toml::Value) -> String {
    format!("expected {what}, found {} {raw}", article(raw.type_str()))
}

/// The type name with its indefinite article.
fn article(type_name: &str) -> String {
    if type_name.starts_with(['a', 'e', 'i', 'o', 'u']) {
        format!("an {type_name}")
    } else {
        format!("a {type_name}")
    }
}

/// The strings of a TOML array of strings.
fn string_list<'a>(raw: &'a toml::Value, what: &str) -> Result<Vec<&'a str>, String> {
    let items = raw.as_array().ok_or_else(|| expected(what, raw))?;
    items
        .iter()
        .map(|item| item.as_str().ok_or_else(|| expected(what, raw)))
        .collect()
}

/// The scalar settings, in file order (the `[keys]` and gamepad button
/// entries are added from the action tables).
const SCALARS: &[Entry] = &[
    Entry {
        section: "game",
        name: "speed",
        field: Field::Speed,
        kind: Kind::Integer {
            min: 0,
            max: Speed::MAX as i64,
        },
        default: DefaultValue::Integer(Speed::DEFAULT.get() as i64),
        comment: "0–500; frame time = speed × 2000 µs; 0 = unpaced",
    },
    Entry {
        section: "game",
        name: "mode",
        field: Field::Mode,
        kind: Kind::Choice(&GameMode::NAMES),
        default: DefaultValue::Choice("one-player"),
        comment: "one-player | two-players | two-diggers | gauntlet | two-digger-gauntlet",
    },
    Entry {
        section: "game",
        name: "gauntlet_time",
        field: Field::GauntletTime,
        kind: Kind::Integer { min: 1, max: 3599 },
        default: DefaultValue::Integer(120),
        comment: "seconds, 1–3599",
    },
    Entry {
        section: "game",
        name: "start_level",
        field: Field::StartLevel,
        kind: Kind::Integer { min: 1, max: 1000 },
        default: DefaultValue::Integer(1),
        comment: "1–1000",
    },
    Entry {
        section: "game",
        name: "unlimited_lives",
        field: Field::UnlimitedLives,
        kind: Kind::Boolean,
        default: DefaultValue::Boolean(false),
        comment: "",
    },
    Entry {
        section: "sound",
        name: "enabled",
        field: Field::SoundEnabled,
        kind: Kind::Boolean,
        default: DefaultValue::Boolean(true),
        comment: "",
    },
    Entry {
        section: "sound",
        name: "music",
        field: Field::Music,
        kind: Kind::Boolean,
        default: DefaultValue::Boolean(true),
        comment: "",
    },
    Entry {
        section: "sound",
        name: "sample_rate",
        field: Field::SampleRate,
        kind: Kind::Integer {
            min: 8000,
            max: 192_000,
        },
        default: DefaultValue::Integer(44_100),
        comment: "Hz, 8000–192000",
    },
    Entry {
        section: "sound",
        name: "buffer_size",
        field: Field::BufferSize,
        kind: Kind::Integer { min: 64, max: 8192 },
        default: DefaultValue::Integer(512),
        comment: "frames, 64–8192",
    },
    Entry {
        section: "sound",
        name: "volume",
        field: Field::Volume,
        kind: Kind::Integer { min: 0, max: 100 },
        default: DefaultValue::Integer(100),
        comment: "percent, 0–100",
    },
    Entry {
        section: "video",
        name: "fullscreen",
        field: Field::Fullscreen,
        kind: Kind::Boolean,
        default: DefaultValue::Boolean(false),
        comment: "",
    },
    Entry {
        section: "video",
        name: "scale",
        field: Field::Scale,
        kind: Kind::Choice(&ScaleMode::NAMES),
        default: DefaultValue::Choice("integer"),
        comment: "integer | fit | stretch",
    },
    Entry {
        section: "video",
        name: "reduce_flashing",
        field: Field::ReduceFlashing,
        kind: Kind::Boolean,
        default: DefaultValue::Boolean(false),
        comment: "true: the initials screen does not flash",
    },
];

/// The scalar `[gamepad]` settings, written before the button lists.
const PAD_SCALARS: &[Entry] = &[
    Entry {
        section: "gamepad",
        name: "enabled",
        field: Field::PadEnabled,
        kind: Kind::Boolean,
        default: DefaultValue::Boolean(true),
        comment: "",
    },
    Entry {
        section: "gamepad",
        name: "dead_zone",
        field: Field::DeadZone,
        kind: Kind::Float {
            min: 0.05,
            max: 0.95,
        },
        default: DefaultValue::Float(0.5),
        comment: "fraction of full stick deflection, 0.05–0.95",
    },
];

/// Every setting, in file order.
static SCHEMA: LazyLock<Vec<Entry>> = LazyLock::new(|| {
    let keys = Action::ALL.into_iter().map(|action| Entry {
        section: "keys",
        name: action.name(),
        field: Field::Key(action),
        kind: Kind::Keys,
        default: DefaultValue::Keys(action.default_key_names()),
        comment: "",
    });
    let buttons = PadAction::ALL.into_iter().map(|action| Entry {
        section: "gamepad",
        name: action.name(),
        field: Field::PadButtons(action),
        kind: Kind::Buttons,
        default: DefaultValue::Buttons(action.default_button_names()),
        comment: "",
    });
    SCALARS
        .iter()
        .copied()
        .chain(keys)
        .chain(PAD_SCALARS.iter().copied())
        .chain(buttons)
        .collect()
});

/// Every setting of the configuration file, in file order.
pub(crate) fn schema() -> &'static [Entry] {
    &SCHEMA
}

/// The entry for `section.name`, if there is one.
pub(crate) fn find(section: &str, name: &str) -> Option<&'static Entry> {
    schema()
        .iter()
        .find(|entry| entry.section == section && entry.name == name)
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::{DefaultValue, Kind, SECTIONS, Value, find, schema};
    use crate::config::{GameMode, Layer, ScaleMode, Settings};
    use crate::input::{Action, KeyName, PadAction, PadButton};

    /// Every default of the `configuration` spec, by key path.
    #[test]
    fn schema_defaults_match_spec() {
        let settings = Settings::default();
        assert_eq!(settings.speed.get(), 40);
        assert_eq!(settings.speed.frame_time().as_micros(), 80_000);
        assert_eq!(settings.mode, GameMode::OnePlayer);
        assert_eq!(settings.gauntlet_time, 120);
        assert_eq!(settings.start_level, 1);
        assert!(!settings.unlimited_lives);
        assert!(settings.sound_enabled);
        assert!(settings.music);
        assert_eq!(settings.sample_rate, 44_100);
        assert_eq!(settings.buffer_size, 512);
        assert_eq!(settings.volume, 100);
        assert!(!settings.fullscreen);
        assert_eq!(settings.scale, ScaleMode::Integer);
        assert!(!settings.reduce_flashing);
        assert!(settings.pad_enabled);
        assert!((settings.dead_zone - 0.5).abs() < f64::EPSILON);
        assert_eq!(
            settings.buttons_of(PadAction::Fire),
            [PadButton::South, PadButton::West]
        );
        assert_eq!(settings.buttons_of(PadAction::Pause), [PadButton::Start]);
        assert_eq!(
            settings.buttons_of(PadAction::ChangeMode),
            [PadButton::Select]
        );
        assert_eq!(settings.buttons_of(PadAction::Quit), [] as [PadButton; 0]);
        for action in Action::ALL {
            let names: Vec<&str> = settings
                .keys_of(action)
                .iter()
                .map(|key| key.name())
                .collect();
            assert_eq!(names, action.default_key_names(), "{action}");
        }
    }

    /// Every range and choice list of the `configuration` spec.
    #[test]
    fn schema_ranges_match_spec() {
        let range = |path: &str| {
            let (section, name) = path.split_once('.').unwrap_or_default();
            find(section, name).map(|entry| entry.kind)
        };
        assert_eq!(
            range("game.speed"),
            Some(Kind::Integer { min: 0, max: 500 })
        );
        assert_eq!(
            range("game.gauntlet_time"),
            Some(Kind::Integer { min: 1, max: 3599 })
        );
        assert_eq!(
            range("game.start_level"),
            Some(Kind::Integer { min: 1, max: 1000 })
        );
        assert_eq!(
            range("sound.sample_rate"),
            Some(Kind::Integer {
                min: 8000,
                max: 192_000
            })
        );
        assert_eq!(
            range("sound.buffer_size"),
            Some(Kind::Integer { min: 64, max: 8192 })
        );
        assert_eq!(
            range("sound.volume"),
            Some(Kind::Integer { min: 0, max: 100 })
        );
        assert_eq!(
            range("gamepad.dead_zone"),
            Some(Kind::Float {
                min: 0.05,
                max: 0.95
            })
        );
        assert_eq!(
            range("game.mode"),
            Some(Kind::Choice(&[
                "one-player",
                "two-players",
                "two-diggers",
                "gauntlet",
                "two-digger-gauntlet"
            ]))
        );
        assert_eq!(
            range("video.scale"),
            Some(Kind::Choice(&["integer", "fit", "stretch"]))
        );

        // 13 scalars + 20 actions + 2 pad scalars + 4 button lists.
        assert_eq!(schema().len(), 39);
        // Paths are unique, sections are known and grouped in file order.
        let mut last_section = 0;
        for (index, entry) in schema().iter().enumerate() {
            let section = SECTIONS
                .iter()
                .position(|s| *s == entry.section)
                .unwrap_or(usize::MAX);
            assert!(section >= last_section && section < SECTIONS.len());
            last_section = section;
            assert!(
                schema()
                    .iter()
                    .skip(index + 1)
                    .all(|other| other.path() != entry.path()),
                "{} twice",
                entry.path()
            );
        }
    }

    /// Each default passes its own check, so the default layer is
    /// complete.
    #[test]
    fn schema_defaults_are_valid() {
        for entry in schema() {
            let raw = match entry.default {
                DefaultValue::Integer(v) => toml::Value::Integer(v),
                DefaultValue::Float(v) => toml::Value::Float(v),
                DefaultValue::Boolean(v) => toml::Value::Boolean(v),
                DefaultValue::Choice(name) => toml::Value::String(name.to_owned()),
                DefaultValue::Keys(names) | DefaultValue::Buttons(names) => toml::Value::Array(
                    names
                        .iter()
                        .map(|n| toml::Value::String((*n).to_owned()))
                        .collect(),
                ),
            };
            assert_eq!(
                entry.check(&raw),
                Ok(entry.default_value()),
                "{}",
                entry.path()
            );
        }
        let defaults = Layer::defaults();
        assert!(defaults.keys.iter().all(Option::is_some));
        assert!(defaults.pad_buttons.iter().all(Option::is_some));
        assert!(defaults.speed.is_some() && defaults.dead_zone.is_some());
    }

    #[test]
    fn value_text() {
        assert_eq!(Value::Integer(40).to_string(), "40");
        assert_eq!(Value::Float(0.5).to_string(), "0.5");
        assert_eq!(Value::Float(1.0).to_string(), "1.0");
        assert_eq!(Value::Boolean(false).to_string(), "false");
        assert_eq!(Value::Choice("fit").to_string(), "\"fit\"");
        let keys = ["NumpadAdd", "Equal"]
            .iter()
            .filter_map(|n| KeyName::parse(n))
            .collect();
        assert_eq!(Value::Keys(keys).to_string(), "[\"NumpadAdd\", \"Equal\"]");
        assert_eq!(Value::Buttons(Vec::new()).to_string(), "[]");
    }

    #[test]
    fn range_texts() {
        let text = |section, name| find(section, name).map(super::Entry::range_text);
        assert_eq!(text("sound", "sample_rate").as_deref(), Some("8000–192000"));
        assert_eq!(text("gamepad", "dead_zone").as_deref(), Some("0.05–0.95"));
        assert_eq!(
            text("video", "scale").as_deref(),
            Some("integer | fit | stretch")
        );
        assert_eq!(
            text("video", "fullscreen").as_deref(),
            Some("true or false")
        );
        assert_eq!(text("keys", "quit").as_deref(), Some("list of key names"));
        assert_eq!(
            text("gamepad", "quit").as_deref(),
            Some("list of button names")
        );
    }
}
