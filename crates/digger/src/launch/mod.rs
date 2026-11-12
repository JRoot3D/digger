//! Start-up options shared by the desktop's command line and the
//! browser's query string (add-web-build-and-releases, design decision 9).
//!
//! Both fill the same settings [`Layer`](crate::config::Layer), which is
//! laid over the stored settings in memory by `Settings::resolve` and is
//! never saved; the mode options are validated by the same rule
//! (`GameMode::from_options`), so an impossible combination is refused
//! before a game is configured. The desktop's command line (`cli`) is
//! compiled for native targets only; it refuses a bad value with a usage
//! error. The browser reads the page address's query string with
//! [`parse_query`], ignores a bad value and shows a [`LaunchWarning`].

use std::fmt;

mod query;

#[cfg(target_arch = "wasm32")]
pub(crate) use query::parse_query;

/// How the key redefinition opens before the title screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    target_arch = "wasm32",
    allow(
        dead_code,
        reason = "only the desktop command line opens on key redefinition"
    )
)]
pub(crate) enum Redefine {
    /// The usual list of actions.
    Usual,
    /// Every action (`--redefine-keys=all`, `/KA`).
    All,
}

/// A start-up option that was ignored: its name, the value given and why.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(any(test, target_arch = "wasm32")),
    expect(dead_code, reason = "made by the browser's query parser")
)]
pub(crate) struct LaunchWarning {
    /// The option's name (`speed`).
    pub(crate) name: String,
    /// The value given (`fast`).
    pub(crate) value: String,
    /// Why it was ignored.
    pub(crate) reason: String,
}

impl fmt::Display for LaunchWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={} ignored: {}", self.name, self.value, self.reason)
    }
}

#[cfg(test)]
mod tests {
    use super::LaunchWarning;
    use crate::config::{GameMode, Layer, ModeConflict, Settings};

    #[test]
    fn launch_warning_names_option_and_value() {
        let warning = LaunchWarning {
            name: String::from("speed"),
            value: String::from("fast"),
            reason: String::from("a number from 0 to 500 is expected"),
        };
        assert_eq!(
            warning.to_string(),
            "speed=fast ignored: a number from 0 to 500 is expected"
        );
    }

    #[test]
    fn launch_invalid_mode_combination_is_rejected() {
        // The shared rule: two players taking turns cannot be combined with
        // two Diggers at once or with the gauntlet.
        assert_eq!(
            GameMode::from_options(Some(2), true, false),
            Err(ModeConflict::TwoDiggers)
        );
        assert_eq!(
            GameMode::from_options(Some(2), false, true),
            Err(ModeConflict::Gauntlet)
        );
        assert_eq!(
            GameMode::from_options(Some(1), true, true),
            Ok(Some(GameMode::TwoDiggerGauntlet))
        );
        assert_eq!(GameMode::from_options(None, false, false), Ok(None));
        // A launch layer holds one mode, which overrides the stored one in
        // memory only.
        let stored = Layer {
            mode: Some(GameMode::Gauntlet),
            ..Layer::default()
        };
        let launch = Layer {
            mode: Some(GameMode::TwoPlayers),
            ..Layer::default()
        };
        assert_eq!(
            Settings::resolve(&stored, &launch).mode,
            GameMode::TwoPlayers
        );
        assert_eq!(
            Settings::resolve(&stored, &Layer::default()).mode,
            GameMode::Gauntlet
        );
        // The desktop's command line refuses the combination before any
        // window opens (status 64).
        #[cfg(not(target_arch = "wasm32"))]
        {
            let refused =
                crate::cli::parse(["digger", "--players", "2", "--two-diggers"].map(String::from));
            assert!(refused.is_err_and(|error| error.exit_code() == 64));
        }
    }
}
