//! The page around the canvas: the browser's own events forwarded to the
//! application as [`AppEvent`]s, and the few browser calls the game makes
//! (add-web-build-and-releases, design decisions 4, 5 and 8).
//!
//! Every listener lives as long as the page: its closure is handed to the
//! browser with `Closure::forget`. A listener never touches game state; it
//! sends an [`AppEvent`] through the event loop's proxy, or, for the page's
//! buttons, makes the one browser call the button stands for while the
//! click still counts as a user gesture.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Document, Event, HtmlCanvasElement, HtmlElement, HtmlInputElement, KeyboardEvent};
use winit::event_loop::EventLoopProxy;

use crate::event::AppEvent;
use crate::input::{Bindings, KeyName};
use crate::report;
use crate::transfer::{TransferError, check_import_size};

/// The canvas the game draws into: `<canvas id="game">`.
pub(super) const CANVAS_ID: &str = "game";
/// The page's fullscreen button.
const FULLSCREEN_BUTTON_ID: &str = "fullscreen";
/// The page's play-recording button.
const PLAY_BUTTON_ID: &str = "play-recording";
/// The hidden file input the play-recording button opens.
const FILE_INPUT_ID: &str = "recording-file";

/// The page's document.
pub(super) fn document() -> Option<Document> {
    web_sys::window().and_then(|window| window.document())
}

/// Hands `handler` to the browser as the `kind` listener of `target` for
/// the life of the page.
fn listen(target: &web_sys::EventTarget, kind: &str, handler: impl FnMut(Event) + 'static) {
    let closure = Closure::<dyn FnMut(Event)>::new(handler);
    if target
        .add_event_listener_with_callback(kind, closure.as_ref().unchecked_ref())
        .is_err()
    {
        report::notice(format!("the page cannot listen for {kind} events"));
    }
    closure.forget();
}

/// The page's game canvas, made and added to the page when it has none.
pub(super) fn canvas() -> Result<HtmlCanvasElement, String> {
    let document = document().ok_or_else(|| String::from("the page has no document"))?;
    if let Some(element) = document.get_element_by_id(CANVAS_ID) {
        return element
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| format!("the element #{CANVAS_ID} is not a canvas"));
    }
    let canvas = document
        .create_element("canvas")
        .map_err(|_| String::from("cannot make a canvas"))?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|_| String::from("cannot make a canvas"))?;
    canvas.set_id(CANVAS_ID);
    let body = document
        .body()
        .ok_or_else(|| String::from("the page has no body"))?;
    body.append_child(&canvas)
        .map_err(|_| String::from("cannot add the canvas to the page"))?;
    Ok(canvas)
}

/// Forwards the document's `visibilitychange` as
/// [`AppEvent::Visibility`].
pub(super) fn forward_visibility(proxy: EventLoopProxy<AppEvent>) {
    let Some(document) = document() else {
        return;
    };
    let watched = document.clone();
    listen(&document, "visibilitychange", move |_: Event| {
        let visible = !watched.hidden();
        let _ = proxy.send_event(AppEvent::Visibility(visible));
    });
}

/// Whether the page is hidden now.
pub(super) fn hidden() -> bool {
    document().is_some_and(|document| document.hidden())
}

/// Keeps the browser's own action (scrolling, help, focus change, menus)
/// off every key bound to a game function while the canvas has focus
/// (`web-build` spec, keyboard input). `bindings` is replaced whenever a
/// redefinition completes.
pub(super) fn guard_bound_keys(canvas: &HtmlCanvasElement, bindings: Rc<RefCell<Bindings>>) {
    let guard = move |event: Event| {
        if let Some(key) = event.dyn_ref::<KeyboardEvent>()
            && KeyName::parse(&key.code()).is_some_and(|key| bindings.borrow().is_bound(key))
        {
            event.prevent_default();
        }
    };
    listen(canvas, "keydown", guard);
}

/// Switches the page between fullscreen and the page layout. The whole
/// page goes fullscreen, not the canvas, so the scaling rule applies to
/// the whole screen and the page's black background is the letterbox.
pub(super) fn toggle_fullscreen() {
    let Some(document) = document() else {
        return;
    };
    if document.fullscreen_element().is_some() {
        document.exit_fullscreen();
        return;
    }
    let Some(root) = document.document_element() else {
        return;
    };
    if root.request_fullscreen().is_err() {
        report::notice("the browser refused fullscreen");
    }
}

/// Whether the page has a user gesture in effect now
/// (`navigator.userActivation.isActive`); `None` where the browser does
/// not tell.
pub(super) fn user_activation() -> Option<bool> {
    let window: JsValue = web_sys::window()?.into();
    let navigator = js_sys::Reflect::get(&window, &JsValue::from_str("navigator")).ok()?;
    let activation = js_sys::Reflect::get(&navigator, &JsValue::from_str("userActivation")).ok()?;
    if activation.is_undefined() || activation.is_null() {
        return None;
    }
    js_sys::Reflect::get(&activation, &JsValue::from_str("isActive"))
        .ok()?
        .as_bool()
}

/// The element `id`, as an `HtmlElement`.
fn element(document: &Document, id: &str) -> Option<HtmlElement> {
    document
        .get_element_by_id(id)
        .and_then(|element| element.dyn_into::<HtmlElement>().ok())
}

/// Wires the page's fullscreen button, if it has one, to the browser's
/// fullscreen.
pub(super) fn wire_fullscreen_button() {
    let Some(button) = document().and_then(|document| element(&document, FULLSCREEN_BUTTON_ID))
    else {
        return;
    };
    listen(&button, "click", |_: Event| toggle_fullscreen());
}

/// The hidden file input for picking recordings, made when the page has
/// none, with its change listener: the picked file is checked against the
/// 16 MiB limit before it is read, read locally, and sent as
/// [`AppEvent::RecordingPicked`]; a file too large or unreadable is sent
/// as [`AppEvent::TransferFailed`]. The play-recording button, if the page
/// has one, opens the picker.
pub(super) fn file_input(proxy: EventLoopProxy<AppEvent>) -> Option<HtmlInputElement> {
    let document = document()?;
    let input = if let Some(element) = document.get_element_by_id(FILE_INPUT_ID) {
        element.dyn_into::<HtmlInputElement>().ok()?
    } else {
        let input = document
            .create_element("input")
            .ok()?
            .dyn_into::<HtmlInputElement>()
            .ok()?;
        input.set_id(FILE_INPUT_ID);
        input.set_type("file");
        input.set_hidden(true);
        document.body()?.append_child(&input).ok()?;
        input
    };
    input.set_accept(".drf,.edrf");
    let watched = input.clone();
    listen(&input, "change", move |_: Event| {
        let file = watched.files().and_then(|files| files.get(0));
        // Cleared, so picking the same file again is a change too.
        watched.set_value("");
        let Some(file) = file else {
            return;
        };
        let name = file.name();
        // A file's size is a whole number of bytes, far below 2^53.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a file size is a non-negative whole number"
        )]
        let size = file.size().max(0.0) as u64;
        if let Err(error) = check_import_size(&name, size) {
            let _ = proxy.send_event(AppEvent::TransferFailed(error));
            return;
        }
        let proxy = proxy.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let event = match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                Ok(buffer) => AppEvent::RecordingPicked {
                    name,
                    bytes: js_sys::Uint8Array::new(&buffer).to_vec(),
                },
                Err(error) => AppEvent::TransferFailed(TransferError::Unreadable {
                    name,
                    reason: crate::store::web::describe(&error),
                }),
            };
            let _ = proxy.send_event(event);
        });
    });
    if let Some(button) = element(&document, PLAY_BUTTON_ID) {
        let picker = input.clone();
        listen(&button, "click", move |_: Event| picker.click());
    }
    Some(input)
}
