//! `cargo xtask conformance [--tier quick|full] [--filter <substr>]
//! [--jobs N]`: verifies the upstream corpus, one line per recording, and
//! writes `target/conformance/report.md` (replay-verification spec,
//! "Conformance suite" and "Suite tiers and run time"; design D10).
//!
//! A recording passes when its verify run exits 0 and prints the result
//! line of `results/<base>.out`. The command exits non-zero if any
//! recording selected fails.

use std::fmt::{self, Write as _};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use clap::{Args, ValueEnum};
use digger_core::replay::{Failure, Outcome};

use crate::recordings::{DATA_DIR, RESULTS_DIR, corpus_dir};
use crate::verify::{Verdict, verify_file};

/// The base names of the quick tier: those whose DRF is at most 15 KB
/// (replay-verification spec, "Suite tiers and run time").
pub(crate) const QUICK_BASE_NAMES: [&str; 11] = [
    "2riptest01",
    "sobodig",
    "dror0",
    "gauntlet01",
    "2test03",
    "thevoid",
    "ramon",
    "muu27700",
    "2test01",
    "2test02",
    "thevoids",
];

/// Which recordings to verify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Tier {
    /// The 18 recordings of the 11 quick base names.
    Quick,
    /// All 50 recordings.
    Full,
}

/// Arguments of `cargo xtask conformance`.
#[derive(Debug, Args)]
pub(crate) struct ConformanceArgs {
    /// The tier to run.
    #[arg(long, value_enum, default_value_t = Tier::Full)]
    pub(crate) tier: Tier,
    /// Only recordings whose file name contains this text.
    #[arg(long)]
    pub(crate) filter: Option<String>,
    /// How many recordings to verify at once (default: the number of
    /// processors).
    #[arg(long)]
    pub(crate) jobs: Option<usize>,
}

/// Why the command could not run, or that a recording failed.
#[derive(Debug)]
pub(crate) enum ConformanceError {
    /// The corpus directory could not be listed.
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
    /// No recording matched the tier and filter.
    NothingSelected,
    /// This many recordings failed.
    Failed { failed: usize, total: usize },
}

impl fmt::Display for ConformanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "{}: {error}", path.display()),
            Self::NothingSelected => f.write_str("no recording matches the tier and filter"),
            Self::Failed { failed, total } => {
                write!(f, "{failed} of {total} recordings failed")
            }
        }
    }
}

impl std::error::Error for ConformanceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// The result of verifying one corpus recording.
#[derive(Debug)]
pub(crate) struct Row {
    /// The file name.
    pub(crate) file: String,
    /// The expected result line, without its line end.
    pub(crate) expected: Option<String>,
    /// What the verify run gave.
    pub(crate) verdict: Verdict,
    /// How long it took.
    pub(crate) elapsed: Duration,
}

impl Row {
    /// The result line the run computed (also when its status is not 0),
    /// without its line end.
    fn actual(&self) -> Option<String> {
        match &self.verdict {
            Verdict::Played(report) => Some(report.result_line().trim_end().to_owned()),
            Verdict::Unplayable { .. } => None,
        }
    }

    /// Whether the recording passes: status 0 and the expected line.
    pub(crate) fn passes(&self) -> bool {
        self.verdict.exit_status() == 0
            && self.expected.is_some()
            && self.actual().as_ref() == self.expected.as_ref()
    }

    /// The outcome in words.
    fn outcome(&self) -> &'static str {
        match &self.verdict {
            Verdict::Played(report) => match report.outcome {
                Outcome::Verified => "verified",
                Outcome::Completed => "completed",
                Outcome::Failed(_) => "failed",
                Outcome::Stopped => "stopped",
                Outcome::Truncated => "truncated",
                Outcome::TakenOver => "taken over",
            },
            Verdict::Unplayable { .. } => "unreadable",
        }
    }

    /// The first failing checkpoint or failure, in words.
    fn first_failure(&self) -> String {
        match &self.verdict {
            Verdict::Played(report) => match &report.outcome {
                Outcome::Failed(Failure::Divergence(divergence)) => {
                    use digger_core::replay::Divergence;
                    let kind = match divergence {
                        Divergence::Tick { .. } => "C",
                        Divergence::RoundEnd { .. } => "E",
                        Divergence::End { .. } => "Z",
                    };
                    format!("{kind} at tick {}", divergence.tick())
                }
                Outcome::Failed(failure @ Failure::Playback(_)) => failure.to_string(),
                _ => String::new(),
            },
            Verdict::Unplayable { message, .. } => message.clone(),
        }
    }

    /// The tick of the last matching checkpoint, in words.
    fn last_match(&self) -> String {
        match &self.verdict {
            Verdict::Played(report) => report
                .last_good_checkpoint
                .map(|tick| tick.to_string())
                .unwrap_or_default(),
            Verdict::Unplayable { .. } => String::new(),
        }
    }
}

/// The recordings of `tier` matching `filter`, sorted by name.
pub(crate) fn select(
    data: &Path,
    tier: Tier,
    filter: Option<&str>,
) -> Result<Vec<String>, ConformanceError> {
    let io = |error| ConformanceError::Io {
        path: data.to_path_buf(),
        error,
    };
    let mut names = Vec::new();
    for entry in fs::read_dir(data).map_err(io)? {
        let name = entry
            .map_err(io)?
            .file_name()
            .to_string_lossy()
            .into_owned();
        let Some((base, suffix)) = name.rsplit_once('.') else {
            continue;
        };
        if !matches!(suffix, "drf" | "edrf") {
            continue;
        }
        if tier == Tier::Quick && !QUICK_BASE_NAMES.contains(&base) {
            continue;
        }
        if filter.is_some_and(|filter| !name.contains(filter)) {
            continue;
        }
        names.push(name);
    }
    names.sort();
    Ok(names)
}

/// Verifies one corpus recording.
fn check(corpus: &Path, file: &str) -> Row {
    let start = Instant::now();
    let verdict = verify_file(&corpus.join(DATA_DIR).join(file));
    let base = file.rsplit_once('.').map_or(file, |(base, _)| base);
    let expected = fs::read_to_string(corpus.join(RESULTS_DIR).join(format!("{base}.out")))
        .ok()
        .map(|text| text.trim_end().to_owned());
    Row {
        file: file.to_owned(),
        expected,
        verdict,
        elapsed: start.elapsed(),
    }
}

/// Verifies `files` on `jobs` threads; the rows come back in the order of
/// `files`.
pub(crate) fn check_all(corpus: &Path, files: &[String], jobs: usize) -> Vec<Row> {
    let next = AtomicUsize::new(0);
    let rows: Mutex<Vec<(usize, Row)>> = Mutex::new(Vec::with_capacity(files.len()));
    std::thread::scope(|scope| {
        for _ in 0..jobs.clamp(1, files.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(file) = files.get(index) else {
                        break;
                    };
                    let row = check(corpus, file);
                    eprintln!(
                        "{:<16} {:<4} {:>8.1}s  {}",
                        row.file,
                        if row.passes() { "ok" } else { "FAIL" },
                        row.elapsed.as_secs_f64(),
                        row.outcome()
                    );
                    if let Ok(mut rows) = rows.lock() {
                        rows.push((index, row));
                    }
                }
            });
        }
    });
    let mut rows = rows
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    rows.sort_by_key(|(index, _)| *index);
    rows.into_iter().map(|(_, row)| row).collect()
}

/// Escapes `text` for a Markdown table cell.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

/// The Markdown report: a summary and one row per recording.
pub(crate) fn report(rows: &[Row], tier: Tier, filter: Option<&str>) -> String {
    let passed = rows.iter().filter(|row| row.passes()).count();
    let total_time: Duration = rows.iter().map(|row| row.elapsed).sum();
    let mut text = String::from("# Conformance report\n\n");
    let tier = match tier {
        Tier::Quick => "quick",
        Tier::Full => "full",
    };
    let _ = write!(text, "Tier: {tier}");
    if let Some(filter) = filter {
        let _ = write!(text, ", filter: `{filter}`");
    }
    let _ = writeln!(
        text,
        ".\n\n{passed} of {} recordings pass; {:.1} s of verification in total.\n",
        rows.len(),
        total_time.as_secs_f64()
    );
    text.push_str(
        "| Recording | Pass | Outcome | Status | First failing checkpoint | \
         Last matching tick | Expected | Actual | Time (s) |\n\
         |---|---|---|---|---|---|---|---|---|\n",
    );
    for row in rows {
        let _ = writeln!(
            text,
            "| `{}` | {} | {} | {} | {} | {} | `{}` | `{}` | {:.1} |",
            row.file,
            if row.passes() { "yes" } else { "**no**" },
            row.outcome(),
            row.verdict.exit_status(),
            cell(&row.first_failure()),
            row.last_match(),
            cell(row.expected.as_deref().unwrap_or("(no .out file)")),
            cell(row.actual().as_deref().unwrap_or("")),
            row.elapsed.as_secs_f64(),
        );
    }
    text
}

/// The report's path: `target/conformance/report.md` in the workspace.
fn report_path() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap_or(manifest_dir)
        .join("target")
        .join("conformance")
        .join("report.md")
}

/// Runs `cargo xtask conformance`.
pub(crate) fn run(args: &ConformanceArgs) -> Result<(), ConformanceError> {
    let corpus = corpus_dir();
    let filter = args.filter.as_deref();
    let files = select(&corpus.join(DATA_DIR), args.tier, filter)?;
    if files.is_empty() {
        return Err(ConformanceError::NothingSelected);
    }
    let jobs = args.jobs.unwrap_or_else(|| {
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
    });
    let rows = check_all(&corpus, &files, jobs);
    let text = report(&rows, args.tier, filter);
    let path = report_path();
    let written = path
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| fs::write(&path, &text));
    if let Err(error) = written {
        return Err(ConformanceError::Io { path, error });
    }
    for row in rows.iter().filter(|row| !row.passes()) {
        eprintln!("--- {}", row.file);
        eprint!("{}", row.verdict.diagnostics(Path::new(&row.file)));
        if let (Some(expected), Some(actual)) = (&row.expected, row.actual())
            && *expected != actual
        {
            eprintln!("expected `{expected}`, got `{actual}`");
        }
    }
    let failed = rows.iter().filter(|row| !row.passes()).count();
    println!(
        "{} of {} recordings pass; report written to {}",
        rows.len() - failed,
        rows.len(),
        path.display()
    );
    if failed > 0 {
        return Err(ConformanceError::Failed {
            failed,
            total: rows.len(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_select_the_corpus() {
        let data = corpus_dir().join(DATA_DIR);
        let full = select(&data, Tier::Full, None).unwrap_or_default();
        assert_eq!(full.len(), 50);
        let quick = select(&data, Tier::Quick, None).unwrap_or_default();
        assert_eq!(quick.len(), 18);
        let filtered = select(&data, Tier::Full, Some("r3333300")).unwrap_or_default();
        assert_eq!(filtered, ["r3333300.drf", "r3333300.edrf"]);
    }

    #[test]
    fn report_has_one_row_per_recording() {
        let corpus = corpus_dir();
        let files = vec!["sobodig.edrf".to_owned(), "2riptest01.edrf".to_owned()];
        let rows = check_all(&corpus, &files, 2);
        assert_eq!(rows.len(), 2);
        assert!(rows.first().is_some_and(Row::passes));
        let text = report(&rows, Tier::Quick, Some("x"));
        let table: Vec<&str> = text.lines().filter(|l| l.starts_with("| `")).collect();
        assert_eq!(table.len(), 2);
        assert!(
            table.first().is_some_and(|row| row.contains(
                "| `sobodig.edrf` | yes | verified | 0 |  | 1378 | \
                 `score=3100 level=2 ticks=1378` | `score=3100 level=2 ticks=1378` |"
            )),
            "{text}"
        );
    }
}
