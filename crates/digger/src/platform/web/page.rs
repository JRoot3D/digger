//! The page around the canvas as the game shows it
//! (add-web-build-and-releases, design decision 10, `PageUi` seam): the
//! sound prompt, the notices, the controls panel, the save-recording
//! button, the footer and the page root's `data-` attributes, which are
//! also what the browser tests observe. Nothing here touches the game's
//! pixels.
//!
//! Every element is optional: a page without one simply does not show what
//! it would hold. Text is always set as text, never as markup.

use web_sys::{Document, Element};

use super::dom;
use crate::input::Bindings;
use crate::notice::Notice;
use crate::page::{AudioStatus, Footer, PageUi, Status, control_rows};
use crate::report::{self, Level};

/// The sound prompt, shown while audio waits for a user gesture.
const PROMPT_ID: &str = "sound-prompt";
/// Where notices and warnings appear.
const NOTICES_ID: &str = "notices";
/// The controls panel's table body.
const CONTROLS_ID: &str = "controls-list";
/// The save-recording button.
const SAVE_BUTTON_ID: &str = "save-recording";
/// The footer's version text.
const VERSION_ID: &str = "version";
/// The footer's link to the source code.
const SOURCE_ID: &str = "source";
/// The footer's link to the credits.
const CREDITS_ID: &str = "credits";
/// The message that replaces the game area on failure.
const FAILURE_ID: &str = "failure";
/// The loading indicator.
const LOADING_ID: &str = "loading";
/// The element over the canvas holding the prompt and the notices.
const OVERLAY_ID: &str = "overlay";

/// How many notices stay on the page; older ones are removed.
const MAX_NOTICES: u32 = 4;

/// The browser page's implementation of [`PageUi`].
#[derive(Debug)]
pub(crate) struct WebPageUi {
    document: Option<Document>,
    /// The status last shown, so unchanged values are not written again.
    shown: Option<Status>,
    /// The `data-scale` value last shown.
    scale: Option<u32>,
}

impl WebPageUi {
    /// The page: the footer is filled in, and a device that reports touch
    /// as its only input is told that a keyboard or gamepad is needed.
    pub(crate) fn new() -> Self {
        let mut page = Self {
            document: dom::document(),
            shown: None,
            scale: None,
        };
        page.write_footer();
        if touch_only() {
            page.message(
                Level::Notice,
                "Digger needs a keyboard or a gamepad: this device reports touch as its only input.",
            );
        }
        page
    }

    /// The element `id`.
    fn element(&self, id: &str) -> Option<Element> {
        self.document.as_ref()?.get_element_by_id(id)
    }

    /// Writes the version, the source link and the credits link.
    fn write_footer(&self) {
        let footer = Footer::of_build();
        if let Some(version) = self.element(VERSION_ID) {
            version.set_text_content(Some(&footer.version));
        }
        for (id, href) in [(SOURCE_ID, &footer.source), (CREDITS_ID, &footer.credits)] {
            if let Some(link) = self.element(id) {
                let _ = link.set_attribute("href", href);
            }
        }
    }

    /// Shows `text` in the notice area (and on the console), keeping the
    /// newest few.
    pub(crate) fn message(&mut self, level: Level, text: &str) {
        report::emit(level, text);
        let (Some(document), Some(area)) = (self.document.as_ref(), self.element(NOTICES_ID))
        else {
            return;
        };
        let Ok(line) = document.create_element("p") else {
            return;
        };
        line.set_text_content(Some(text));
        line.set_class_name(level.word());
        let _ = area.append_child(&line);
        while area.child_element_count() > MAX_NOTICES {
            match area.first_element_child() {
                Some(oldest) => oldest.remove(),
                None => break,
            }
        }
    }

    /// Shows the picture's whole-number scale (`data-scale`), or `fit`
    /// when it is reduced or stretched.
    pub(crate) fn set_scale(&mut self, scale: u32) {
        if self.scale == Some(scale) {
            return;
        }
        self.scale = Some(scale);
        let value = if scale == 0 {
            String::from("fit")
        } else {
            scale.to_string()
        };
        set_root(self.document.as_ref(), "data-scale", Some(&value));
    }

    /// Replaces the game area with `text`: the game cannot run.
    pub(crate) fn fail(&mut self, text: &str) {
        report::error(text);
        set_root(self.document.as_ref(), "data-phase", Some("failed"));
        for id in [LOADING_ID, dom::CANVAS_ID, OVERLAY_ID] {
            if let Some(element) = self.element(id) {
                element.remove();
            }
        }
        if let Some(failure) = self.element(FAILURE_ID) {
            failure.set_text_content(Some(&format!("The game cannot run: {text}.")));
            let _ = failure.remove_attribute("hidden");
        }
    }
}

/// Sets (or, with `None`, removes) the attribute `name` of the page root.
fn set_root(document: Option<&Document>, name: &str, value: Option<&str>) {
    let Some(root) = document.and_then(Document::document_element) else {
        return;
    };
    let _ = match value {
        Some(value) => root.set_attribute(name, value),
        None => root.remove_attribute(name),
    };
}

/// Whether the device reports touch as its only pointing input.
fn touch_only() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let matches = |query: &str| {
        window
            .match_media(query)
            .ok()
            .flatten()
            .is_some_and(|list| list.matches())
    };
    matches("(any-pointer: coarse)") && !matches("(any-pointer: fine)")
}

impl PageUi for WebPageUi {
    fn set_notice(&mut self, notice: &Notice) {
        self.message(Level::Notice, &notice.to_string());
    }

    fn set_bindings(&mut self, bindings: &Bindings) {
        let (Some(document), Some(list)) = (self.document.as_ref(), self.element(CONTROLS_ID))
        else {
            return;
        };
        list.set_text_content(None);
        for row in control_rows(bindings) {
            let Ok(tr) = document.create_element("tr") else {
                return;
            };
            for text in [row.function, row.keys.as_str()] {
                if let Ok(cell) = document.create_element("td") {
                    cell.set_text_content(Some(text));
                    let _ = tr.append_child(&cell);
                }
            }
            let _ = list.append_child(&tr);
        }
    }

    fn set_status(&mut self, status: &Status) {
        let shown = self.shown.take();
        let document = self.document.clone();
        let document = document.as_ref();
        let changed = |pick: fn(&Status) -> String| {
            shown
                .as_ref()
                .is_none_or(|shown| pick(shown) != pick(status))
        };
        if changed(|status| status.phase.name().to_owned()) {
            set_root(document, "data-phase", Some(status.phase.name()));
        }
        if changed(|status| status.audio.name().to_owned()) {
            set_root(document, "data-audio", Some(status.audio.name()));
            if let Some(prompt) = self.element(PROMPT_ID) {
                let _ = if status.audio == AudioStatus::Locked {
                    prompt.remove_attribute("hidden")
                } else {
                    prompt.set_attribute("hidden", "")
                };
            }
            if status.audio == AudioStatus::Failed {
                self.message(
                    Level::Notice,
                    "Sound is unavailable: the browser gave the game no audio output.",
                );
            }
        }
        set_root(document, "data-frames", Some(&status.frames.to_string()));
        if changed(|status| status.result.clone().unwrap_or_default()) {
            set_root(document, "data-result", status.result.as_deref());
        }
        if changed(|status| status.can_save.to_string())
            && let Some(button) = self.element(SAVE_BUTTON_ID)
        {
            let _ = if status.can_save {
                button.remove_attribute("disabled")
            } else {
                button.set_attribute("disabled", "")
            };
        }
        self.shown = Some(status.clone());
    }
}
