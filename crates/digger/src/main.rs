//! The Digger desktop game.
//!
//! The module layout follows design D1 of `add-desktop-app`: everything
//! that talks to a window, an audio device or a gamepad lives in
//! [`platform`]; the shell, input mapping, pacing, presentation maths,
//! configuration and score storage are device-free. For now the binary
//! parses its command line and configuration and stops there.

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

/// The program: parse the command line, load the configuration file,
/// resolve the settings and check the level file. The window, the shell
/// and verification runs are wired in by later task groups.
fn run() -> Result<(), AppError> {
    let options = match cli::parse(std::env::args_os())? {
        cli::Command::Print(text) => {
            print!("{text}");
            return Ok(());
        }
        cli::Command::Run(options) => options,
    };
    let paths = config::paths::Paths::resolve(options.config.clone(), options.data_dir.clone())?;
    let file = config::file::read(paths.config_file())?.unwrap_or_default();
    for warning in &file.warnings {
        report::warning(warning);
    }
    let settings = config::Settings::resolve(&file.layer, &options.layer);
    let levels = options
        .levels
        .as_deref()
        .map(cli::levels::load)
        .transpose()?;
    let _ = (paths, settings, levels);
    Ok(())
}
