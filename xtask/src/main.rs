//! Developer tooling for the Digger workspace, run as `cargo xtask <command>`.
//!
//! Commands are plain Rust so they behave the same on Linux, Windows and
//! macOS. Later changes add further subcommands beside `ci`.

#![forbid(unsafe_code)]

mod ci;
mod deps;
mod import;

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
    /// Read the upstream sprite, font, title and icon tables as data and
    /// write the asset files, index and manifest (or compare with --check).
    ImportAssets(import::ImportArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result: Result<(), Box<dyn std::error::Error>> = match cli.command {
        Command::Ci => ci::run().map_err(Into::into),
        Command::ImportAssets(args) => import::run(&args).map_err(Into::into),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: error: {error}");
            ExitCode::FAILURE
        }
    }
}
