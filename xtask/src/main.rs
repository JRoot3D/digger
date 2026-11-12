//! Developer tooling for the Digger workspace, run as `cargo xtask <command>`.
//!
//! Commands are plain Rust so they behave the same on Linux, Windows and
//! macOS. Later changes add further subcommands beside `ci`.

#![forbid(unsafe_code)]

mod ci;
mod conformance;
mod deps;
mod import;
mod recordings;
mod render;
mod verify;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Developer tasks for the Digger workspace.
#[derive(Debug, Parser)]
#[command(name = "cargo xtask", bin_name = "cargo xtask", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the same checks as continuous integration: formatting, Clippy,
    /// tests, documentation and the dependency-direction check.
    Ci,
    /// Read the upstream sprite, font, title, icon and level tables as data
    /// and write the asset files, index and manifest (or compare with
    /// --check).
    ImportAssets(import::ImportArgs),
    /// Write the golden scenes and a contact sheet of all 120 images as PPM
    /// files (default target/sheets/), for review by eye.
    RenderSheet(render::RenderArgs),
    /// Copy upstream's recordings and expected results byte-for-byte into
    /// tests/recordings/ and write MANIFEST.sha256; refuses a checkout whose
    /// HEAD is not the pinned commit unless --allow-other-commit is given.
    ImportRecordings(recordings::ImportRecordingsArgs),
    /// Play a DRF or eDRF recording headless and check it: the result line
    /// on standard output on success, diagnostics on standard error, exit
    /// status 0 verified or completed, 1 unreadable, 2 failed, 3 stopped,
    /// 4 truncated.
    Verify(verify::VerifyArgs),
    /// Verify the upstream corpus (all 50 recordings, or the quick tier),
    /// one line per recording, and write target/conformance/report.md;
    /// fails if any recording does not pass.
    Conformance(conformance::ConformanceArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result: Result<(), Box<dyn std::error::Error>> = match cli.command {
        Command::Ci => ci::run().map_err(Into::into),
        Command::ImportAssets(args) => import::run(&args).map_err(Into::into),
        Command::RenderSheet(args) => render::run(&args).map_err(Into::into),
        Command::ImportRecordings(args) => recordings::run(&args).map_err(Into::into),
        Command::Conformance(args) => conformance::run(&args).map_err(Into::into),
        Command::Verify(args) => return ExitCode::from(verify::run(&args)),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: error: {error}");
            ExitCode::FAILURE
        }
    }
}
