//! `cargo xtask verify <file>`: the headless verify run
//! (replay-verification spec, "Headless verify run", "Result line" and
//! "Verify exit statuses"; design D6 and D10 of
//! `add-recording-and-conformance`).
//!
//! On status 0 the result line is the only output on standard output;
//! every explanation goes to standard error.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use clap::Args;
use digger_core::replay::{ReplayOptions, ReplayReport, STATUS_OK, STATUS_UNREADABLE, verify};

/// Arguments of `cargo xtask verify`.
#[derive(Debug, Args)]
pub(crate) struct VerifyArgs {
    /// The DRF or eDRF recording to verify.
    pub(crate) file: PathBuf,
}

/// What verifying one file gave: a report, or the exit status and message
/// of a file that could not be played.
#[derive(Debug)]
pub(crate) enum Verdict {
    /// The recording was played.
    Played(ReplayReport),
    /// The file could not be read or played: its exit status and why.
    Unplayable { status: u8, message: String },
}

impl Verdict {
    /// The verify run's exit status.
    pub(crate) const fn exit_status(&self) -> u8 {
        match self {
            Self::Played(report) => report.exit_status(),
            Self::Unplayable { status, .. } => *status,
        }
    }

    /// The result line on status 0, `None` otherwise.
    pub(crate) fn result_line(&self) -> Option<String> {
        match self {
            Self::Played(report) if report.exit_status() == STATUS_OK => Some(report.result_line()),
            _ => None,
        }
    }

    /// The text for standard error, empty on status 0.
    pub(crate) fn diagnostics(&self, file: &Path) -> String {
        match self {
            Self::Played(report) => report.diagnostics(&file.display().to_string()),
            Self::Unplayable { message, .. } => format!("{message}\n"),
        }
    }
}

/// Verifies the recording at `path`.
pub(crate) fn verify_file(path: &Path) -> Verdict {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Verdict::Unplayable {
                status: STATUS_UNREADABLE,
                message: format!("cannot open {}: {error}", path.display()),
            };
        }
    };
    match verify(&bytes, ReplayOptions::default()) {
        Ok(report) => Verdict::Played(report),
        Err(error) => Verdict::Unplayable {
            status: error.exit_status(),
            message: format!("{}: {error}", path.display()),
        },
    }
}

/// Runs `cargo xtask verify` and yields its exit status.
pub(crate) fn run(args: &VerifyArgs) -> u8 {
    let verdict = verify_file(&args.file);
    if let Some(line) = verdict.result_line() {
        let mut stdout = std::io::stdout().lock();
        if stdout
            .write_all(line.as_bytes())
            .and_then(|()| stdout.flush())
            .is_err()
        {
            eprintln!("xtask verify: cannot write the result line");
            return STATUS_UNREADABLE;
        }
    }
    eprint!("{}", verdict.diagnostics(&args.file));
    verdict.exit_status()
}
