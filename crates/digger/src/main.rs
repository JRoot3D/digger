//! The Digger desktop game.
//!
//! The module layout follows design D1 of `add-desktop-app`: everything
//! that talks to a window, an audio device or a gamepad lives in
//! [`platform`]; the shell, input mapping, pacing, presentation maths,
//! configuration and score storage are device-free. For now the binary
//! only reports its name and version.

#![forbid(unsafe_code)]

use std::process::ExitCode;

mod cli;
mod config;
mod error;
mod headless;
mod input;
mod pacing;
mod platform;
mod present;
mod report;
mod scores;
mod shell;

use error::AppError;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if error.has_message() {
                report::error(&error);
            }
            ExitCode::from(error.exit_code())
        }
    }
}

/// The program: parse, load, run. Command-line handling arrives with task
/// group 3.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the errors are produced once the command line and configuration are wired in"
)]
fn run() -> Result<(), AppError> {
    println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    Ok(())
}
