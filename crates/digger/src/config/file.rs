//! Reading the TOML configuration file by walking its table with key paths,
//! writing it and saving it atomically (design D9).
//!
//! The file is parsed into a plain TOML table and walked against the
//! schema, so that every error names the file, the full key path and the
//! offending value, and unknown settings are only warned about.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "reading and saving are wired into main and the shell by later task groups"
    )
)]

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::schema::{self, SECTIONS};
use super::{Layer, Settings};
use crate::error::AppError;
use crate::input::{Action, PadAction};

/// The first line of every file the program writes.
pub(crate) const HEADER: &str =
    "# Digger settings, written by the game. Comments added by hand are not kept.";

/// What a configuration file contributed: its settings and the warnings
/// about settings it holds that do not exist.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct FileLayer {
    /// The settings the file gives.
    pub(crate) layer: Layer,
    /// One line per ignored setting, already naming the file.
    pub(crate) warnings: Vec<String>,
}

/// Reads the configuration file at `path`. A file that does not exist is
/// not an error: the result is `None` and the defaults apply.
///
/// # Errors
///
/// [`AppError::Config`] (status 78) for a file that cannot be read, is not
/// valid TOML or holds a bad value.
pub(crate) fn read(path: &Path) -> Result<Option<FileLayer>, AppError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(config_error(path, None, format!("cannot be read: {error}")));
        }
    };
    let text = String::from_utf8(bytes)
        .map_err(|_| config_error(path, None, "is not UTF-8 text".to_owned()))?;
    parse(&text, path).map(Some)
}

/// Parses the text of a configuration file; `path` is used in messages.
///
/// # Errors
///
/// [`AppError::Config`] naming the file and, where there is one, the key.
pub(crate) fn parse(text: &str, path: &Path) -> Result<FileLayer, AppError> {
    let table: toml::Table = text.parse().map_err(|error: toml::de::Error| {
        let message = error.to_string();
        let message = message.trim_end();
        config_error(path, None, format!("not valid TOML: {message}"))
    })?;
    let mut result = FileLayer::default();
    for (section, value) in &table {
        if !SECTIONS.contains(&section.as_str()) {
            result.warnings.push(format!(
                "{}: unknown setting {section}, ignored",
                path.display()
            ));
            continue;
        }
        let Some(entries) = value.as_table() else {
            return Err(config_error(
                path,
                Some(section.clone()),
                format!("expected a table, found {}", value.type_str()),
            ));
        };
        for (name, raw) in entries {
            let Some(entry) = schema::find(section, name) else {
                result.warnings.push(format!(
                    "{}: unknown setting {section}.{name}, ignored",
                    path.display()
                ));
                continue;
            };
            let value = entry
                .check(raw)
                .map_err(|problem| config_error(path, Some(entry.path()), problem))?;
            result.layer.set(entry.field, value);
        }
    }
    check_bindings(&result.layer, path)?;
    Ok(result)
}

/// Refuses a key (or gamepad button) that the file, together with the
/// defaults of the actions it leaves out, binds to two actions.
fn check_bindings(layer: &Layer, path: &Path) -> Result<(), AppError> {
    let mut settings = Settings::default();
    settings.apply(layer);
    if let Some((key, first, second)) = settings.duplicate_key() {
        // Point at an action the file sets: the later one if it does.
        let named: Action = if layer.sets_keys(second) {
            second
        } else {
            first
        };
        return Err(config_error(
            path,
            Some(format!("keys.{named}")),
            format!("{key} is bound to both {first} and {second}"),
        ));
    }
    if let Some((button, first, second)) = settings.duplicate_button() {
        let named: PadAction = if layer.sets_buttons(second) {
            second
        } else {
            first
        };
        return Err(config_error(
            path,
            Some(format!("gamepad.{named}")),
            format!("{button} is bound to both {first} and {second}"),
        ));
    }
    Ok(())
}

/// An [`AppError::Config`] for `path`.
fn config_error(path: &Path, key: Option<String>, problem: String) -> AppError {
    AppError::Config {
        file: path.to_path_buf(),
        key,
        problem,
    }
}

/// The complete text of a configuration file holding `settings`: the
/// header, then every setting of the schema in order, each table
/// separated by a blank line.
pub(crate) fn render(settings: &Settings) -> String {
    let mut text = String::from(HEADER);
    text.push('\n');
    let mut section = "";
    for entry in schema::schema() {
        if entry.section != section {
            section = entry.section;
            text.push_str("\n[");
            text.push_str(section);
            text.push_str("]\n");
        }
        let line = format!("{} = {}", entry.name, settings.value(entry.field));
        text.push_str(&line);
        if !entry.comment.is_empty() {
            text.push_str("  # ");
            text.push_str(entry.comment);
        }
        text.push('\n');
    }
    text
}

/// Why a save did not happen.
#[derive(Debug)]
pub(crate) enum SaveError {
    /// The file on disk has become unusable since the program started;
    /// it is left as it is.
    Invalid(AppError),
    /// The directory or the file cannot be written.
    Io {
        /// The path that could not be written.
        path: PathBuf,
        /// What went wrong.
        source: io::Error,
    },
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(error) => write!(f, "settings not saved: {error}"),
            Self::Io { path, source } => {
                write!(f, "settings not saved: {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for SaveError {}

/// Saves the settings the player changed at run time (`changes`) into the
/// file at `path` (`configuration` spec, saving settings): the file is read
/// again (or the defaults taken if there is none), the changes are applied
/// on top and the complete file is written to `<name>.tmp` beside it and
/// renamed over it. The directory is created if needed. Command-line values
/// never reach this function, so they are never written.
///
/// Returns whether the file was written: with no changes, or when the new
/// text equals the file's, nothing is touched.
///
/// # Errors
///
/// [`SaveError`]; the caller reports it as a notice.
pub(crate) fn save(path: &Path, changes: &Layer) -> Result<bool, SaveError> {
    if changes.is_empty() {
        return Ok(false);
    }
    let on_disk = read(path).map_err(SaveError::Invalid)?;
    let mut settings = Settings::default();
    if let Some(file) = &on_disk {
        settings.apply(&file.layer);
    }
    settings.apply(changes);
    let text = render(&settings);
    if on_disk.is_some() && fs::read(path).is_ok_and(|old| old == text.as_bytes()) {
        return Ok(false);
    }
    write_atomic(path, text.as_bytes())?;
    Ok(true)
}

/// Writes `bytes` to `<path>.tmp` and renames it over `path`, creating the
/// directory first.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    let io_error = |path: &Path| {
        let path = path.to_path_buf();
        move |source| SaveError::Io { path, source }
    };
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(io_error(dir))?;
    }
    let tmp = temporary_path(path);
    fs::write(&tmp, bytes).map_err(io_error(&tmp))?;
    if let Err(source) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(SaveError::Io {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(())
}

/// `config.toml` → `config.toml.tmp`, in the same directory.
pub(crate) fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map_or_else(|| OsString::from("config.toml"), OsString::from);
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
pub(crate) mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{HEADER, parse, read, render, save, temporary_path};
    use crate::config::schema::schema;
    use crate::config::{GameMode, Layer, ScaleMode, Settings, Speed};
    use crate::error::AppError;
    use crate::input::{Action, KeyName};

    /// A fresh, empty directory under the system temporary directory,
    /// removed when dropped.
    #[derive(Debug)]
    pub(crate) struct TempDir(PathBuf);

    impl TempDir {
        pub(crate) fn new(label: &str) -> Self {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("digger-test-{}-{label}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            Self(path)
        }

        pub(crate) fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn file() -> &'static Path {
        Path::new("config.toml")
    }

    /// The `(key, problem)` of a configuration error, or a panic.
    fn config_error(text: &str) -> (Option<String>, String) {
        match parse(text, file()) {
            Err(AppError::Config { file, key, problem }) => {
                assert_eq!(file, Path::new("config.toml"));
                (key, problem)
            }
            other => panic!("expected a configuration error, got {other:?}"),
        }
    }

    fn parsed(text: &str) -> super::FileLayer {
        parse(text, file()).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn config_reads_every_table() {
        let result = parsed(
            "[game]\nspeed = 20\nmode = \"gauntlet\"\ngauntlet_time = 90\n\
             [sound]\nvolume = 50\n[video]\nscale = \"stretch\"\n\
             [keys]\np1_fire = [\"ControlLeft\", \"F1\"]\n\
             [gamepad]\nenabled = false\ndead_zone = 0.3\nquit = [\"Mode\"]\n",
        );
        assert_eq!(result.warnings, Vec::<String>::new());
        let settings = Settings::resolve(&result.layer, &Layer::default());
        assert_eq!(settings.speed.frame_time().as_micros(), 40_000);
        assert_eq!(settings.mode, GameMode::Gauntlet);
        assert_eq!(settings.gauntlet_time, 90);
        assert_eq!(settings.volume, 50);
        assert_eq!(settings.scale, ScaleMode::Stretch);
        let fire: Vec<&str> = settings
            .keys_of(Action::P1Fire)
            .iter()
            .map(|k| k.name())
            .collect();
        assert_eq!(fire, ["ControlLeft", "F1"]);
        // Other actions keep their defaults.
        assert_eq!(
            settings.keys_of(Action::P1Up),
            Action::P1Up.default_keys().as_slice()
        );
        assert!(!settings.pad_enabled);
        assert!((settings.dead_zone - 0.3).abs() < 1e-12);
        // A whole number is checked against a fraction's range too.
        assert!(parse("[gamepad]\ndead_zone = 0\n", file()).is_err());
        assert!(parse("", file()).is_ok_and(|r| r.layer.is_empty()));
    }

    #[test]
    fn config_out_of_range_names_key() {
        let (key, problem) = config_error("[game]\nstart_level = 0\n");
        assert_eq!(key.as_deref(), Some("game.start_level"));
        assert_eq!(problem, "0 is outside 1–1000");
        let (key, problem) = config_error("[sound]\nsample_rate = 999\n");
        assert_eq!(key.as_deref(), Some("sound.sample_rate"));
        // The message of the spec's example.
        let error = AppError::Config {
            file: file().to_path_buf(),
            key,
            problem,
        };
        assert_eq!(
            error.to_string(),
            "config.toml: sound.sample_rate: 999 is outside 8000–192000"
        );
        assert_eq!(error.exit_code(), 78);
        let (key, problem) = config_error("[gamepad]\ndead_zone = 0.99\n");
        assert_eq!(key.as_deref(), Some("gamepad.dead_zone"));
        assert_eq!(problem, "0.99 is outside 0.05–0.95");
        let (key, _) = config_error("[game]\nspeed = 501\n");
        assert_eq!(key.as_deref(), Some("game.speed"));
        // Wrong types.
        let (key, problem) = config_error("[game]\nspeed = \"fast\"\n");
        assert_eq!(key.as_deref(), Some("game.speed"));
        assert_eq!(problem, "expected a whole number, found a string \"fast\"");
        let (_, problem) = config_error("[game]\nspeed = 40.0\n");
        assert!(problem.starts_with("expected a whole number"), "{problem}");
        let (key, problem) = config_error("[video]\nfullscreen = 1\n");
        assert_eq!(key.as_deref(), Some("video.fullscreen"));
        assert_eq!(problem, "expected true or false, found an integer 1");
        let (key, problem) = config_error("[game]\nmode = \"solo\"\n");
        assert_eq!(key.as_deref(), Some("game.mode"));
        assert!(problem.starts_with("\"solo\" is not one of \"one-player\""));
        let (key, problem) = config_error("game = 5\n");
        assert_eq!(key.as_deref(), Some("game"));
        assert_eq!(problem, "expected a table, found integer");
        // Unknown key and button names are errors, not warnings.
        let (key, problem) = config_error("[keys]\np1_fire = [\"Ctrl\"]\n");
        assert_eq!(key.as_deref(), Some("keys.p1_fire"));
        assert_eq!(problem, "unknown key name \"Ctrl\"");
        let (key, problem) = config_error("[gamepad]\nfire = [\"A\"]\n");
        assert_eq!(key.as_deref(), Some("gamepad.fire"));
        assert_eq!(problem, "unknown button name \"A\"");
        let (_, problem) = config_error("[keys]\np1_fire = []\n");
        assert_eq!(problem, "needs at least one key name");
        let (_, problem) = config_error("[keys]\np1_fire = \"F1\"\n");
        assert!(problem.starts_with("expected a list of key names"));
        let (_, problem) = config_error("[keys]\np1_fire = [\"F1\", \"F1\"]\n");
        assert_eq!(problem, "F1 is listed twice");
        // An empty button list is allowed (quit has none by default).
        assert!(parse("[gamepad]\nfire = []\n", file()).is_ok());
    }

    #[test]
    fn config_duplicate_binding() {
        let (key, problem) = config_error("[keys]\npause = [\"Space\"]\np1_fire = [\"Space\"]\n");
        assert_eq!(key.as_deref(), Some("keys.pause"));
        assert_eq!(problem, "Space is bound to both p1_fire and pause");
        // The default of an action the file leaves out counts too.
        let (key, problem) = config_error("[keys]\np1_fire = [\"Space\"]\n");
        assert_eq!(key.as_deref(), Some("keys.p1_fire"));
        assert!(problem.contains("Space") && problem.contains("pause"));
        assert!(problem.contains("p1_fire"));
        // Moving the default away first is fine.
        assert!(
            parse(
                "[keys]\np1_fire = [\"Space\"]\npause = [\"KeyP\"]\n",
                file()
            )
            .is_ok()
        );
        // Pad buttons likewise.
        let (key, problem) = config_error("[gamepad]\nquit = [\"Start\"]\n");
        assert_eq!(key.as_deref(), Some("gamepad.quit"));
        assert_eq!(problem, "Start is bound to both pause and quit");
    }

    #[test]
    fn config_unknown_key_warns() {
        let result = parsed("colour = 3\n[game]\nturbo = true\nspeed = 30\n[extras]\nx = 1\n");
        assert_eq!(result.layer.speed, Speed::new(30));
        assert_eq!(
            result.warnings,
            [
                "config.toml: unknown setting colour, ignored",
                "config.toml: unknown setting extras, ignored",
                "config.toml: unknown setting game.turbo, ignored",
            ]
        );
        // An unknown action name in [keys] is an unknown setting.
        let result = parsed("[keys]\ncheat = [\"KeyC\"]\n");
        assert_eq!(
            result.warnings,
            ["config.toml: unknown setting keys.cheat, ignored"]
        );
    }

    #[test]
    fn config_invalid_toml() {
        let (key, problem) = config_error("[game\nspeed = 40\n");
        assert_eq!(key, None);
        assert!(problem.starts_with("not valid TOML: "), "{problem}");
        let (key, _) = config_error("[game]\nspeed = 40\nspeed = 41\n");
        assert_eq!(key, None);
        let error = parse("speed = = 1", file()).err();
        assert!(
            error
                .is_some_and(|e| e.exit_code() == 78 && e.to_string().starts_with("config.toml: "))
        );
        // A file that is not text.
        let dir = TempDir::new("binary");
        let path = dir.path().join("config.toml");
        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap_or_else(|e| panic!("{e}"));
        assert!(read(&path).is_err_and(|e| e.exit_code() == 78));
    }

    #[test]
    fn missing_file_is_defaults() {
        let dir = TempDir::new("missing");
        let path = dir.path().join("config.toml");
        assert!(read(&path).is_ok_and(|r| r.is_none()));
        assert!(!path.exists());
    }

    #[test]
    fn rendered_file_round_trips() {
        let mut settings = Settings {
            music: false,
            dead_zone: 0.25,
            ..Settings::default()
        };
        settings.keys[Action::P1Fire.index()] = ["ControlLeft", "F1"]
            .iter()
            .filter_map(|n| KeyName::parse(n))
            .collect();
        let text = render(&settings);
        assert!(text.starts_with(HEADER));
        let read_back = parsed(&text);
        assert_eq!(read_back.warnings, Vec::<String>::new());
        let mut again = Settings::default();
        again.apply(&read_back.layer);
        assert_eq!(again, settings);
        // Every setting is written.
        assert_eq!(read_back.layer, {
            let mut full = Layer::defaults();
            full.music = Some(false);
            full.dead_zone = Some(0.25);
            full.keys[Action::P1Fire.index()] = Some(settings.keys_of(Action::P1Fire).to_vec());
            full
        });
    }

    #[test]
    fn first_save_writes_all_keys() {
        let dir = TempDir::new("first-save");
        let path = dir.path().join("nested").join("digger").join("config.toml");
        let changes = Layer {
            music: Some(false),
            ..Layer::default()
        };
        assert!(save(&path, &changes).is_ok_and(|written| written));
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(text.starts_with(HEADER));
        for entry in schema() {
            assert!(
                text.contains(&format!("\n{} = ", entry.name)),
                "{} missing",
                entry.path()
            );
        }
        assert!(text.contains("[sound]\nenabled = true\nmusic = false\n"));
        assert!(text.contains("speed = 40"));
        assert!(!temporary_path(&path).exists());
        let layer = read(&path)
            .unwrap_or_else(|e| panic!("{e}"))
            .map(|f| f.layer);
        let expected = {
            let mut full = Layer::defaults();
            full.music = Some(false);
            full
        };
        assert_eq!(layer, Some(expected));
    }

    #[test]
    fn cli_values_not_saved() {
        let dir = TempDir::new("cli-not-saved");
        let path = dir.path().join("config.toml");
        fs::write(&path, "[game]\nspeed = 25\n[sound]\nenabled = true\n")
            .unwrap_or_else(|e| panic!("{e}"));
        // Started with `--speed 10 --quiet`: those are a command-line layer
        // that the program uses but never hands to `save`.
        let cli = Layer {
            speed: Speed::new(10),
            sound_enabled: Some(false),
            ..Layer::default()
        };
        let file_layer = read(&path)
            .unwrap_or_else(|e| panic!("{e}"))
            .map(|f| f.layer)
            .unwrap_or_default();
        let running = Settings::resolve(&file_layer, &cli);
        assert_eq!(running.speed.get(), 10);
        assert!(!running.sound_enabled);
        // The player toggles fullscreen.
        let changes = Layer {
            fullscreen: Some(true),
            ..Layer::default()
        };
        assert!(save(&path, &changes).is_ok_and(|written| written));
        let saved = read(&path)
            .unwrap_or_else(|e| panic!("{e}"))
            .map(|f| f.layer)
            .unwrap_or_default();
        assert_eq!(saved.fullscreen, Some(true));
        assert_eq!(saved.speed, Speed::new(25));
        assert_eq!(saved.sound_enabled, Some(true));
    }

    #[test]
    fn no_change_no_write() {
        let dir = TempDir::new("no-change");
        let path = dir.path().join("config.toml");
        // No file and no change: nothing is created.
        assert!(save(&path, &Layer::default()).is_ok_and(|written| !written));
        assert!(!path.exists());
        // An existing file is left byte for byte, with its comments.
        let original = "# my settings\n[game]\nspeed = 30\n";
        fs::write(&path, original).unwrap_or_else(|e| panic!("{e}"));
        assert!(save(&path, &Layer::default()).is_ok_and(|written| !written));
        assert_eq!(fs::read_to_string(&path).ok().as_deref(), Some(original));
        // A change that gives the text already on disk writes nothing.
        let changes = Layer {
            music: Some(false),
            ..Layer::default()
        };
        assert!(save(&path, &changes).is_ok_and(|written| written));
        let first = fs::read(&path).unwrap_or_default();
        assert!(save(&path, &changes).is_ok_and(|written| !written));
        assert_eq!(fs::read(&path).unwrap_or_default(), first);
    }

    #[test]
    fn save_keeps_a_broken_file() {
        let dir = TempDir::new("broken");
        let path = dir.path().join("config.toml");
        fs::write(&path, "[game\n").unwrap_or_else(|e| panic!("{e}"));
        let changes = Layer {
            music: Some(false),
            ..Layer::default()
        };
        assert!(save(&path, &changes).is_err());
        assert_eq!(fs::read_to_string(&path).ok().as_deref(), Some("[game\n"));
    }

    #[test]
    fn temporary_name() {
        assert_eq!(
            temporary_path(Path::new("/a/b/config.toml")),
            Path::new("/a/b/config.toml.tmp")
        );
        assert_eq!(
            temporary_path(Path::new("test.toml")),
            Path::new("test.toml.tmp")
        );
    }
}
