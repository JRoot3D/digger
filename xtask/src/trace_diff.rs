//! `cargo xtask trace-diff <a> <b>`: the first tick and block at which two
//! `dump-state` traces differ (replay-verification spec, "Per-tick state
//! trace for triage"; design D10 and D12 of `add-recording-and-conformance`).
//!
//! Exit status as `diff`'s: 0 when the traces agree, 1 when they differ,
//! 2 when a file cannot be read or holds no trace.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use clap::Args;
use digger_core::replay::{TraceField, TraceLine};
use digger_core::state_hash::layout::Block;

/// Arguments of `cargo xtask trace-diff`.
#[derive(Debug, Args)]
pub(crate) struct TraceDiffArgs {
    /// The first trace, as `cargo xtask dump-state` writes it.
    pub(crate) a: PathBuf,
    /// The second trace.
    pub(crate) b: PathBuf,
}

/// One tick of a trace: its line and the decoded values after it, if the
/// trace has them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TraceTick {
    pub(crate) line: TraceLine,
    pub(crate) fields: Vec<TraceField>,
}

/// Reads a trace. Lines that are neither tick lines nor field lines are
/// an error, so that a wrong file is not silently taken for an empty
/// trace.
pub(crate) fn parse_trace(text: &str) -> Result<Vec<TraceTick>, String> {
    let mut ticks: Vec<TraceTick> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if let Some(parsed) = TraceLine::parse(line) {
            ticks.push(TraceTick {
                line: parsed,
                fields: Vec::new(),
            });
        } else if let Some(field) = TraceField::parse(line) {
            let Some(tick) = ticks.last_mut() else {
                return Err(format!("line {}: a field line before any tick line", n + 1));
            };
            tick.fields.push(field);
        } else if !line.is_empty() {
            return Err(format!("line {}: not a trace line: {line:?}", n + 1));
        }
    }
    if ticks.is_empty() {
        return Err("no trace lines".to_owned());
    }
    Ok(ticks)
}

/// How two traces compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TraceDiff {
    /// Every tick of both traces agrees.
    Same {
        /// The first and last tick.
        first: u32,
        last: u32,
    },
    /// The traces cover different ticks from the `index`-th line on.
    Misaligned {
        index: usize,
        a: Option<u32>,
        b: Option<u32>,
    },
    /// The first tick at which the state differs.
    Differs {
        tick: u32,
        /// The first block that differs, in stream order.
        block: Option<Block>,
        /// The tick's lines of trace a and trace b.
        lines: Box<[TraceLine; 2]>,
        /// The first decoded value that differs, with both values, when
        /// both traces have the decoded dump.
        field: Option<(String, String, String)>,
    },
}

/// The first decoded value that differs.
fn first_field(a: &[TraceField], b: &[TraceField]) -> Option<(String, String, String)> {
    if a.is_empty() || b.is_empty() {
        return None;
    }
    for (x, y) in a.iter().zip(b) {
        if x != y {
            let name = if x.name == y.name {
                x.name.clone()
            } else {
                format!("{} / {}", x.name, y.name)
            };
            return Some((name, x.value.clone(), y.value.clone()));
        }
    }
    let longer = if a.len() > b.len() { a } else { b };
    longer
        .get(a.len().min(b.len()))
        .map(|f| (f.name.clone(), "(absent)".to_owned(), f.value.clone()))
}

/// Compares two traces tick by tick.
pub(crate) fn diff(a: &[TraceTick], b: &[TraceTick]) -> TraceDiff {
    for (index, (x, y)) in a.iter().zip(b).enumerate() {
        if x.line.tick != y.line.tick {
            return TraceDiff::Misaligned {
                index,
                a: Some(x.line.tick),
                b: Some(y.line.tick),
            };
        }
        let field = first_field(&x.fields, &y.fields);
        if x.line != y.line || field.is_some() {
            return TraceDiff::Differs {
                tick: x.line.tick,
                block: x.line.first_differing_block(&y.line),
                lines: Box::new([x.line, y.line]),
                field,
            };
        }
    }
    if a.len() != b.len() {
        let index = a.len().min(b.len());
        return TraceDiff::Misaligned {
            index,
            a: a.get(index).map(|t| t.line.tick),
            b: b.get(index).map(|t| t.line.tick),
        };
    }
    TraceDiff::Same {
        first: a.first().map_or(0, |t| t.line.tick),
        last: a.last().map_or(0, |t| t.line.tick),
    }
}

/// The report of a comparison, each line ending in LF.
pub(crate) fn report(diff: &TraceDiff) -> String {
    match diff {
        TraceDiff::Same { first, last } => {
            format!("traces agree on ticks {first} to {last}\n")
        }
        TraceDiff::Misaligned { index, a, b } => {
            let tick =
                |t: &Option<u32>| t.map_or_else(|| "nothing".to_owned(), |t| format!("tick {t}"));
            format!(
                "traces agree up to their tick line {index}, then part: a has {}, b has {}\n",
                tick(a),
                tick(b)
            )
        }
        TraceDiff::Differs {
            tick,
            block,
            lines,
            field,
        } => {
            let [a, b] = &**lines;
            let mut text = match block {
                Some(block) => format!(
                    "first difference: tick {tick}, {} block\n  a: {}\n  b: {}\n",
                    block.name(),
                    a.block_text(*block),
                    b.block_text(*block)
                ),
                None => format!("first difference: tick {tick}\n  a: {a}\n  b: {b}\n"),
            };
            if let Some((name, x, y)) = field {
                let _ = writeln!(text, "first differing value: {name}: a={x} b={y}");
            }
            text
        }
    }
}

/// Runs `cargo xtask trace-diff` and yields its exit status.
pub(crate) fn run(args: &TraceDiffArgs) -> u8 {
    let read = |path: &PathBuf| -> Result<Vec<TraceTick>, String> {
        let text =
            fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        parse_trace(&text).map_err(|e| format!("{}: {e}", path.display()))
    };
    let (a, b) = match (read(&args.a), read(&args.b)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(error), _) | (_, Err(error)) => {
            eprintln!("xtask trace-diff: {error}");
            return 2;
        }
    };
    let diff = diff(&a, &b);
    print!("{}", report(&diff));
    match diff {
        TraceDiff::Same { .. } => 0,
        TraceDiff::Misaligned { .. } | TraceDiff::Differs { .. } => 1,
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
    use digger_core::state_hash::BlockHashes;

    /// A synthetic trace line for `tick`.
    fn line(tick: u32) -> TraceLine {
        TraceLine {
            tick,
            hash: tick.wrapping_mul(0x9E37_79B9),
            blocks: BlockHashes {
                hashes: [tick, tick ^ 1, tick ^ 2, tick ^ 3, tick ^ 4],
                lengths: [300, 263, 163, 165, 12],
            },
            rng_state: -i32::try_from(tick).unwrap_or(0),
            scores: [0, 0],
        }
    }

    /// The text of a synthetic trace of ticks 513 to 768, with the
    /// monster block changed from tick `from` on when given.
    fn trace(monster_from: Option<u32>) -> String {
        let mut text = String::new();
        for tick in 513..=768 {
            let mut l = line(tick);
            if monster_from.is_some_and(|from| tick >= from) {
                l.blocks.hashes[2] ^= 0xFFFF_0000;
                l.hash ^= 0x0000_FFFF;
            }
            let _ = writeln!(text, "{l}");
        }
        text
    }

    #[test]
    fn trace_diff_finds_monster_block_at_tick_700() {
        let a = parse_trace(&trace(None)).expect("a trace");
        let b = parse_trace(&trace(Some(700))).expect("a trace");
        assert_eq!(a.len(), 256);
        let result = diff(&a, &b);
        let TraceDiff::Differs { tick, block, .. } = &result else {
            panic!("{result:?}")
        };
        assert_eq!(*tick, 700);
        assert_eq!(*block, Some(Block::Monster));
        let text = report(&result);
        assert!(
            text.starts_with("first difference: tick 700, monster block\n"),
            "{text}"
        );
    }

    #[test]
    fn identical_traces_agree() {
        let a = parse_trace(&trace(None)).expect("a trace");
        assert_eq!(
            diff(&a, &a),
            TraceDiff::Same {
                first: 513,
                last: 768
            }
        );
    }

    #[test]
    fn decoded_values_name_the_field() {
        let base = format!("{}\n  monster.x[2]=100\n  monster.y[2]=40\n", line(700));
        let changed = format!("{}\n  monster.x[2]=104\n  monster.y[2]=40\n", line(700));
        let result = diff(
            &parse_trace(&base).expect("a trace"),
            &parse_trace(&changed).expect("a trace"),
        );
        let text = report(&result);
        assert!(
            text.contains("first differing value: monster.x[2]: a=100 b=104"),
            "{text}"
        );
    }

    #[test]
    fn misaligned_and_malformed_traces() {
        let a = parse_trace(&format!("{}\n{}\n", line(1), line(2))).expect("a trace");
        let b = parse_trace(&format!("{}\n", line(1))).expect("a trace");
        assert_eq!(
            diff(&a, &b),
            TraceDiff::Misaligned {
                index: 1,
                a: Some(2),
                b: None
            }
        );
        assert!(parse_trace("score=0 level=1 ticks=5\n").is_err());
        assert!(parse_trace("").is_err());
        assert!(parse_trace("  digger.lives[0]=3\n").is_err());
    }
}
