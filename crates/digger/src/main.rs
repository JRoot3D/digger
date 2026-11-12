//! The Digger desktop game.
//!
//! The module layout follows design D1 of `add-desktop-app`: everything
//! that talks to a window, an audio device or a gamepad lives in
//! [`platform`]; the shell, input mapping, pacing, presentation maths,
//! configuration and score storage are device-free. The binary parses its
//! command line and configuration, runs `--verify --headless` without a
//! window, and otherwise runs the shell in the game window.

#![forbid(unsafe_code)]
// Temporary (add-web-build-and-releases, tasks 2.1 to 3.8): until the web
// platform of task group 3 drives the shell, most of the program is not
// reached from the web stub entry point.
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        dead_code,
        reason = "temporary: the web platform is filled in by task group 3"
    )
)]

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::process::ExitCode;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(not(target_arch = "wasm32"))]
use digger_core::recording::RecordingKind;

#[cfg(not(target_arch = "wasm32"))]
mod cli;
mod config;
mod error;
mod event;
// The web stub entry point of task 1.2 runs no verification yet (temporary
// until task 3.8 of add-web-build-and-releases).
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        dead_code,
        reason = "temporary: the web stub entry point verifies nothing"
    )
)]
mod headless;
mod input;
mod launch;
#[cfg(test)]
mod manual_doc;
mod notice;
mod pacing;
mod page;
mod platform;
mod present;
mod report;
mod scores;
mod shell;
mod store;
mod transfer;

#[cfg(not(target_arch = "wasm32"))]
use error::AppError;

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
/// The environment variable that turns on the debug frame counter (design
/// D15): any value but empty or `0`.
const FRAME_COUNTER_VAR: &str = "DIGGER_FRAME_COUNTER";

#[cfg(not(target_arch = "wasm32"))]
/// The program: parse the command line; run a headless verification, or
/// load the configuration file, resolve the settings, check the level
/// file, and run the shell in the window until it ends; then save what the
/// player changed and report a verification's result.
fn run() -> Result<(), AppError> {
    let options = match cli::parse(std::env::args_os())? {
        cli::Command::Print(text) => {
            print!("{text}");
            return Ok(());
        }
        cli::Command::Run(options) => options,
    };
    // A headless verification needs nothing from the user's profile: the
    // recording supplies every game setting, and a build server may have
    // no configuration or data directory at all.
    if options.headless
        && let Some(file) = &options.verify
    {
        let line = headless::run(file, options.record.as_deref())?;
        print!("{line}");
        return Ok(());
    }
    let paths = config::paths::Paths::resolve(options.config.clone(), options.data_dir.clone())?;
    let files = store::NativeStore::new(paths.config_file().to_path_buf(), paths.scores_file());
    let file = config::file::load(&files)?.unwrap_or_default();
    for warning in &file.warnings {
        report::warning(warning);
    }
    let settings = config::Settings::resolve(&file.layer, &options.layer);
    let levels = options
        .levels
        .as_deref()
        .map(cli::levels::load)
        .transpose()?;
    // A recording to verify that cannot be read ends the program with
    // status 1 before any window opens.
    let verify = options
        .verify
        .as_deref()
        .map(headless::VerifyJob::load)
        .transpose()?;
    let setup = shell::Setup {
        levels: levels
            .as_ref()
            .map_or_else(digger_core::LevelSet::builtin, |file| file.levels),
        record_kind: options
            .record
            .as_deref()
            .map_or(RecordingKind::Drf, shell::recording::kind_for_path),
        base_seed: clock_seed(),
        score_levels: scores::LevelSet::for_level_file(
            levels.as_ref().map(|file| file.path.as_path()),
        ),
        record_path: options.record.clone(),
        redefine_keys: options.redefine_keys,
        play: options.play.clone(),
        loop_playback: options.loop_playback,
        take_over_at: options.take_over_at,
        verify,
    };
    let window = platform::WindowOptions {
        fullscreen: settings.fullscreen,
        scale: settings.scale,
        frame_counter: std::env::var_os(FRAME_COUNTER_VAR)
            .is_some_and(|value| !value.is_empty() && value != "0"),
    };
    let mut shell = shell::Shell::with_storage(settings, setup, store::Storage::new(files));
    shell.set_transfer(transfer::NativeFileTransfer::new(paths.recordings_dir()));
    let platform::Finished { shell, status } = platform::run(platform::App::new(shell, window))?;
    finish(shell, status, options.verify.as_deref())
}

#[cfg(not(target_arch = "wasm32"))]
/// The end of the single shutdown path, after the platform stopped the
/// sound and the shell wrote any pending recording (`desktop-app` spec,
/// clean shutdown): the settings the player changed are saved (a failure
/// is a notice), and a verification run prints its result line or ends
/// with its status.
fn finish(mut shell: shell::Shell, status: u8, verify: Option<&Path>) -> Result<(), AppError> {
    shell.save_settings();
    for notice in shell.take_notices() {
        report::notice(notice);
    }
    if let (Some(report), Some(file)) = (shell.verify_report(), verify) {
        let line = headless::conclude(report, file)?;
        print!("{line}");
        return Ok(());
    }
    match status {
        0 => Ok(()),
        status => Err(AppError::Verify {
            status,
            message: None,
        }),
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// A base seed for live games from the system clock: games differ from
/// run to run, while the simulation itself stays deterministic.
fn clock_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            // Seconds and nanoseconds folded into 64 bits.
            since.as_secs().rotate_left(32) ^ u64::from(since.subsec_nanos())
        })
}

/// A stub entry point for the web build (add-web-build-and-releases, task
/// 1.2): the web platform is still empty (task 2.1), so this only builds
/// the shell with default settings and in-memory tables. Task 3.1 runs it
/// in the page's canvas; task 3.8 removes the temporary gates.
#[cfg(target_arch = "wasm32")]
fn main() {
    let shell = shell::Shell::new(config::Settings::default(), shell::Setup::default());
    drop(shell);
}
