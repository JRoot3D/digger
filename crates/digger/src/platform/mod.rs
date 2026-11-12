//! The platform layer: the only modules that use `winit`, `softbuffer`,
//! `cpal` and `gilrs` (design D1). Nothing outside this module imports them;
//! the test `platform_imports_are_confined` checks it.

use winit::event_loop::EventLoop;

use crate::error::AppError;
use crate::shell::Shell;

mod app;
mod audio;
mod keys;
mod pads;
mod window;

pub(crate) use app::WindowOptions;

/// Opens the window and runs `shell` in it until the shell asks to exit
/// or the window is closed; then shuts down through the single path of
/// [`app::App::shut_down`]. Returns the shell, for the settings save and
/// the verification report, and the exit status the shell asked for.
///
/// # Errors
///
/// [`AppError::NoDisplay`] (status 69) when no display can be reached or
/// the window cannot be opened.
pub(crate) fn run(shell: Shell, options: WindowOptions) -> Result<(Shell, u8), AppError> {
    let event_loop = EventLoop::new().map_err(|error| {
        AppError::NoDisplay(format!("cannot open a display: {}", cause(&error)))
    })?;
    let mut app = app::App::new(shell, options);
    let result = event_loop.run_app(&mut app);
    let (shell, ended) = app.shut_down();
    result.map_err(|error| {
        AppError::NoDisplay(format!("the window system failed: {}", cause(&error)))
    })?;
    match ended {
        app::Ended::Exit(status) => Ok((shell, status)),
        app::Ended::NoWindow(reason) => Err(AppError::NoDisplay(reason)),
    }
}

/// The cause of a winit error as a sentence for the user: winit prefixes
/// operating-system errors with the source location they were raised at
/// (`os error at src/…/mod.rs:765: `), which is dropped here.
fn cause(error: &impl std::fmt::Display) -> String {
    let text = error.to_string();
    match text.strip_prefix("os error at ") {
        Some(rest) => rest
            .split_once(": ")
            .map_or(rest, |(_, cause)| cause)
            .to_owned(),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::cause;

    #[test]
    fn winit_source_locations_are_dropped() {
        assert_eq!(
            cause(&"os error at /src/linux/mod.rs:765: DISPLAY is not set."),
            "DISPLAY is not set."
        );
        assert_eq!(
            cause(&"the event loop was exited"),
            "the event loop was exited"
        );
    }
}
