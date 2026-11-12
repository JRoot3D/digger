//! What the program shows beside the game picture (add-web-build-and-
//! releases, design decisions 4 and 10, `PageUi` seam): notices, the
//! controls in effect and a status summary. The browser shows them as
//! elements of the page and `data-` attributes; the desktop prints notices
//! as `digger: notice:` lines and has nothing else to show. Nothing here is
//! ever drawn into the game's pixels.

use std::fmt;

use crate::input::Bindings;
use crate::notice::Notice;
use crate::report;

/// What the program is doing, for the page root's `data-phase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// The title screen.
    Title,
    /// Key redefinition.
    Redefine,
    /// A game played by a person.
    Playing,
    /// A recording being played back.
    Playback,
}

impl Phase {
    /// The phase's name on the page.
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "written into the page by task 4.2")
    )]
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Redefine => "redefine",
            Self::Playing => "playing",
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
pub(crate) struct NativePageUi;

impl PageUi for NativePageUi {
    fn set_notice(&mut self, notice: &Notice) {
        report::notice(notice);
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
mod tests {
    use super::{AudioStatus, NativePageUi, PageUi, Phase, RecordingPageUi, Status};
    use crate::input::Bindings;
    use crate::notice::Notice;

    #[test]
    fn page_shows_what_it_is_given() {
        let notice = Notice::SettingsNotSaved(String::from("the file is broken"));
        let status = Status {
            phase: Phase::Title,
            audio: AudioStatus::Locked,
            frames: 3,
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
                Phase::Playback
            ]
            .map(Phase::name),
            ["title", "redefine", "playing", "playback"]
        );
        // The desktop prints the notice and shows nothing else.
        let mut native = NativePageUi;
        native.set_notice(&notice);
        native.set_bindings(&Bindings::default());
        native.set_status(&status);
    }
}
