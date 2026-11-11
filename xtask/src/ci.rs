//! `cargo xtask ci`: the checks continuous integration runs, in order,
//! stopping at the first failure.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::deps;

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
    /// `cargo tree` output was not valid UTF-8.
    TreeOutput { package: &'static str },
    /// A simulation-side crate depends on a platform crate.
    Platform {
        package: &'static str,
        offenders: Vec<String>,
    },
}

impl fmt::Display for CiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn { step, source } => write!(f, "could not run cargo for {step}: {source}"),
            Self::StepFailed { step, code } => match code {
                Some(code) => write!(f, "{step} failed (exit code {code})"),
                None => write!(f, "{step} failed (terminated by a signal)"),
            },
            Self::TreeOutput { package } => {
                write!(f, "`cargo tree` output for {package} is not UTF-8")
            }
            Self::Platform { package, offenders } => write!(
                f,
                "{package} must not depend on platform crates, but its dependency tree \
                 contains: {}",
                offenders.join(", ")
            ),
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
    for &package in deps::CHECKED_PACKAGES {
        check_dependency_direction(&root, package)?;
    }
    eprintln!("xtask ci: all checks passed");
    Ok(())
}

/// Fails if the normal-dependency tree of `package` contains a platform crate.
fn check_dependency_direction(root: &Path, package: &'static str) -> Result<(), CiError> {
    let step = format!("dependency-direction check for {package}");
    eprintln!("xtask ci: running {step}");
    let output = cargo(root)
        .args([
            "tree",
            "--package",
            package,
            "--edges",
            "normal",
            "--prefix",
            "none",
            // Every target, not just the host: a platform crate behind a
            // `[target.'cfg(windows)'.dependencies]` table must fail on Linux too.
            "--target",
            "all",
        ])
        .output()
        .map_err(|source| CiError::Spawn {
            step: step.clone(),
            source,
        })?;
    if !output.status.success() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        return Err(CiError::StepFailed {
            step,
            code: output.status.code(),
        });
    }
    let tree = String::from_utf8(output.stdout).map_err(|_| CiError::TreeOutput { package })?;
    let offenders = deps::denied_crates(&tree);
    if offenders.is_empty() {
        Ok(())
    } else {
        Err(CiError::Platform { package, offenders })
    }
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
