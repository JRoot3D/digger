//! The command line: the clap derive struct `Cli` and its conversion to a
//! settings layer and the program's options (design D8).
//!
//! The arguments first go through the upstream alias pre-pass
//! ([`legacy::translate`]); clap then parses long options only (plus `-h`
//! and `-V`) and one optional bare argument. Every error is a usage error
//! with exit status 64, never clap's own status 2 (which means "diverged"
//! in verification runs), and quotes options in the form the user typed.

#[cfg(not(target_arch = "wasm32"))]
use std::ffi::OsString;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

// The web build has no command line: only [`Redefine`] is shared with it
// until task 2.6 of add-web-build-and-releases splits the launch options
// (temporary gates of task 1.2).
#[cfg(not(target_arch = "wasm32"))]
use clap::error::{ContextKind, ContextValue, ErrorKind};
#[cfg(not(target_arch = "wasm32"))]
use clap::{ArgAction, ColorChoice, Parser};

#[cfg(not(target_arch = "wasm32"))]
use crate::config::{GameMode, Layer, ModeConflict, ScaleMode, Speed};
#[cfg(not(target_arch = "wasm32"))]
use crate::error::AppError;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod legacy;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod levels;
#[cfg(test)]
mod manual_tests;

#[cfg(not(target_arch = "wasm32"))]
use legacy::Translated;

#[cfg(not(target_arch = "wasm32"))]
/// Digger Remastered, re-created in Rust.
#[derive(Debug, Parser)]
#[command(
    name = "digger",
    version,
    about = "Digger Remastered, re-created in Rust.",
    disable_help_flag = true,
    disable_version_flag = true,
    args_override_self = true,
    color = ColorChoice::Never,
    after_help = "Upstream options are also accepted with '/' or '-', in either case, \
                  value after ':' (/S:40, -s40). /V is accepted and ignored. \
                  /C, /N and /X are not supported."
)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the struct mirrors the command line, whose switches are booleans"
)]
struct Cli {
    /// Game speed 0–500: frame time N × 2000 µs, 0 = as fast as possible
    /// [default: 40] [alias: /S:N]
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u16).range(0..=500))]
    speed: Option<u16>,

    /// Level games start on, 1–1000 [alias: /I:N]
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..=1000))]
    start_level: Option<u16>,

    /// Lives are never used up [alias: /U]
    #[arg(long)]
    unlimited_lives: bool,

    /// Number of players taking turns, 1 or 2
    #[arg(long, value_name = "1|2", value_parser = clap::value_parser!(u8).range(1..=2))]
    players: Option<u8>,

    /// Two players at once, two Diggers [alias: /2]
    #[arg(long)]
    two_diggers: bool,

    /// Gauntlet: play against the clock, 1–3599 seconds; without a value
    /// or with 0, the configured time [alias: /G[:SECS]]
    #[arg(
        long,
        value_name = "SECS",
        num_args = 0..=1,
        require_equals = true,
        value_parser = clap::value_parser!(u16).range(0..=3599)
    )]
    #[expect(
        clippy::option_option,
        reason = "clap's form of an option whose value may be left out"
    )]
    gauntlet: Option<Option<u16>>,

    /// Start with all sound off [alias: /Q]
    #[arg(long)]
    quiet: bool,

    /// Start with music off [alias: /M]
    #[arg(long)]
    no_music: bool,

    /// Start fullscreen [alias: /F]
    #[arg(long)]
    fullscreen: bool,

    /// How the picture fills the window
    #[arg(
        long,
        value_name = "MODE",
        value_parser = clap::builder::PossibleValuesParser::new(ScaleMode::NAMES)
    )]
    scale: Option<String>,

    /// Redefine the keys before the title screen; =all asks for every
    /// action [alias: /K, /KA]
    #[arg(
        long,
        value_name = "all",
        num_args = 0..=1,
        require_equals = true,
        value_parser = ["all"]
    )]
    #[expect(
        clippy::option_option,
        reason = "clap's form of an option whose value may be left out"
    )]
    redefine_keys: Option<Option<String>>,

    /// Load the eight levels from a level file (.dlf is added if the file
    /// does not exist) [alias: /L:FILE]
    #[arg(long, value_name = "FILE")]
    levels: Option<PathBuf>,

    /// Save every game played to FILE; a name ending in .edrf selects the
    /// extended format [alias: /R:FILE]
    #[arg(long, value_name = "FILE")]
    record: Option<PathBuf>,

    /// Play a recording before the title screen; may be given several
    /// times [alias: /P:FILE]
    #[arg(long, value_name = "FILE", action = ArgAction::Append)]
    play: Vec<PathBuf>,

    /// Repeat the --play list until quit [alias: /O]
    #[arg(long = "loop")]
    loop_playback: bool,

    /// Hand the game played back to the player after this tick (with
    /// --play) [alias: /T:N]
    #[arg(long, value_name = "N")]
    take_over_at: Option<u32>,

    /// Play one recording and exit with 0 if it agrees, 1 unreadable, 2
    /// diverged, 3 stopped, 4 truncated [alias: /E:FILE]
    #[arg(long, value_name = "FILE")]
    verify: Option<PathBuf>,

    /// Verify without a window or sound, as fast as possible (with
    /// --verify)
    #[arg(long)]
    headless: bool,

    /// Read settings from FILE instead of the configuration directory
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Keep scores and recordings in DIR instead of the data directory
    #[arg(long, value_name = "DIR")]
    data_dir: Option<PathBuf>,

    /// Print help [aliases: /?, /H]
    #[arg(short = 'h', long, action = ArgAction::Help)]
    help: Option<bool>,

    /// Print the version
    #[arg(short = 'V', long, action = ArgAction::Version)]
    version: Option<bool>,

    /// A number is the speed (as --speed); anything else is a level file
    /// (as --levels)
    #[arg(value_name = "SPEED|LEVELS")]
    bare: Option<OsString>,
}

/// How the key redefinition opens before the title screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        dead_code,
        reason = "temporary (task 1.2): only the desktop command line makes one"
    )
)]
pub(crate) enum Redefine {
    /// The usual list of actions.
    Usual,
    /// Every action (`--redefine-keys=all`, `/KA`).
    All,
}

#[cfg(not(target_arch = "wasm32"))]
/// What the program was asked to do, apart from the settings.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Options {
    /// The settings the command line gives (`configuration` spec,
    /// precedence). Its mode is the command line's as a whole.
    pub(crate) layer: Layer,
    /// `--config`.
    pub(crate) config: Option<PathBuf>,
    /// `--data-dir`.
    pub(crate) data_dir: Option<PathBuf>,
    /// `--levels`, or a bare argument that is not a number.
    pub(crate) levels: Option<PathBuf>,
    /// `--record`.
    pub(crate) record: Option<PathBuf>,
    /// `--play`, in the order given.
    pub(crate) play: Vec<PathBuf>,
    /// `--loop`.
    pub(crate) loop_playback: bool,
    /// `--take-over-at`.
    pub(crate) take_over_at: Option<u32>,
    /// `--verify`.
    pub(crate) verify: Option<PathBuf>,
    /// `--headless`.
    pub(crate) headless: bool,
    /// `--redefine-keys[=all]`.
    pub(crate) redefine_keys: Option<Redefine>,
}

#[cfg(not(target_arch = "wasm32"))]
/// The outcome of parsing the command line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Command {
    /// Run the program with these options.
    Run(Box<Options>),
    /// Print this text on standard output and exit with status 0 (help
    /// or version).
    Print(String),
}

#[cfg(not(target_arch = "wasm32"))]
/// Parses the command line `args` (program name first).
///
/// # Errors
///
/// [`AppError::Usage`] (status 64) for an unknown or unsupported option, a
/// bad value or a forbidden combination; the message quotes options as the
/// user typed them.
pub(crate) fn parse<I>(args: I) -> Result<Command, AppError>
where
    I: IntoIterator,
    I::Item: Into<OsString>,
{
    let translated = legacy::translate(args).map_err(|e| AppError::Usage(e.to_string()))?;
    let cli = match Cli::try_parse_from(&translated.args) {
        Ok(cli) => cli,
        Err(error) => {
            return match error.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                    Ok(Command::Print(error.to_string()))
                }
                _ => Err(usage_error(&error, &translated)),
            };
        }
    };
    let options = convert(cli, &translated)?;
    Ok(Command::Run(Box::new(options)))
}

#[cfg(not(target_arch = "wasm32"))]
/// A clap error as a usage error, with a long option that came from an
/// upstream alias replaced by the alias as typed.
fn usage_error(error: &clap::Error, translated: &Translated) -> AppError {
    let mut text = error.to_string();
    if let Some(ContextValue::String(arg)) = error.get(ContextKind::InvalidArg) {
        let long = arg.split([' ', '=', '[']).next().unwrap_or(arg);
        let value = match error.get(ContextKind::InvalidValue) {
            Some(ContextValue::String(value)) => Some(value.as_str()),
            _ => None,
        };
        let spelling = translated.spellings.iter().find(|spelling| {
            spelling.long == long
                && value.is_none_or(|value| {
                    spelling
                        .value
                        .as_ref()
                        .map_or(value.is_empty(), |given| given.to_string_lossy() == value)
                })
        });
        if let Some(spelling) = spelling {
            text = text.replacen(&format!("'{arg}'"), &format!("'{}'", spelling.typed), 1);
        }
    }
    let text = text.strip_prefix("error: ").unwrap_or(&text).trim_end();
    AppError::Usage(text.to_owned())
}

#[cfg(not(target_arch = "wasm32"))]
/// A usage error saying that `first` cannot be combined with `second`.
fn conflict(first: &str, second: &str) -> AppError {
    AppError::Usage(format!("{first} cannot be combined with {second}"))
}

#[cfg(not(target_arch = "wasm32"))]
/// A usage error saying that `option` needs `other`.
fn needs(option: &str, other: &str) -> AppError {
    AppError::Usage(format!("{option} can only be used together with {other}"))
}

#[cfg(not(target_arch = "wasm32"))]
/// Checks the combinations of the `configuration` spec and turns the parsed
/// command line into [`Options`].
fn convert(cli: Cli, translated: &Translated) -> Result<Options, AppError> {
    let spelled = |long: &str| translated.spelling_of(long);

    let gauntlet = cli.gauntlet.is_some();
    let mode =
        GameMode::from_options(cli.players, cli.two_diggers, gauntlet).map_err(
            |which| match which {
                ModeConflict::TwoDiggers => conflict("--players 2", &spelled("--two-diggers")),
                ModeConflict::Gauntlet => conflict("--players 2", &spelled("--gauntlet")),
            },
        )?;

    if cli.verify.is_some() {
        let verify = spelled("--verify");
        if !cli.play.is_empty() {
            return Err(conflict(&verify, &spelled("--play")));
        }
        if cli.loop_playback {
            return Err(conflict(&verify, &spelled("--loop")));
        }
        if cli.take_over_at.is_some() {
            return Err(conflict(&spelled("--take-over-at"), &verify));
        }
    } else if cli.headless {
        return Err(needs("--headless", "--verify"));
    }
    if cli.play.is_empty() {
        if cli.loop_playback {
            return Err(needs(&spelled("--loop"), "--play"));
        }
        if cli.take_over_at.is_some() {
            return Err(needs(&spelled("--take-over-at"), "--play"));
        }
    }

    let mut speed = cli.speed;
    let mut levels = cli.levels;
    if let Some(bare) = cli.bare {
        let text = bare.to_string_lossy();
        if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
            if speed.is_some() {
                return Err(conflict(
                    &format!("the bare speed {text}"),
                    &spelled("--speed"),
                ));
            }
            speed = Some(bare_speed(&text)?);
        } else {
            if levels.is_some() {
                return Err(conflict(
                    &format!("the bare level file {text}"),
                    &spelled("--levels"),
                ));
            }
            levels = Some(PathBuf::from(bare));
        }
    }

    let layer = Layer {
        speed: speed.and_then(Speed::new),
        mode,
        gauntlet_time: cli.gauntlet.flatten().filter(|&seconds| seconds > 0),
        start_level: cli.start_level,
        unlimited_lives: cli.unlimited_lives.then_some(true),
        sound_enabled: cli.quiet.then_some(false),
        music: cli.no_music.then_some(false),
        fullscreen: cli.fullscreen.then_some(true),
        scale: cli.scale.as_deref().and_then(ScaleMode::from_name),
        ..Layer::default()
    };
    let redefine_keys = cli.redefine_keys.map(|all| match all {
        Some(_) => Redefine::All,
        None => Redefine::Usual,
    });
    Ok(Options {
        layer,
        config: cli.config,
        data_dir: cli.data_dir,
        levels,
        record: cli.record,
        play: cli.play,
        loop_playback: cli.loop_playback,
        take_over_at: cli.take_over_at,
        verify: cli.verify,
        headless: cli.headless,
        redefine_keys,
    })
}

#[cfg(not(target_arch = "wasm32"))]
/// The speed given as a bare number.
fn bare_speed(text: &str) -> Result<u16, AppError> {
    text.parse::<u16>()
        .ok()
        .filter(|speed| *speed <= Speed::MAX)
        .ok_or_else(|| {
            AppError::Usage(format!(
                "invalid value '{text}' for the bare speed argument: {text} is not in 0..={}",
                Speed::MAX
            ))
        })
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use std::path::PathBuf;

    use super::{Command, Options, Redefine, parse};
    use crate::config::{GameMode, ScaleMode, Speed};
    use crate::error::AppError;

    fn run(args: &[&str]) -> Result<Command, AppError> {
        parse(std::iter::once("digger").chain(args.iter().copied()))
    }

    fn options(args: &[&str]) -> Options {
        match run(args) {
            Ok(Command::Run(options)) => *options,
            other => panic!("{args:?}: {other:?}"),
        }
    }

    /// The usage error `args` give, checked to exit with 64.
    fn usage(args: &[&str]) -> String {
        match run(args) {
            Err(error @ AppError::Usage(_)) => {
                assert_eq!(error.exit_code(), 64);
                error.to_string()
            }
            other => panic!("{args:?}: expected a usage error, got {other:?}"),
        }
    }

    #[test]
    fn cli_long_options() {
        let o = options(&[
            "--speed",
            "30",
            "--start-level",
            "5",
            "--unlimited-lives",
            "--gauntlet=300",
            "--quiet",
            "--no-music",
            "--fullscreen",
            "--scale",
            "stretch",
            "--redefine-keys=all",
            "--levels",
            "hard.dlf",
            "--record",
            "run.edrf",
            "--play",
            "a.drf",
            "--play",
            "b.edrf",
            "--loop",
            "--take-over-at",
            "500",
            "--config",
            "./test.toml",
            "--data-dir",
            "data",
        ]);
        assert_eq!(o.layer.speed, Speed::new(30));
        assert_eq!(
            o.layer.speed.map(|s| s.frame_time().as_micros()),
            Some(60_000)
        );
        assert_eq!(o.layer.start_level, Some(5));
        assert_eq!(o.layer.unlimited_lives, Some(true));
        assert_eq!(o.layer.mode, Some(GameMode::Gauntlet));
        assert_eq!(o.layer.gauntlet_time, Some(300));
        assert_eq!(o.layer.sound_enabled, Some(false));
        assert_eq!(o.layer.music, Some(false));
        assert_eq!(o.layer.fullscreen, Some(true));
        assert_eq!(o.layer.scale, Some(ScaleMode::Stretch));
        assert_eq!(o.redefine_keys, Some(Redefine::All));
        assert_eq!(o.levels, Some(PathBuf::from("hard.dlf")));
        assert_eq!(o.record, Some(PathBuf::from("run.edrf")));
        assert_eq!(o.play, [PathBuf::from("a.drf"), PathBuf::from("b.edrf")]);
        assert!(o.loop_playback);
        assert_eq!(o.take_over_at, Some(500));
        assert_eq!(o.config, Some(PathBuf::from("./test.toml")));
        assert_eq!(o.data_dir, Some(PathBuf::from("data")));
        assert_eq!(o.verify, None);
        assert!(!o.headless);

        // Nothing given: nothing in the layer, so file and defaults apply.
        let o = options(&[]);
        assert!(o.layer.is_empty());
        assert_eq!(o, Options::default());

        // Verification and its window-less form; `=` and space forms.
        let o = options(&["--verify=x.drf", "--headless", "--record=y.edrf"]);
        assert_eq!(o.verify, Some(PathBuf::from("x.drf")));
        assert!(o.headless);
        assert_eq!(o.record, Some(PathBuf::from("y.edrf")));

        // Mode options.
        let mode = |args: &[&str]| options(args).layer.mode;
        assert_eq!(mode(&["--players", "1"]), Some(GameMode::OnePlayer));
        assert_eq!(mode(&["--players", "2"]), Some(GameMode::TwoPlayers));
        assert_eq!(mode(&["--two-diggers"]), Some(GameMode::TwoDiggers));
        assert_eq!(
            mode(&["--two-diggers", "--gauntlet"]),
            Some(GameMode::TwoDiggerGauntlet)
        );
        // `--gauntlet` without a value or with 0: the configured time.
        let o = options(&["--gauntlet"]);
        assert_eq!(o.layer.mode, Some(GameMode::Gauntlet));
        assert_eq!(o.layer.gauntlet_time, None);
        assert_eq!(options(&["--gauntlet=0"]).layer.gauntlet_time, None);
        // `--redefine-keys` alone.
        assert_eq!(
            options(&["--redefine-keys"]).redefine_keys,
            Some(Redefine::Usual)
        );
        // Order does not matter.
        assert_eq!(
            options(&["--start-level", "5", "--speed", "30"]),
            options(&["--speed", "30", "--start-level", "5"])
        );
        // A repeated option: the last wins, as upstream reads in order.
        assert_eq!(
            options(&["--speed", "10", "--speed", "20"]).layer.speed,
            Speed::new(20)
        );
        assert_eq!(options(&["--quiet", "/Q"]).layer.sound_enabled, Some(false));
    }

    #[test]
    fn cli_upstream_forms_match_long_forms() {
        assert_eq!(
            options(&["/S:40", "/Q", "/G:120", "/2", "/U", "/I:3"]),
            options(&[
                "--speed",
                "40",
                "--quiet",
                "--gauntlet=120",
                "--two-diggers",
                "--unlimited-lives",
                "--start-level",
                "3",
            ])
        );
        assert_eq!(
            options(&["-e:game.drf", "-s0"]),
            options(&["--verify", "game.drf", "--speed", "0"])
        );
        assert_eq!(
            options(&["/P:game.drf", "/T:500"]),
            options(&["--play", "game.drf", "--take-over-at", "500"])
        );
        assert_eq!(
            options(&["/KA", "/M", "/F", "/L:x.dlf", "/R:r.drf", "/V"]),
            options(&[
                "--redefine-keys=all",
                "--no-music",
                "--fullscreen",
                "--levels",
                "x.dlf",
                "--record",
                "r.drf",
            ])
        );
        assert_eq!(
            options(&["/P:a.drf", "/O"]),
            options(&["--play", "a.drf", "--loop"])
        );
        // An absolute path is a level file, not `/U`.
        assert_eq!(
            options(&["/Users/ann/levels.dlf"]).levels,
            Some(PathBuf::from("/Users/ann/levels.dlf"))
        );
    }

    #[test]
    fn cli_bare_number_is_speed() {
        let o = options(&["60"]);
        assert_eq!(
            o.layer.speed.map(|s| s.frame_time().as_micros()),
            Some(120_000)
        );
        assert_eq!(o.levels, None);
        assert_eq!(options(&["0"]).layer.speed, Speed::new(0));
        let text = usage(&["9000"]);
        assert!(text.contains("9000"), "{text}");
        let text = usage(&["99999999999999999999"]);
        assert!(text.contains("99999999999999999999"), "{text}");
        let text = usage(&["--speed", "20", "60"]);
        assert!(text.contains("--speed") && text.contains("60"), "{text}");
    }

    #[test]
    fn cli_bare_word_is_level_file() {
        let o = options(&["mylevels"]);
        assert_eq!(o.levels, Some(PathBuf::from("mylevels")));
        assert_eq!(o.layer.speed, None);
        assert_eq!(options(&["40x"]).levels, Some(PathBuf::from("40x")));
        let text = usage(&["--levels", "a.dlf", "b.dlf"]);
        assert!(
            text.contains("--levels") && text.contains("b.dlf"),
            "{text}"
        );
        // Only one bare argument.
        let text = usage(&["a.dlf", "b.dlf"]);
        assert!(text.contains("b.dlf"), "{text}");
    }

    #[test]
    fn cli_help_and_version() {
        for args in [
            &["--help"][..],
            &["-h"],
            &["/?"],
            &["/H"],
            &["-H"],
            &["--speed", "5", "/h"],
        ] {
            match run(args) {
                Ok(Command::Print(text)) => {
                    assert!(text.contains("--speed <N>"), "{text}");
                    assert!(text.contains("/S:N"), "{text}");
                }
                other => panic!("{args:?}: {other:?}"),
            }
        }
        for args in [&["--version"][..], &["-V"]] {
            match run(args) {
                Ok(Command::Print(text)) => {
                    assert_eq!(
                        text.trim_end(),
                        format!("digger {}", env!("CARGO_PKG_VERSION"))
                    );
                }
                other => panic!("{args:?}: {other:?}"),
            }
        }
    }

    #[test]
    fn cli_help_lists_every_long_option_with_alias() {
        let Ok(Command::Print(text)) = run(&["--help"]) else {
            panic!("no help");
        };
        for (long, alias) in [
            ("--speed <N>", Some("/S:N")),
            ("--start-level <N>", Some("/I:N")),
            ("--unlimited-lives", Some("/U")),
            ("--players <1|2>", None),
            ("--two-diggers", Some("/2")),
            ("--gauntlet[=<SECS>]", Some("/G[:SECS]")),
            ("--quiet", Some("/Q")),
            ("--no-music", Some("/M")),
            ("--fullscreen", Some("/F")),
            ("--scale <MODE>", None),
            ("--redefine-keys[=<all>]", Some("/KA")),
            ("--levels <FILE>", Some("/L:FILE")),
            ("--record <FILE>", Some("/R:FILE")),
            ("--play <FILE>", Some("/P:FILE")),
            ("--loop", Some("/O")),
            ("--take-over-at <N>", Some("/T:N")),
            ("--verify <FILE>", Some("/E:FILE")),
            ("--headless", None),
            ("--config <FILE>", None),
            ("--data-dir <DIR>", None),
            ("--help", Some("/H")),
            ("--version", None),
        ] {
            let line = text
                .lines()
                .find(|line| line.contains(long))
                .unwrap_or_else(|| panic!("{long} missing from:\n{text}"));
            if let Some(alias) = alias {
                // The alias is on the option's line or the lines of its
                // description that follow.
                let from = text.find(line).unwrap_or(0);
                let rest = &text[from..];
                let block_end = rest[1..].find("\n  -").map_or(rest.len(), |end| end + 1);
                assert!(rest[..block_end].contains(alias), "{long}: {alias}\n{text}");
            }
        }
    }

    #[test]
    fn cli_conflicts() {
        let both = |args: &[&str], first: &str, second: &str| {
            let text = usage(args);
            assert!(text.contains(first), "{args:?}: {text}");
            assert!(text.contains(second), "{args:?}: {text}");
        };
        let text = usage(&["--players", "2", "--two-diggers"]);
        assert_eq!(text, "--players 2 cannot be combined with --two-diggers");
        both(
            &["--players", "2", "--gauntlet"],
            "--players 2",
            "--gauntlet",
        );
        both(&["--players", "2", "/G"], "--players 2", "/G");
        both(&["--players", "2", "-2"], "--players 2", "-2");
        both(
            &["--verify", "a.drf", "--play", "b.drf"],
            "--verify",
            "--play",
        );
        both(&["/E:a.drf", "/P:b.drf"], "/E", "/P");
        both(&["--verify", "a.drf", "--loop"], "--verify", "--loop");
        both(
            &["--verify", "a.drf", "--take-over-at", "5"],
            "--take-over-at",
            "--verify",
        );
        both(&["--headless"], "--headless", "--verify");
        both(&["--headless", "--play", "a.drf"], "--headless", "--verify");
        both(&["--loop"], "--loop", "--play");
        both(&["/O"], "/O", "--play");
        both(&["--take-over-at", "500"], "--take-over-at", "--play");
        both(&["/T:500"], "/T", "--play");
        // Allowed combinations.
        let _ = options(&["--players", "1", "--two-diggers"]);
        let _ = options(&["--verify", "a.drf", "--headless", "--record", "b.edrf"]);
        let _ = options(&["--play", "a.drf", "--loop", "--take-over-at", "5"]);
    }

    #[test]
    fn cli_range_errors_quote_alias() {
        let check = |args: &[&str], option: &str, value: &str| {
            let text = usage(args);
            assert!(text.contains(&format!("'{option}'")), "{args:?}: {text}");
            assert!(text.contains(value), "{args:?}: {text}");
            text
        };
        check(&["--speed", "9000"], "--speed <N>", "9000");
        let text = check(&["/I:abc"], "/I", "abc");
        assert!(!text.contains("'--start-level"), "{text}");
        check(&["/S:9000"], "/S", "9000");
        check(&["-s9000"], "-s", "9000");
        check(&["/G:5000"], "/G", "5000");
        check(&["/T:abc"], "/T", "abc");
        check(&["--start-level", "0"], "--start-level <N>", "0");
        check(&["--players", "3"], "--players <1|2>", "3");
        check(&["--scale", "huge"], "--scale <MODE>", "huge");
        check(&["--gauntlet=x"], "--gauntlet[=<SECS>]", "x");
        // The long form and an alias of the same option: the one with the
        // bad value is quoted.
        check(&["/S:40", "--speed", "9000"], "--speed <N>", "9000");
        check(&["--speed", "40", "/S:9000"], "/S", "9000");
        // A missing value.
        let text = usage(&["/S"]);
        assert!(text.contains("'/S'"), "{text}");
        // Unknown options name themselves and point at --help.
        let text = usage(&["--turbo"]);
        assert!(
            text.contains("--turbo") && text.contains("--help"),
            "{text}"
        );
        let text = usage(&["-Z"]);
        assert!(text.contains("-Z"), "{text}");
        // Upstream flags take no value.
        let text = usage(&["/Q:x"]);
        assert!(text.contains("/Q"), "{text}");
    }

    #[test]
    fn cli_unsupported_upstream_options() {
        assert_eq!(
            usage(&["/C"]),
            "option /C is not supported: CGA graphics are not part of this port"
        );
        let text = usage(&["/N:host"]);
        assert!(
            text.contains("/N") && text.contains("networked play"),
            "{text}"
        );
        let text = usage(&["--speed", "5", "/X:12"]);
        assert!(text.contains("/X") && text.contains("X11"), "{text}");
    }
}
