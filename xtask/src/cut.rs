//! `cargo xtask cut <file.edrf> --at <tick> --out <path>`: an eDRF holding
//! the same game up to a tick, followed by `Q 0` (design D10 and D12 of
//! `add-recording-and-conformance`).
//!
//! The cut recording keeps the header, maps, seeds, checkpoints and
//! controls of ticks 1 to `--at`; it has no `Z` record, so `cargo xtask
//! verify` plays exactly that many ticks, compares the checkpoints on the
//! way and ends with status 4 (truncated) when they all match.

use std::fs;
use std::path::PathBuf;

use clap::Args;
use digger_core::recording::cut_edrf;

/// Arguments of `cargo xtask cut`.
#[derive(Debug, Args)]
pub(crate) struct CutArgs {
    /// The eDRF recording to cut.
    pub(crate) file: PathBuf,
    /// The last tick to keep.
    #[arg(long)]
    pub(crate) at: u32,
    /// Where to write the cut recording (parent directories are created).
    #[arg(long)]
    pub(crate) out: PathBuf,
}

/// Runs `cargo xtask cut` and yields its exit status: 0 when the cut
/// recording was written, 1 otherwise.
pub(crate) fn run(args: &CutArgs) -> u8 {
    let bytes = match fs::read(&args.file) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("xtask cut: cannot open {}: {error}", args.file.display());
            return 1;
        }
    };
    let text = match cut_edrf(&bytes, args.at) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("xtask cut: {}: {error}", args.file.display());
            return 1;
        }
    };
    if let Some(dir) = args.out.parent().filter(|d| !d.as_os_str().is_empty())
        && let Err(error) = fs::create_dir_all(dir)
    {
        eprintln!("xtask cut: cannot create {}: {error}", dir.display());
        return 1;
    }
    if let Err(error) = fs::write(&args.out, text) {
        eprintln!("xtask cut: cannot write {}: {error}", args.out.display());
        return 1;
    }
    eprintln!(
        "xtask cut: wrote {} ({} ticks of {})",
        args.out.display(),
        args.at,
        args.file.display()
    );
    0
}
