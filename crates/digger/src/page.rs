//! What the program shows beside the game picture (add-web-build-and-
//! releases, design decisions 4 and 10, `PageUi` seam): notices, the
//! controls in effect and a status summary. The browser shows them as
//! elements of the page and `data-` attributes; the desktop prints notices
//! as `digger: notice:` lines and has nothing else to show. Nothing here is
//! ever drawn into the game's pixels.

use std::fmt;

use crate::input::{Action, Bindings, KeyName, PadAction, PadButton};
use crate::notice::Notice;

/// What the program is doing, for the page root's `data-phase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// The title screen.
    Title,
    /// Key redefinition.
    Redefine,
    /// A game played by a person.
    Playing,
    /// A game played by a person, in its pause phase.
    Paused,
    /// A recording being played back.
    Playback,
}

impl Phase {
    /// The phase's name on the page.
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        allow(dead_code, reason = "written into the browser's page")
    )]
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Redefine => "redefine",
            Self::Playing => "playing",
            Self::Paused => "paused",
            Self::Playback => "playback",
        }
    }
}

/// The state of the audio output, for `data-audio`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioStatus {
    /// No engine.
    Off,
    /// Waiting for a user gesture.
    Locked,
    /// Playing.
    Running,
    /// No device.
    Failed,
}

impl AudioStatus {
    /// The state's name on the page (`data-audio`).
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        allow(dead_code, reason = "written into the browser's page")
    )]
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Locked => "locked",
            Self::Running => "running",
            Self::Failed => "failed",
        }
    }
}

/// A summary of the program's state for the page.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(any(test, target_arch = "wasm32")),
    allow(dead_code, reason = "read by the browser's page of task 4.2")
)]
pub(crate) struct Status {
    /// What the program is doing.
    pub(crate) phase: Phase,
    /// The audio output's state.
    pub(crate) audio: AudioStatus,
    /// Game frames run since the start.
    pub(crate) frames: u64,
    /// The result line of the last recording played back from the page
    /// (`score=S level=L ticks=T`), or why it did not match.
    pub(crate) result: Option<String>,
    /// Whether a finished game's recording can be saved now.
    pub(crate) can_save: bool,
}

/// One row of the controls panel: a game function and what triggers it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(any(test, target_arch = "wasm32")),
    allow(dead_code, reason = "the browser's controls panel")
)]
pub(crate) struct ControlRow {
    /// The function, in words.
    pub(crate) function: &'static str,
    /// The keys or buttons bound to it, in words; `none` when unbound.
    pub(crate) keys: String,
}

/// What a keyboard action does, in words, for the controls panel.
const fn action_label(action: Action) -> &'static str {
    match action {
        Action::P1Right => "Player 1 right",
        Action::P1Up => "Player 1 up",
        Action::P1Left => "Player 1 left",
        Action::P1Down => "Player 1 down",
        Action::P1Fire => "Player 1 fire",
        Action::P2Right => "Player 2 right",
        Action::P2Up => "Player 2 up",
        Action::P2Left => "Player 2 left",
        Action::P2Down => "Player 2 down",
        Action::P2Fire => "Player 2 fire",
        Action::SpeedUp => "Faster",
        Action::SpeedDown => "Slower",
        Action::ToggleMusic => "Music on or off",
        Action::ToggleSound => "Sound on or off",
        Action::Quit => "Back to the title screen",
        Action::Pause => "Pause",
        Action::ChangeMode => "Next game mode (title screen)",
        Action::SaveRecording => "Save the last game's recording (title screen)",
        Action::RedefineKeys => "Redefine the keys (title screen)",
        Action::Fullscreen => "Fullscreen",
    }
}

/// What a gamepad action does, in words.
const fn pad_action_label(action: PadAction) -> &'static str {
    match action {
        PadAction::Fire => "Gamepad: fire",
        PadAction::Pause => "Gamepad: pause",
        PadAction::ChangeMode => "Gamepad: next game mode",
        PadAction::Quit => "Gamepad: back to the title screen",
    }
}

/// A key's name as a player reads it: `KeyA` is `A`, `ArrowLeft` is
/// `Left arrow`, `NumpadAdd` is `Numpad +`; other keys keep their code
/// name (`F1`, `Space`, `Enter`).
pub(crate) fn key_label(key: KeyName) -> String {
    let name = key.name();
    let symbol = |rest: &str| match rest {
        "Add" => Some("+"),
        "Subtract" => Some("-"),
        "Multiply" => Some("*"),
        "Divide" => Some("/"),
        "Decimal" => Some("."),
        _ => None,
    };
    if let Some(rest) = name.strip_prefix("Arrow") {
        format!("{rest} arrow")
    } else if let Some(rest) = name.strip_prefix("Key") {
        rest.to_owned()
    } else if let Some(rest) = name.strip_prefix("Digit") {
        rest.to_owned()
    } else if let Some(rest) = name.strip_prefix("Numpad") {
        format!("Numpad {}", symbol(rest).unwrap_or(rest))
    } else {
        match name {
            "Equal" => String::from("="),
            "Minus" => String::from("-"),
            _ => name.to_owned(),
        }
    }
}

/// Joins labels with commas, or says `none`.
fn joined(labels: impl Iterator<Item = String>) -> String {
    let labels: Vec<String> = labels.collect();
    if labels.is_empty() {
        String::from("none")
    } else {
        labels.join(", ")
    }
}

/// The rows of the controls panel for `bindings` (`web-build` spec,
/// controls panel): every keyboard action in key-table order, then the
/// gamepad actions that have a button.
#[cfg_attr(
    not(any(test, target_arch = "wasm32")),
    allow(dead_code, reason = "the browser's controls panel")
)]
pub(crate) fn control_rows(bindings: &Bindings) -> Vec<ControlRow> {
    let keys = Action::ALL.into_iter().map(|action| ControlRow {
        function: action_label(action),
        keys: joined(bindings.keys_of(action).into_iter().map(key_label)),
    });
    let buttons = PadAction::ALL.into_iter().filter_map(|action| {
        let buttons = bindings.buttons_of(action);
        (!buttons.is_empty()).then(|| ControlRow {
            function: pad_action_label(action),
            keys: joined(
                buttons
                    .into_iter()
                    .map(|button| pad_label(button).to_owned()),
            ),
        })
    });
    keys.chain(buttons).collect()
}

/// A pad button's name as a player reads it.
const fn pad_label(button: PadButton) -> &'static str {
    button.name()
}

/// The page footer's version line and links (`web-build` spec, version,
/// licence and source shown on the page).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(any(test, target_arch = "wasm32")),
    allow(dead_code, reason = "the browser's footer")
)]
pub(crate) struct Footer {
    /// `Digger 1.2.0`.
    pub(crate) version: String,
    /// The source code of exactly this version.
    pub(crate) source: String,
    /// The credits: the notice file of this version.
    pub(crate) credits: String,
}

impl Footer {
    /// The footer of version `version` from `repository`, built from
    /// `revision` (a tag or commit set by the release build) or, without
    /// one, from the version's tag `v<version>`.
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        allow(dead_code, reason = "the browser's footer")
    )]
    pub(crate) fn new(version: &str, repository: &str, revision: Option<&str>) -> Self {
        let tag = format!("v{version}");
        let revision = revision
            .filter(|revision| !revision.is_empty())
            .unwrap_or(&tag);
        let repository = repository.trim_end_matches('/');
        Self {
            version: format!("Digger {version}"),
            source: format!("{repository}/tree/{revision}"),
            credits: format!("{repository}/blob/{revision}/NOTICE"),
        }
    }

    /// This build's footer: the package version, the repository, and the
    /// revision `xtask dist` compiles in as `DIGGER_REVISION`.
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        allow(dead_code, reason = "the browser's footer")
    )]
    pub(crate) fn of_build() -> Self {
        Self::new(
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_REPOSITORY"),
            option_env!("DIGGER_REVISION"),
        )
    }
}

/// The page around the game picture.
pub(crate) trait PageUi: fmt::Debug {
    /// Shows `notice`.
    fn set_notice(&mut self, notice: &Notice);

    /// Shows the bindings in effect (the controls panel).
    fn set_bindings(&mut self, bindings: &Bindings);

    /// Shows the status summary.
    fn set_status(&mut self, status: &Status);
}

/// The desktop's page: notices are lines on standard error; there is no
/// controls panel and no status to show.
#[derive(Debug, Clone, Copy, Default)]
#[cfg(any(test, not(target_arch = "wasm32")))]
pub(crate) struct NativePageUi;

#[cfg(any(test, not(target_arch = "wasm32")))]
impl PageUi for NativePageUi {
    fn set_notice(&mut self, notice: &Notice) {
        crate::report::notice(notice);
    }

    fn set_bindings(&mut self, _bindings: &Bindings) {}

    fn set_status(&mut self, _status: &Status) {}
}

/// A page that keeps what it was shown, for tests.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct RecordingPageUi {
    /// The notices shown, as text, in order.
    pub(crate) notices: Vec<String>,
    /// The bindings shown, in order.
    pub(crate) bindings: Vec<Bindings>,
    /// The statuses shown, in order.
    pub(crate) statuses: Vec<Status>,
}

#[cfg(test)]
impl PageUi for RecordingPageUi {
    fn set_notice(&mut self, notice: &Notice) {
        self.notices.push(notice.to_string());
    }

    fn set_bindings(&mut self, bindings: &Bindings) {
        self.bindings.push(bindings.clone());
    }

    fn set_status(&mut self, status: &Status) {
        self.statuses.push(status.clone());
    }
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::{
        AudioStatus, ControlRow, Footer, NativePageUi, PageUi, Phase, RecordingPageUi, Status,
        control_rows, key_label,
    };
    use crate::config::{Layer, Settings};
    use crate::input::{Action, Bindings, KeyName};
    use crate::notice::Notice;

    #[test]
    fn page_shows_what_it_is_given() {
        let notice = Notice::SettingsNotSaved(String::from("the file is broken"));
        let status = Status {
            phase: Phase::Title,
            audio: AudioStatus::Locked,
            frames: 3,
            result: None,
            can_save: false,
        };
        let mut page = RecordingPageUi::default();
        page.set_notice(&notice);
        page.set_bindings(&Bindings::default());
        page.set_status(&status);
        assert_eq!(page.notices, ["settings not saved: the file is broken"]);
        assert_eq!(page.bindings, [Bindings::default()]);
        assert_eq!(page.statuses, std::slice::from_ref(&status));
        assert_eq!(
            [
                Phase::Title,
                Phase::Redefine,
                Phase::Playing,
                Phase::Paused,
                Phase::Playback
            ]
            .map(Phase::name),
            ["title", "redefine", "playing", "paused", "playback"]
        );
        assert_eq!(
            [
                AudioStatus::Off,
                AudioStatus::Locked,
                AudioStatus::Running,
                AudioStatus::Failed
            ]
            .map(AudioStatus::name),
            ["off", "locked", "running", "failed"]
        );
        // The desktop prints the notice and shows nothing else.
        let mut native = NativePageUi;
        native.set_notice(&notice);
        native.set_bindings(&Bindings::default());
        native.set_status(&status);
    }

    /// The row of the panel for `function`.
    fn row<'a>(rows: &'a [ControlRow], function: &str) -> &'a ControlRow {
        rows.iter()
            .find(|row| row.function == function)
            .unwrap_or_else(|| panic!("no row {function}"))
    }

    #[test]
    fn controls_panel_lists_default_keys() {
        let rows = control_rows(&Bindings::from_settings(&Settings::default()));
        assert_eq!(rows.len(), Action::ALL.len() + 3, "{rows:#?}");
        assert_eq!(row(&rows, "Player 1 right").keys, "Right arrow");
        assert_eq!(row(&rows, "Player 1 down").keys, "Down arrow");
        assert_eq!(row(&rows, "Player 1 fire").keys, "F1");
        assert_eq!(row(&rows, "Pause").keys, "Space");
        assert_eq!(row(&rows, "Player 2 left").keys, "A");
        assert_eq!(row(&rows, "Player 2 fire").keys, "Tab");
        assert_eq!(row(&rows, "Faster").keys, "=, Numpad +");
        assert_eq!(row(&rows, "Gamepad: fire").keys, "South, West");
        assert_eq!(row(&rows, "Gamepad: pause").keys, "Start");
    }

    #[test]
    fn controls_panel_follows_redefinition() {
        let mut keys = Layer::default();
        keys.keys[Action::P1Fire.index()] = KeyName::parse("Enter").map(|key| vec![key]);
        let settings = Settings::resolve(&keys, &Layer::default());
        let rows = control_rows(&Bindings::from_settings(&settings));
        assert_eq!(row(&rows, "Player 1 fire").keys, "Enter");
        let mut none = Layer::default();
        none.keys[Action::P1Fire.index()] = Some(Vec::new());
        let settings = Settings::resolve(&none, &Layer::default());
        let rows = control_rows(&Bindings::from_settings(&settings));
        assert_eq!(row(&rows, "Player 1 fire").keys, "none");
    }

    #[test]
    fn key_labels_read_as_words() {
        let label = |name: &str| KeyName::parse(name).map(key_label);
        assert_eq!(label("ArrowLeft").as_deref(), Some("Left arrow"));
        assert_eq!(label("KeyZ").as_deref(), Some("Z"));
        assert_eq!(label("Digit7").as_deref(), Some("7"));
        assert_eq!(label("NumpadSubtract").as_deref(), Some("Numpad -"));
        assert_eq!(label("Numpad4").as_deref(), Some("Numpad 4"));
        assert_eq!(label("Minus").as_deref(), Some("-"));
        assert_eq!(label("Enter").as_deref(), Some("Enter"));
        assert_eq!(label("F10").as_deref(), Some("F10"));
    }

    #[test]
    fn footer_links_to_the_version_tag() {
        let footer = Footer::new("1.2.0", "https://github.com/example/digger/", None);
        assert_eq!(footer.version, "Digger 1.2.0");
        assert_eq!(
            footer.source,
            "https://github.com/example/digger/tree/v1.2.0"
        );
        assert_eq!(
            footer.credits,
            "https://github.com/example/digger/blob/v1.2.0/NOTICE"
        );
        let built = Footer::new("1.2.0", "https://github.com/example/digger", Some("abc123"));
        assert_eq!(
            built.source,
            "https://github.com/example/digger/tree/abc123"
        );
        let empty = Footer::new("1.2.0", "https://github.com/example/digger", Some(""));
        assert_eq!(empty, footer);
        let this = Footer::of_build();
        assert!(this.version.ends_with(env!("CARGO_PKG_VERSION")));
        assert!(this.source.starts_with(env!("CARGO_PKG_REPOSITORY")));
    }
}
