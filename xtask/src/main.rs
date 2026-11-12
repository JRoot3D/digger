//! Developer tooling for the Digger workspace, run as `cargo xtask <command>`.
//!
//! Commands are plain Rust so they behave the same on Linux, Windows and
//! macOS. Later changes add further subcommands beside `ci`.

#![forbid(unsafe_code)]

mod bisect;
mod ci;
mod conformance;
mod convert;
mod cut;
mod deps;
mod dump_state;
mod import;
mod recordings;
mod render;
mod trace_diff;
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
    /// Play a DRF or eDRF recording and record it again in the format the
    /// output name selects (.edrf for eDRF, anything else DRF); the exit
    /// status is that of `verify` on the input.
    Convert(convert::ConvertArgs),
    /// Play a recording and write one trace line per tick from --from to
    /// --to (full hash, block hashes and lengths, RNG state, scores); with
    /// --fields also every hashed value. Plays on past a failed checkpoint.
    DumpState(dump_state::DumpStateArgs),
    /// Write an eDRF holding the same game up to tick --at, followed by
    /// `Q 0`; it verifies as truncated after exactly that many ticks.
    Cut(cut::CutArgs),
    /// Verify an eDRF and report the failing window, the last matching
    /// checkpoint, the recorded round ends inside the window and the
    /// dump-state command to run next.
    Bisect(bisect::BisectArgs),
    /// Compare two dump-state traces and report the first tick and block
    /// that differ; exit status 0 same, 1 different, 2 unreadable.
    TraceDiff(trace_diff::TraceDiffArgs),
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
        Command::Convert(args) => return ExitCode::from(convert::run(&args)),
        Command::DumpState(args) => return ExitCode::from(dump_state::run(&args)),
        Command::Cut(args) => return ExitCode::from(cut::run(&args)),
        Command::Bisect(args) => return ExitCode::from(bisect::run(&args)),
        Command::TraceDiff(args) => return ExitCode::from(trace_diff::run(&args)),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: error: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::*;

    /// The `cargo xtask` command lines in the `sh` blocks of the triage
    /// procedure of `docs/conformance.md`, as argument lists after
    /// `cargo xtask` (or `cargo run … -p xtask --`).
    fn triage_commands() -> Vec<Vec<String>> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/conformance.md");
        let doc = std::fs::read_to_string(&path).expect("docs/conformance.md");
        let start = doc.find("## Triage procedure").expect("the triage section");
        let end = doc.find("## Tracking table").expect("the tracking table");
        let mut commands = Vec::new();
        let mut in_sh = false;
        for line in doc.get(start..end).expect("a section").lines() {
            let line = line.trim();
            if line.starts_with("```") {
                in_sh = line == "```sh";
                continue;
            }
            if !in_sh {
                continue;
            }
            let words: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
            let args = if let Some(rest) = line.strip_prefix("cargo xtask ") {
                rest.split_whitespace().map(str::to_owned).collect()
            } else if let Some(dashes) = words.iter().position(|w| w == "--")
                && line.starts_with("cargo run ")
                && line.contains("-p xtask")
            {
                words.get(dashes + 1..).unwrap_or_default().to_vec()
            } else {
                panic!("not an xtask command in the triage procedure: {line}")
            };
            commands.push(args);
        }
        commands
    }

    #[test]
    fn triage_commands_in_docs_parse() {
        let commands = triage_commands();
        assert!(commands.len() >= 9, "{commands:?}");
        for args in commands {
            let argv = std::iter::once("cargo xtask".to_owned()).chain(args.iter().cloned());
            if let Err(error) = Cli::try_parse_from(argv) {
                panic!("{args:?}: {error}");
            }
        }
    }
}
