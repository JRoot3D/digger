//! `cargo xtask dump-state <file> --from <tick> --to <tick> [--fields]
//! [--out <path>]`: the per-tick state trace of a recording
//! (replay-verification spec, "Per-tick state trace for triage"; design
//! D10 and D12 of `add-recording-and-conformance`).
//!
//! The recording is played as the verify run plays it, except that it
//! plays on past a failed checkpoint with the recorded values, so that a
//! window can be dumped beyond the first divergence. It stops after the
//! last tick asked for. The trace goes to standard output or to `--out`;
//! notes about the playback go to standard error.

use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::Args;
use digger_core::FrameInput;
use digger_core::recording::parse;
use digger_core::replay::{Replay, ReplayOptions, ReplayStep, TraceOptions};

/// Arguments of `cargo xtask dump-state`.
#[derive(Debug, Args)]
pub(crate) struct DumpStateArgs {
    /// The DRF or eDRF recording to play.
    pub(crate) file: PathBuf,
    /// The first tick to trace (the first tick of the game is 1).
    #[arg(long)]
    pub(crate) from: u32,
    /// The last tick to trace, inclusive.
    #[arg(long)]
    pub(crate) to: u32,
    /// Follow each tick line with the decoded value of every hashed field.
    #[arg(long)]
    pub(crate) fields: bool,
    /// Write the trace here (parent directories are created) instead of to
    /// standard output.
    #[arg(long)]
    pub(crate) out: Option<PathBuf>,
}

/// Plays `bytes` and writes the trace of `options` to `out`; returns the
/// notes for standard error.
fn dump(bytes: &[u8], options: TraceOptions, out: &mut dyn Write) -> Result<String, String> {
    let recording = parse(bytes).map_err(|e| e.to_string())?;
    let replay_options = ReplayOptions {
        stop_at_first_failure: false,
        ..ReplayOptions::default()
    };
    let mut replay = Replay::new(&recording, replay_options).map_err(|e| e.to_string())?;
    replay.set_trace(options);
    let write = |out: &mut dyn Write, text: String| {
        out.write_all(text.as_bytes())
            .map_err(|e| format!("cannot write the trace: {e}"))
    };
    let mut finished = false;
    loop {
        let step = replay.step(&FrameInput::NONE, &mut digger_core::NoopObserver);
        write(out, replay.take_trace())?;
        if let ReplayStep::Finished(_) = step {
            finished = true;
            break;
        }
        if replay.game().state().ticks() >= options.to {
            break;
        }
    }
    out.flush()
        .map_err(|e| format!("cannot write the trace: {e}"))?;
    let mut notes = String::new();
    let ticks = replay.game().state().ticks();
    if let Some(failure) = replay.failure() {
        let _ = writeln!(notes, "note: the playback failed: {failure}");
    }
    if finished && ticks < options.to {
        let _ = writeln!(
            notes,
            "note: the game ended after tick {ticks}, before tick {}",
            options.to
        );
    }
    Ok(notes)
}

/// Opens the output: the file `path` (creating its directory) or
/// standard output.
fn output(path: Option<&Path>) -> Result<Box<dyn Write>, String> {
    let Some(path) = path else {
        return Ok(Box::new(io::BufWriter::new(io::stdout().lock())));
    };
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let file =
        fs::File::create(path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    Ok(Box::new(io::BufWriter::new(file)))
}

/// Runs `cargo xtask dump-state` and yields its exit status: 0 when the
/// trace was written, 1 otherwise.
pub(crate) fn run(args: &DumpStateArgs) -> u8 {
    if args.from == 0 || args.from > args.to {
        eprintln!(
            "xtask dump-state: --from must be at least 1 and at most --to (got {} to {})",
            args.from, args.to
        );
        return 1;
    }
    let bytes = match fs::read(&args.file) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!(
                "xtask dump-state: cannot open {}: {error}",
                args.file.display()
            );
            return 1;
        }
    };
    let options = TraceOptions {
        from: args.from,
        to: args.to,
        fields: args.fields,
    };
    let result = output(args.out.as_deref()).and_then(|mut out| dump(&bytes, options, &mut *out));
    match result {
        Ok(notes) => {
            eprint!("{notes}");
            0
        }
        Err(error) => {
            eprintln!("xtask dump-state: {}: {error}", args.file.display());
            1
        }
    }
}
