//! The browser implementation of the platform seam
//! (add-web-build-and-releases, design decisions 4 to 8): the winit
//! application on the page's canvas, driven from the animation callback by
//! [`run`], which hands it to the browser with `EventLoop::spawn_app` and
//! returns at once; the page's own events forwarded as [`AppEvent`]s; the
//! start-up options from the query string; and the panic hook.

use winit::event_loop::EventLoop;
use winit::platform::web::EventLoopExtWebSys;

use super::{RunError, cause};
use crate::config::Layer;
use crate::event::AppEvent;
use crate::launch::{LaunchWarning, parse_query};
use crate::transfer::WebFileTransfer;

mod app;
mod canvas;
mod dom;
mod page;

pub(crate) use app::{App, WindowOptions};

/// Hands `app` to the browser: the page's events are forwarded, the file
/// picker is made, and the event loop runs from the browser's callbacks.
/// Returns at once; the game then runs for the life of the page.
///
/// # Errors
///
/// [`RunError::NoDisplay`] when the browser's event loop cannot be made.
pub(crate) fn run(mut app: App) -> Result<(), RunError> {
    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .map_err(|error| RunError::NoDisplay(format!("cannot start: {}", cause(&error))))?;
    let proxy = event_loop.create_proxy();
    dom::forward_visibility(proxy.clone());
    dom::wire_fullscreen_button();
    dom::wire_save_button(proxy.clone());
    if dom::file_input(proxy).is_none() {
        crate::report::notice("the page cannot offer a file picker for recordings");
    }
    app.set_transfer(WebFileTransfer);
    event_loop.spawn_app(app);
    Ok(())
}

/// The start-up options of the page address's query string.
pub(crate) fn launch_options() -> (Layer, Vec<LaunchWarning>) {
    let search = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .unwrap_or_default();
    parse_query(&search)
}

/// Writes a panic's message to the console before the program aborts
/// (`panic = "abort"`); the page then shows its internal-error message.
pub(crate) fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        crate::report::error(format!("internal error: {info}"));
    }));
}
