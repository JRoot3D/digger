//! User-facing messages on standard error, with one uniform prefix
//! (design D12): `digger: error: …` for an error that ends the program,
//! `digger: warning: …` for a setting that was ignored and
//! `digger: notice: …` for a condition the game carries on through (no
//! audio device, an unwritable score file, a skipped playback file).
//!
//! Every message goes through this module, so the wording stays uniform
//! and the browser build can send the lines to its console instead.

use std::fmt;
use std::io::Write;

/// How serious a message is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Level {
    /// The program ends.
    Error,
    /// Something in the user's input was ignored.
    #[cfg_attr(
        target_arch = "wasm32",
        expect(
            dead_code,
            reason = "temporary: the web stub entry point of task 1.2 reports no warnings"
        )
    )]
    Warning,
    /// The program carries on with less (no sound, scores not saved).
    Notice,
}

impl Level {
    /// The word after `digger:` on the line.
    pub(crate) const fn word(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Notice => "notice",
        }
    }
}

/// The text of a message: `digger: <level>: <message>`, without the line
/// end.
pub(crate) fn line(level: Level, message: impl fmt::Display) -> String {
    format!("digger: {}: {message}", level.word())
}

/// Prints a message at `level` on standard error. A failed write to
/// standard error is ignored: there is nowhere left to report it.
pub(crate) fn emit(level: Level, message: impl fmt::Display) {
    let text = line(level, message);
    let _ = writeln!(std::io::stderr().lock(), "{text}");
}

/// Prints `line` on standard error as it is, without the prefix: for
/// reports whose wording another specification fixes, such as a replay's
/// divergence report (replay-verification spec, "Divergence report").
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        dead_code,
        reason = "temporary: the web stub entry point verifies nothing"
    )
)]
pub(crate) fn emit_raw(line: &str) {
    let _ = writeln!(std::io::stderr().lock(), "{line}");
}

/// Prints `digger: error: <message>` on standard error.
pub(crate) fn error(message: impl fmt::Display) {
    emit(Level::Error, message);
}

/// Prints `digger: warning: <message>` on standard error.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn warning(message: impl fmt::Display) {
    emit(Level::Warning, message);
}

/// Prints `digger: notice: <message>` on standard error.
pub(crate) fn notice(message: impl fmt::Display) {
    emit(Level::Notice, message);
}

#[cfg(test)]
mod tests {
    use super::{Level, error, line, notice, warning};

    #[test]
    fn report_lines_carry_the_prefix() {
        assert_eq!(
            line(Level::Error, "cannot read level file x.dlf"),
            "digger: error: cannot read level file x.dlf"
        );
        assert_eq!(
            line(Level::Warning, "config.toml: unknown setting video.foo"),
            "digger: warning: config.toml: unknown setting video.foo"
        );
        assert_eq!(
            line(Level::Notice, "no audio device"),
            "digger: notice: no audio device"
        );
    }

    #[test]
    fn report_helpers_print_without_panicking() {
        error("test error line");
        warning("test warning line");
        notice("test notice line");
    }
}
