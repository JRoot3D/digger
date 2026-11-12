//! Reading an eDRF recording as a timeline and cutting it short, for
//! divergence triage (design D10 and D12 of `add-recording-and-conformance`).
//!
//! The timeline places every record at the tick count the recording's
//! controls have reached when it is read, which is where the recorder
//! wrote it: a `C` record at the tick it was taken after, an `E` record at
//! the round's last tick. A cut keeps the same header, maps, seeds,
//! checkpoints and controls up to a tick and then ends with `Q 0`, so that
//! the cut recording is a truncated eDRF of exactly that many ticks.

use core::fmt;

use super::edrf::{EdrfRecords, InputRecord, Record, Run};
use super::error::RecordingError;
use super::header::{Header, LineReader, RecordingKind};
use crate::sim::SlotId;

/// The number of map rows after an `L` record with an inline map.
const MAP_ROWS: usize = 10;

/// One record of an eDRF and where it falls in the game.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimedRecord {
    /// The (first) line of the record.
    pub line: u32,
    /// The tick count the recording's controls have reached once this
    /// record is read: for an `I` record, including its own ticks.
    pub tick: u32,
    /// The record.
    pub record: Record,
}

/// The ticks an `I` record covers: the longest of its slots' run lists.
fn input_ticks(input: &InputRecord) -> u64 {
    SlotId::ALL
        .into_iter()
        .map(|slot| {
            input
                .runs_for(slot)
                .iter()
                .fold(0_u64, |sum, run| sum.saturating_add(run.count))
        })
        .max()
        .unwrap_or(0)
}

/// Reads an eDRF's header and its records in order, each with the tick
/// count reached when it is read.
///
/// # Errors
///
/// Those of [`EdrfRecording::parse`](super::EdrfRecording::parse).
pub fn edrf_timeline(bytes: &[u8]) -> Result<Vec<TimedRecord>, RecordingError> {
    let mut lines = LineReader::new(bytes);
    let header = Header::read(&mut lines)?;
    if header.kind() != RecordingKind::Edrf {
        return Err(RecordingError::NotARecording);
    }
    let mut tick = 0_u64;
    let mut timeline = Vec::new();
    for item in EdrfRecords::new(lines) {
        let (line, record) = item?;
        if let Record::Input(input) = &record {
            tick = tick.saturating_add(input_ticks(input));
        }
        timeline.push(TimedRecord {
            line,
            tick: u32::try_from(tick).unwrap_or(u32::MAX),
            record,
        });
    }
    Ok(timeline)
}

/// Why a recording could not be cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CutError {
    /// The file is not a readable eDRF.
    Recording(RecordingError),
    /// The cut tick is 0: a cut keeps at least one tick.
    ZeroTick,
    /// The recording's controls end before the cut tick.
    BeyondEnd {
        /// The tick asked for.
        at: u32,
        /// The ticks the recording's controls cover.
        ticks: u32,
    },
}

impl fmt::Display for CutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Recording(error) => error.fmt(f),
            Self::ZeroTick => f.write_str("a cut keeps at least one tick: --at must be 1 or more"),
            Self::BeyondEnd { at, ticks } => write!(
                f,
                "cannot cut at tick {at}: the recording's controls end at tick {ticks}"
            ),
        }
    }
}

impl std::error::Error for CutError {}

impl From<RecordingError> for CutError {
    fn from(error: RecordingError) -> Self {
        Self::Recording(error)
    }
}

/// The first `ticks` ticks of `runs`.
fn take_ticks(runs: &[Run], ticks: u64) -> Vec<Run> {
    let mut left = ticks;
    let mut kept = Vec::new();
    for run in runs {
        if left == 0 {
            break;
        }
        let count = run.count.min(left);
        kept.push(Run {
            value: run.value,
            count,
        });
        left -= count;
    }
    kept
}

/// A run list as an `I` record writes it: each control character followed
/// by its count when the count is more than 1.
fn run_text(runs: &[Run]) -> String {
    let mut text = String::new();
    for run in runs {
        text.push(char::from(run.value.to_char()));
        if run.count > 1 {
            text.push_str(&run.count.to_string());
        }
    }
    text
}

/// The first `ticks` ticks of `input`, as an `I` line.
fn cut_input(input: &InputRecord, ticks: u64) -> String {
    match input {
        InputRecord::All(runs) => format!("I {}", run_text(&take_ticks(runs, ticks))),
        InputRecord::Slots(lists) => {
            let mut slots = String::new();
            let mut fields = Vec::new();
            for (n, list) in lists.iter().enumerate() {
                if let Some(runs) = list {
                    slots.push_str(&n.to_string());
                    fields.push(run_text(&take_ticks(runs, ticks)));
                }
            }
            format!("I{slots} {}", fields.join(" "))
        }
    }
}

/// The eDRF `bytes` cut after tick `at` (design D10, `cargo xtask cut`):
/// the same header, then every record up to the one whose controls reach
/// tick `at` (an `I` record crossing it shortened to end there), the `C`
/// and `E` records taken at tick `at`, and finally `Q 0`. Lines are copied
/// as they are, with LF line ends. The result has no `Z` record, so it
/// plays as a truncated recording of exactly `at` ticks.
///
/// # Errors
///
/// [`CutError::Recording`] when `bytes` is not a readable eDRF,
/// [`CutError::ZeroTick`] for `at` = 0, and [`CutError::BeyondEnd`] when
/// the recording's controls end before tick `at`.
pub fn cut_edrf(bytes: &[u8], at: u32) -> Result<String, CutError> {
    if at == 0 {
        return Err(CutError::ZeroTick);
    }
    let timeline = edrf_timeline(bytes)?;
    let total = timeline.last().map_or(0, |r| r.tick);
    if total < at {
        return Err(CutError::BeyondEnd { at, ticks: total });
    }
    let mut all_lines = Vec::new();
    let mut reader = LineReader::new(bytes);
    while let Some(line) = reader.next_line() {
        all_lines.push(String::from_utf8_lossy(line.text).into_owned());
    }
    let line_text = |number: u32| -> &str {
        usize::try_from(number)
            .ok()
            .and_then(|n| n.checked_sub(1))
            .and_then(|i| all_lines.get(i))
            .map_or("", String::as_str)
    };
    let mut out = String::new();
    for number in 1..=4 {
        out.push_str(line_text(number));
        out.push('\n');
    }
    let at = u64::from(at);
    let mut before = 0_u64;
    for timed in &timeline {
        // Past the cut only the checkpoints taken at the cut tick itself
        // are kept.
        let at_cut = u64::from(timed.tick) == at
            && matches!(timed.record, Record::Checkpoint(_) | Record::RoundEnd(_));
        if before >= at && !at_cut {
            break;
        }
        match &timed.record {
            Record::Input(input) => {
                let ticks = input_ticks(input);
                if before.saturating_add(ticks) <= at {
                    out.push_str(line_text(timed.line));
                } else {
                    out.push_str(&cut_input(input, at - before));
                }
                out.push('\n');
                before = before.saturating_add(ticks).min(at);
            }
            Record::Level(level) => {
                let rows = match level.map {
                    super::edrf::LevelMap::Inline(_) => MAP_ROWS,
                    super::edrf::LevelMap::Reuse(_) => 0,
                };
                for offset in 0..=rows {
                    let number = u32::try_from(offset)
                        .ok()
                        .and_then(|o| timed.line.checked_add(o))
                        .unwrap_or(u32::MAX);
                    out.push_str(line_text(number));
                    out.push('\n');
                }
            }
            Record::Quit(_) | Record::End(_) => break,
            Record::Seed(_) | Record::Checkpoint(_) | Record::RoundEnd(_) => {
                out.push_str(line_text(timed.line));
                out.push('\n');
            }
        }
    }
    out.push_str("Q 0\n");
    Ok(out)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::*;

    const RECORDING: &str = "eDRF 2\nRS DIG 20261111\n1\n20000\nR 0000ABCD\nI @10D22\nI01 D16P16 @32\nC 01234567\nE 89ABCDEF\nR 00001111\nI @32\nZ 96 00000000\n";

    #[test]
    fn timeline_places_records_at_their_ticks() {
        let timeline = edrf_timeline(RECORDING.as_bytes()).expect("a recording");
        let ticks: Vec<(u32, u32)> = timeline.iter().map(|r| (r.line, r.tick)).collect();
        assert_eq!(
            ticks,
            [
                (5, 0),
                (6, 32),
                (7, 64),
                (8, 64),
                (9, 64),
                (10, 64),
                (11, 96),
                (12, 96)
            ]
        );
    }

    #[test]
    fn cut_inside_an_input_record() {
        let cut = cut_edrf(RECORDING.as_bytes(), 40).expect("a cut");
        assert_eq!(
            cut,
            "eDRF 2\nRS DIG 20261111\n1\n20000\nR 0000ABCD\nI @10D22\nI01 D8 @8\nQ 0\n"
        );
    }

    #[test]
    fn cut_at_a_checkpoint_keeps_it() {
        let cut = cut_edrf(RECORDING.as_bytes(), 64).expect("a cut");
        assert!(
            cut.ends_with("I01 D16P16 @32\nC 01234567\nE 89ABCDEF\nQ 0\n"),
            "{cut}"
        );
    }

    #[test]
    fn cut_errors() {
        assert_eq!(cut_edrf(RECORDING.as_bytes(), 0), Err(CutError::ZeroTick));
        assert_eq!(
            cut_edrf(RECORDING.as_bytes(), 97),
            Err(CutError::BeyondEnd { at: 97, ticks: 96 })
        );
        assert!(matches!(cut_edrf(b"DRF\n", 5), Err(CutError::Recording(_))));
    }
}
