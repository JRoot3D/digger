//! The desktop implementation of the platform seam (design D1 of
//! `add-desktop-app`; add-web-build-and-releases, design decision 4): the
//! winit application, the window and its surface, the audio device and the
//! gamepads, and [`run`], which drives the application with the blocking
//! `EventLoop::run_app` until the program ends.

use winit::event_loop::EventLoop;

use super::{RunError, cause};
use crate::shell::Shell;

mod app;
mod audio;
mod pads;
mod window;

pub(crate) use app::{App, WindowOptions};

/// How the program ended on the desktop: the shell, for the settings save
/// and the verification report, and the exit status the shell asked for.
#[derive(Debug)]
pub(crate) struct Finished {
    /// The shell as it was when the event loop stopped.
    pub(crate) shell: Shell,
    /// The exit status the shell asked for.
    pub(crate) status: u8,
}

/// Opens the window and runs `app` in it until the shell asks to exit or
/// the window is closed; then shuts down through the single path of
/// [`App::shut_down`]. Returns the finished shell and its exit status.
///
/// # Errors
///
/// [`RunError::NoDisplay`] (status 69) when no display can be reached or
/// the window cannot be opened.
pub(crate) fn run(mut app: App) -> Result<Finished, RunError> {
    let event_loop = EventLoop::new().map_err(|error| {
        RunError::NoDisplay(format!("cannot open a display: {}", cause(&error)))
    })?;
    let result = event_loop.run_app(&mut app);
    let (shell, ended) = app.shut_down();
    result.map_err(|error| {
        RunError::NoDisplay(format!("the window system failed: {}", cause(&error)))
    })?;
    match ended {
        app::Ended::Exit(status) => Ok(Finished { shell, status }),
        app::Ended::NoWindow(reason) => Err(RunError::NoDisplay(reason)),
    }
}
