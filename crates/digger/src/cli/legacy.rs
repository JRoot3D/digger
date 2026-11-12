//! The alias pre-pass: upstream `/X:value` and `-X` forms rewritten to long
//! options before clap sees them (design D8).
//!
//! Upstream options are one letter after `/` or `-`, in either case, with
//! the value after a `:` (`configuration` spec, upstream option aliases;
//! research 01 §1.1). Only tokens that match that grammar are rewritten;
//! everything else, and everything after `--`, passes through untouched:
//!
//! - a flag letter (`2 U Q M F O K V ? H`, and `C N X` which are refused)
//!   must stand alone (`/Q`), except `/KA`, the one two-letter form;
//! - a numeric option (`S I G T`) may be alone, take `:value` or be
//!   followed directly by digits (`/S40`);
//! - a file option (`L R P E`) may be alone or take `:value`; without the
//!   colon the token is a path, which keeps `/Library/x.dlf` and
//!   `/Users/ann/levels.dlf` from being read as `/L` or `/U`.
//!
//! `-h` and `-V` are left to clap (help and version); `/V` and `-v` are
//! dropped, as upstream accepts them and does nothing.
//!
//! Each rewritten token is remembered with the spelling the user typed, so
//! that errors about it can quote `/I`, not `--start-level`.

use std::ffi::{OsStr, OsString};
use std::fmt;

/// A long option produced from an upstream alias, with how it was typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Spelling {
    /// The long option, `--start-level`.
    pub(crate) long: &'static str,
    /// The option as typed, without its value: `/I`, `-s`, `/KA`.
    pub(crate) typed: String,
    /// The value the alias carried, if any.
    pub(crate) value: Option<OsString>,
}

/// The arguments after the pre-pass and the side table of original
/// spellings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Translated {
    /// The arguments for clap, program name first.
    pub(crate) args: Vec<OsString>,
    /// One entry per rewritten alias, in command-line order.
    pub(crate) spellings: Vec<Spelling>,
}

impl Translated {
    /// How `long` was written by the user: the first alias that produced
    /// it, else the long option itself.
    pub(crate) fn spelling_of(&self, long: &str) -> String {
        self.spellings
            .iter()
            .find(|spelling| spelling.long == long)
            .map_or_else(|| long.to_owned(), |spelling| spelling.typed.clone())
    }
}

/// An upstream option the pre-pass refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CliError {
    /// An upstream option for a feature this port does not have (`/C`,
    /// `/N`, `/X`).
    Unsupported {
        /// The option as typed, `/C`.
        option: String,
        /// What it would do upstream, as the end of the message.
        reason: &'static str,
    },
    /// A value given to an upstream flag that takes none (`/Q:x`).
    UnexpectedValue {
        /// The option as typed, `/Q`.
        option: String,
        /// The whole token as typed.
        token: String,
    },
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { option, reason } => {
                write!(f, "option {option} is not supported: {reason}")
            }
            Self::UnexpectedValue { option, token } => {
                write!(f, "option {option} takes no value, found '{token}'")
            }
        }
    }
}

impl std::error::Error for CliError {}

/// How an upstream letter takes its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// No value.
    Flag,
    /// A number, after `:` or directly.
    Number,
    /// A file name, after `:` only.
    File,
}

/// What an upstream letter turns into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// This long option.
    Long(&'static str),
    /// Dropped (`/V`).
    Ignored,
    /// Refused with this reason.
    Unsupported(&'static str),
}

/// The upstream letter `letter` (upper case): its shape and target.
const fn letter(letter: u8) -> Option<(Shape, Target)> {
    use Shape::{File, Flag, Number};
    use Target::{Ignored, Long, Unsupported};
    Some(match letter {
        b'S' => (Number, Long("--speed")),
        b'I' => (Number, Long("--start-level")),
        b'G' => (Number, Long("--gauntlet")),
        b'T' => (Number, Long("--take-over-at")),
        b'L' => (File, Long("--levels")),
        b'R' => (File, Long("--record")),
        b'P' => (File, Long("--play")),
        b'E' => (File, Long("--verify")),
        b'2' => (Flag, Long("--two-diggers")),
        b'U' => (Flag, Long("--unlimited-lives")),
        b'Q' => (Flag, Long("--quiet")),
        b'M' => (Flag, Long("--no-music")),
        b'F' => (Flag, Long("--fullscreen")),
        b'O' => (Flag, Long("--loop")),
        b'K' => (Flag, Long("--redefine-keys")),
        b'?' | b'H' => (Flag, Long("--help")),
        b'V' => (Flag, Ignored),
        b'C' => (Flag, Unsupported("CGA graphics are not part of this port")),
        b'N' => (Flag, Unsupported("networked play is not part of this port")),
        b'X' => (
            Flag,
            Unsupported("embedding in an X11 window is not part of this port"),
        ),
        _ => return None,
    })
}

/// The long option `--redefine-keys=all` that `/KA` becomes.
const REDEFINE_ALL: &str = "--redefine-keys=all";

/// Rewrites the upstream aliases in `args` (program name first) to long
/// options.
///
/// # Errors
///
/// [`CliError::Unsupported`] for `/C`, `/N` and `/X`;
/// [`CliError::UnexpectedValue`] for a value given to an upstream flag.
pub(crate) fn translate<I>(args: I) -> Result<Translated, CliError>
where
    I: IntoIterator,
    I::Item: Into<OsString>,
{
    let mut out = Translated::default();
    let mut args = args.into_iter().map(Into::into);
    if let Some(program) = args.next() {
        out.args.push(program);
    }
    let mut verbatim = false;
    for arg in args {
        if verbatim {
            out.args.push(arg);
            continue;
        }
        if arg == "--" {
            verbatim = true;
            out.args.push(arg);
            continue;
        }
        match rewrite(&arg)? {
            Some(Rewrite::Drop) => {}
            Some(Rewrite::Long { token, spelling }) => {
                out.args.push(token);
                out.spellings.push(spelling);
            }
            None => out.args.push(arg),
        }
    }
    Ok(out)
}

/// What one token becomes.
#[derive(Debug)]
enum Rewrite {
    /// Nothing (`/V`).
    Drop,
    /// One long-option token, `--speed=40`.
    Long {
        /// The token for clap.
        token: OsString,
        /// How it was typed.
        spelling: Spelling,
    },
}

/// The rewrite of `arg`, or `None` when it is not an upstream alias.
fn rewrite(arg: &OsStr) -> Result<Option<Rewrite>, CliError> {
    let bytes = arg.as_encoded_bytes();
    let (Some(&prefix), Some(&raw_letter)) = (bytes.first(), bytes.get(1)) else {
        return Ok(None);
    };
    if prefix != b'/' && prefix != b'-' {
        return Ok(None);
    }
    // `-h` and `-V` belong to clap.
    if prefix == b'-' && (raw_letter == b'h' || raw_letter == b'V') && bytes.len() == 2 {
        return Ok(None);
    }
    let Some((shape, target)) = letter(raw_letter.to_ascii_uppercase()) else {
        return Ok(None);
    };
    let rest = bytes.get(2..).unwrap_or_default();
    let typed = String::from_utf8_lossy(bytes.get(..2).unwrap_or_default()).into_owned();

    // `/KA`: the one two-letter form.
    if raw_letter.eq_ignore_ascii_case(&b'K') && rest.eq_ignore_ascii_case(b"A") {
        return Ok(Some(Rewrite::Long {
            token: OsString::from(REDEFINE_ALL),
            spelling: Spelling {
                long: "--redefine-keys",
                typed: String::from_utf8_lossy(bytes).into_owned(),
                value: Some(OsString::from("all")),
            },
        }));
    }

    // Where the value starts, if the token matches the grammar at all.
    let value_start = match (rest.first(), shape) {
        (None, _) => None,
        (Some(b':'), _) => Some(3),
        (Some(_), Shape::Number) if rest.iter().all(u8::is_ascii_digit) => Some(2),
        (Some(_), _) => return Ok(None),
    };

    let long = match target {
        Target::Ignored => return Ok(Some(Rewrite::Drop)),
        Target::Unsupported(reason) => {
            return Err(CliError::Unsupported {
                option: typed,
                reason,
            });
        }
        Target::Long(long) => long,
    };

    let value = match value_start {
        None => None,
        Some(_) if shape == Shape::Flag => {
            return Err(CliError::UnexpectedValue {
                option: typed,
                token: arg.to_string_lossy().into_owned(),
            });
        }
        Some(start) => Some(tail(arg, start)),
    };
    let mut token = OsString::from(long);
    if let Some(value) = &value {
        token.push("=");
        token.push(value);
    }
    Ok(Some(Rewrite::Long {
        token,
        spelling: Spelling { long, typed, value },
    }))
}

/// `arg` without its first `skip` characters, all of which are ASCII.
#[cfg(unix)]
fn tail(arg: &OsStr, skip: usize) -> OsString {
    use std::os::unix::ffi::OsStrExt;
    OsStr::from_bytes(arg.as_bytes().get(skip..).unwrap_or_default()).to_owned()
}

/// `arg` without its first `skip` characters, all of which are ASCII (one
/// UTF-16 unit each).
#[cfg(windows)]
fn tail(arg: &OsStr, skip: usize) -> OsString {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let wide: Vec<u16> = arg.encode_wide().skip(skip).collect();
    OsString::from_wide(&wide)
}

/// `arg` without its first `skip` characters, all of which are ASCII.
/// Without a platform way to slice an `OsStr`, a value that is not UTF-8
/// is kept lossily.
#[cfg(not(any(unix, windows)))]
fn tail(arg: &OsStr, skip: usize) -> OsString {
    let text = arg.to_string_lossy();
    OsString::from(text.get(skip..).unwrap_or_default())
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use std::ffi::OsString;

    use super::{CliError, Translated, translate};

    fn run(args: &[&str]) -> Result<Translated, CliError> {
        translate(std::iter::once("digger").chain(args.iter().copied()))
    }

    /// The clap arguments `args` become, without the program name.
    fn out(args: &[&str]) -> Vec<String> {
        let translated = run(args).unwrap_or_else(|e| panic!("{args:?}: {e}"));
        translated.args[1..]
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn legacy_aliases() {
        // Every letter, in both cases and with both prefixes.
        let table: &[(&str, &[&str])] = &[
            ("/S:40", &["--speed=40"]),
            ("/s:40", &["--speed=40"]),
            ("/S40", &["--speed=40"]),
            ("-s0", &["--speed=0"]),
            ("-S:0", &["--speed=0"]),
            ("/S", &["--speed"]),
            ("/I:3", &["--start-level=3"]),
            ("/i3", &["--start-level=3"]),
            ("/I:abc", &["--start-level=abc"]),
            ("/G", &["--gauntlet"]),
            ("/g", &["--gauntlet"]),
            ("/G:120", &["--gauntlet=120"]),
            ("-g120", &["--gauntlet=120"]),
            ("/T:500", &["--take-over-at=500"]),
            ("/t500", &["--take-over-at=500"]),
            ("/L:levels.dlf", &["--levels=levels.dlf"]),
            ("-l:x", &["--levels=x"]),
            ("/R:run.edrf", &["--record=run.edrf"]),
            ("/r:a:b", &["--record=a:b"]),
            ("/P:game.drf", &["--play=game.drf"]),
            ("-p:game.drf", &["--play=game.drf"]),
            ("/E:game.drf", &["--verify=game.drf"]),
            ("-e:game.drf", &["--verify=game.drf"]),
            ("/2", &["--two-diggers"]),
            ("-2", &["--two-diggers"]),
            ("/U", &["--unlimited-lives"]),
            ("/u", &["--unlimited-lives"]),
            ("/Q", &["--quiet"]),
            ("-q", &["--quiet"]),
            ("/M", &["--no-music"]),
            ("/m", &["--no-music"]),
            ("/F", &["--fullscreen"]),
            ("/f", &["--fullscreen"]),
            ("/O", &["--loop"]),
            ("/o", &["--loop"]),
            ("/K", &["--redefine-keys"]),
            ("/k", &["--redefine-keys"]),
            ("/KA", &["--redefine-keys=all"]),
            ("/ka", &["--redefine-keys=all"]),
            ("-Ka", &["--redefine-keys=all"]),
            ("/?", &["--help"]),
            ("-?", &["--help"]),
            ("/H", &["--help"]),
            ("/h", &["--help"]),
            ("-H", &["--help"]),
            ("/V", &[]),
            ("/v", &[]),
            ("-v", &[]),
            // Left to clap.
            ("-h", &["-h"]),
            ("-V", &["-V"]),
            ("--speed", &["--speed"]),
            ("--gauntlet=120", &["--gauntlet=120"]),
            ("40", &["40"]),
            ("levels", &["levels"]),
            // Not the grammar: a file option without its colon, a flag
            // followed by more letters, an unknown letter.
            ("/Lx", &["/Lx"]),
            ("/S40x", &["/S40x"]),
            ("/Qx", &["/Qx"]),
            ("/Z", &["/Z"]),
            ("/", &["/"]),
            ("-", &["-"]),
        ];
        for &(arg, expected) in table {
            assert_eq!(out(&[arg]), expected, "{arg}");
            // After `--` nothing is translated.
            assert_eq!(out(&["--", arg]), ["--", arg], "-- {arg}");
        }
    }

    #[test]
    fn legacy_classic_command_line() {
        assert_eq!(
            out(&["/S:40", "/Q", "/G:120", "/2", "/U", "/I:3"]),
            [
                "--speed=40",
                "--quiet",
                "--gauntlet=120",
                "--two-diggers",
                "--unlimited-lives",
                "--start-level=3",
            ]
        );
    }

    #[test]
    fn legacy_after_double_dash_untouched() {
        assert_eq!(
            out(&["/Q", "--", "/S:40", "/C", "/V"]),
            ["--quiet", "--", "/S:40", "/C", "/V"]
        );
    }

    #[test]
    fn legacy_absolute_paths_untouched() {
        for path in [
            "/Users/a/x.dlf",
            "/Library/x",
            "/home/u/l.dlf",
            "/usr/share/digger/levels.dlf",
            "/tmp/x.dlf",
            "/Volumes/Data/l.dlf",
            "/etc/x",
            "/private/x",
            "/opt/x",
            "/root/x",
            "/media/x",
            "/nix/x",
            "/C/levels.dlf",
        ] {
            assert_eq!(out(&[path]), [path], "{path}");
        }
    }

    #[test]
    fn legacy_spellings_recorded() {
        let translated =
            run(&["-s0", "/I:abc", "/KA", "--quiet"]).unwrap_or_else(|e| panic!("{e}"));
        let spellings: Vec<(&str, &str, Option<OsString>)> = translated
            .spellings
            .iter()
            .map(|s| (s.long, s.typed.as_str(), s.value.clone()))
            .collect();
        assert_eq!(
            spellings,
            [
                ("--speed", "-s", Some(OsString::from("0"))),
                ("--start-level", "/I", Some(OsString::from("abc"))),
                ("--redefine-keys", "/KA", Some(OsString::from("all"))),
            ]
        );
        assert_eq!(translated.spelling_of("--start-level"), "/I");
        assert_eq!(translated.spelling_of("--quiet"), "--quiet");
    }

    #[test]
    fn legacy_unsupported_options() {
        for (arg, option, topic) in [
            ("/C", "/C", "CGA graphics are not part of this port"),
            ("-c", "-c", "CGA graphics"),
            ("/N:host", "/N", "networked play"),
            ("/n", "/n", "networked play"),
            ("/X:0x1234", "/X", "X11 window"),
            ("-x", "-x", "X11 window"),
        ] {
            match run(&[arg]) {
                Err(error @ CliError::Unsupported { .. }) => {
                    let text = error.to_string();
                    assert!(
                        text.starts_with(&format!("option {option} is not supported: ")),
                        "{text}"
                    );
                    assert!(text.contains(topic), "{text}");
                }
                other => panic!("{arg}: {other:?}"),
            }
        }
        assert_eq!(
            run(&["/C"]).map_err(|e| e.to_string()),
            Err("option /C is not supported: CGA graphics are not part of this port".to_owned())
        );
    }

    #[test]
    fn legacy_take_over_alias() {
        assert_eq!(
            out(&["/P:game.drf", "/T:500"]),
            ["--play=game.drf", "--take-over-at=500"]
        );
        let translated = run(&["/T:500"]).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(translated.spelling_of("--take-over-at"), "/T");
    }

    #[test]
    fn legacy_flag_with_value_refused() {
        assert!(matches!(
            run(&["/Q:x"]),
            Err(CliError::UnexpectedValue { option, .. }) if option == "/Q"
        ));
        assert!(matches!(
            run(&["/K:5"]),
            Err(CliError::UnexpectedValue { option, .. }) if option == "/K"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn legacy_non_utf8_value_kept() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let mut raw = b"/L:lev".to_vec();
        raw.push(0xff);
        let arg = OsString::from_vec(raw);
        let translated =
            translate([OsString::from("digger"), arg]).unwrap_or_else(|e| panic!("{e}"));
        let mut expected = b"--levels=lev".to_vec();
        expected.push(0xff);
        assert_eq!(translated.args[1].as_bytes(), expected.as_slice());
    }
}
