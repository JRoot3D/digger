//! `cargo xtask ci`: the checks continuous integration runs, in order,
//! stopping at the first failure.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One cargo invocation of the CI sequence.
#[derive(Debug)]
struct Step {
    name: &'static str,
    args: &'static [&'static str],
    env: &'static [(&'static str, &'static str)],
}

/// The checks, in the order CI runs them (design.md, "Continuous integration").
const STEPS: &[Step] = &[
    Step {
        name: "format check",
        args: &["fmt", "--all", "--check"],
        env: &[],
    },
    Step {
        name: "clippy",
        args: &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        env: &[],
    },
    Step {
        name: "tests",
        args: &["test", "--workspace"],
        env: &[],
    },
    Step {
        name: "documentation",
        args: &["doc", "--workspace", "--no-deps"],
        env: &[("RUSTDOCFLAGS", "-D warnings")],
    },
];

/// Why `cargo xtask ci` failed.
#[derive(Debug)]
pub(crate) enum CiError {
    /// A cargo process could not be started.
    Spawn {
        step: String,
        source: std::io::Error,
    },
    /// A step ran and exited unsuccessfully.
    StepFailed { step: String, code: Option<i32> },
}

impl fmt::Display for CiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn { step, source } => write!(f, "could not run cargo for {step}: {source}"),
            Self::StepFailed { step, code } => match code {
                Some(code) => write!(f, "{step} failed (exit code {code})"),
                None => write!(f, "{step} failed (terminated by a signal)"),
            },
        }
    }
}

impl std::error::Error for CiError {}

/// Runs every CI step in order and returns the first failure.
pub(crate) fn run() -> Result<(), CiError> {
    let root = workspace_root();
    for step in STEPS {
        eprintln!(
            "xtask ci: running {} (cargo {})",
            step.name,
            step.args.join(" ")
        );
        let mut command = cargo(&root);
        command.args(step.args);
        for (key, value) in step.env {
            command.env(key, value);
        }
        let status = command.status().map_err(|source| CiError::Spawn {
            step: step.name.to_owned(),
            source,
        })?;
        if !status.success() {
            return Err(CiError::StepFailed {
                step: step.name.to_owned(),
                code: status.code(),
            });
        }
    }
    eprintln!("xtask ci: all checks passed");
    Ok(())
}

/// A `cargo` command run from the workspace root, using the same cargo that
/// started xtask when it is known.
fn cargo(root: &Path) -> Command {
    let program = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(program);
    command.current_dir(root);
    command
}

/// The workspace root: the parent of this crate's manifest directory.
fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map_or_else(|| manifest_dir.to_path_buf(), Path::to_path_buf)
}
