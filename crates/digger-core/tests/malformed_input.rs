//! Malformed-input hardening of the recording codecs and the playback
//! driver (task group 11 of `add-recording-and-conformance`; game-recording
//! spec, "Malformed input never crashes").
//!
//! - [`mutated_recordings_never_panic`]: 10000 inputs made from the
//!   quick-tier corpus files by a deterministic mutation generator (byte
//!   flips, deletions, insertions, line duplications and truncations, fixed
//!   seeds) are each read and, when they parse, played with a tick limit;
//!   every attempt ends in a report or an error.
//! - [`prefixes_are_never_verified`]: every 97th-byte prefix of each
//!   quick-tier file is read and played; an eDRF prefix that is not the
//!   whole file never verifies.
//! - [`huge_counts_and_lines_are_cheap`]: `I @4000000000`, the DRF run
//!   `s4000000000`, a 10 MB line and a file of 1000000 blank lines are
//!   each handled quickly and without keeping anything the size of the
//!   count or the line.
//!
//! The crate forbids `unsafe`, so allocations cannot be counted with a
//! global allocator; instead the tests bound the time each input takes,
//! the size of the error it yields (an error holds no copy of a huge
//! line) and the frames each playback runs (no playback runs on past its
//! tick limit).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure; indices are bounded by the lengths they index"
)]

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::time::{Duration, Instant};

use digger_core::recording::{Recording, RecordingError, RecordingKind, parse};
use digger_core::replay::{Outcome, Replay, ReplayOptions, ReplayReport, ReplayStep};
use digger_core::{FrameInput, NoopObserver};

/// The quick-tier corpus files: the recordings of the base names whose DRF
/// is at most 15 KB (replay-verification spec, "Suite tiers and run time").
const QUICK_FILES: [&str; 18] = [
    "2riptest01.edrf",
    "2test01.drf",
    "2test01.edrf",
    "2test02.edrf",
    "2test03.edrf",
    "dror0.drf",
    "dror0.edrf",
    "gauntlet01.edrf",
    "muu27700.drf",
    "muu27700.edrf",
    "ramon.drf",
    "ramon.edrf",
    "sobodig.drf",
    "sobodig.edrf",
    "thevoid.drf",
    "thevoid.edrf",
    "thevoids.drf",
    "thevoids.edrf",
];

/// How many mutated inputs [`mutated_recordings_never_panic`] tries.
const MUTATIONS: u64 = 10_000;

/// The tick limit of the mutated playbacks: enough for the first few
/// checkpoints and a round or two, small enough for 10000 playbacks in a
/// debug build.
const MUTATION_MAX_TICKS: u32 = 600;

/// More frames than any playback with `max_ticks` can need: the game is
/// quit on the frame after the limit, and the frames that are not ticks
/// (round starts and ends, the end of the game) are a bounded handful per
/// round. A playback still running after this many frames hangs.
const fn frame_budget(max_ticks: u32) -> u64 {
    4 * max_ticks as u64 + 10_000
}

fn corpus(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/recordings/data")
        .join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A small deterministic generator (`SplitMix64`): no new dependency, and
/// the same inputs on every run and platform.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..bound`; `bound` is not 0.
    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % bound as u64).expect("below a usize bound")
    }

    /// A byte that is likely to mean something in a recording.
    fn byte(&mut self) -> u8 {
        const MEANINGFUL: &[u8] = b"0123456789abcdefABCDEF srudlSRUDLEeIZCLR@M\n\r-";
        match self.below(4) {
            0 => u8::try_from(self.next() & 0xFF).expect("masked to a byte"),
            _ => MEANINGFUL[self.below(MEANINGFUL.len())],
        }
    }
}

/// One mutation of `bytes`, chosen by `rng`.
fn mutate_once(bytes: &mut Vec<u8>, rng: &mut Rng) {
    let len = bytes.len();
    match rng.below(5) {
        // Byte flip: one bit, or the whole byte replaced.
        0 if len > 0 => {
            let at = rng.below(len);
            if rng.below(2) == 0 {
                bytes[at] ^= 1 << rng.below(8);
            } else {
                bytes[at] = rng.byte();
            }
        }
        // Deletion of 1 to 16 bytes.
        1 if len > 0 => {
            let at = rng.below(len);
            let end = (at + 1 + rng.below(16)).min(len);
            bytes.drain(at..end);
        }
        // Insertion of 1 to 16 bytes; sometimes a long digit string, the
        // stuff of huge counts.
        2 => {
            let at = rng.below(len + 1);
            let inserted: Vec<u8> = if rng.below(8) == 0 {
                (0..10 + rng.below(12))
                    .map(|_| b'0' + u8::try_from(rng.below(10)).expect("a digit"))
                    .collect()
            } else {
                (0..=rng.below(16)).map(|_| rng.byte()).collect()
            };
            bytes.splice(at..at, inserted);
        }
        // Line duplication: one line repeated 1 to 3 times in place.
        3 if len > 0 => {
            let at = rng.below(len);
            let start = bytes[..at]
                .iter()
                .rposition(|&b| b == b'\n')
                .map_or(0, |i| i + 1);
            let end = bytes[at..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(len, |i| at + i + 1);
            let line = bytes[start..end].to_vec();
            for _ in 0..=rng.below(3) {
                bytes.splice(end..end, line.iter().copied());
            }
        }
        // Truncation.
        _ => {
            let at = rng.below(len + 1);
            bytes.truncate(at);
        }
    }
}

/// The `index`th mutated input: one to three mutations of a quick-tier
/// file, seeded by `index` alone.
fn mutated(files: &[Vec<u8>], index: u64) -> (usize, Vec<u8>) {
    let mut rng = Rng(index.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0xD16_6E5);
    let file = rng.below(files.len());
    let mut bytes = files[file].clone();
    for _ in 0..=rng.below(3) {
        mutate_once(&mut bytes, &mut rng);
    }
    (file, bytes)
}

/// What one attempt ended in.
enum Attempt {
    /// The input did not parse, or could not be played.
    Unreadable,
    /// The input parsed and played to this report.
    Played(RecordingKind, ReplayReport),
}

/// Reads `bytes` and, if they parse, plays them with a tick limit of
/// `max_ticks`, failing the test if the playback hangs, runs past its
/// limit or the error is unplaced or huge.
fn attempt(bytes: &[u8], max_ticks: u32, what: &str) -> Attempt {
    let recording: Recording = match parse(bytes) {
        Ok(recording) => recording,
        Err(error) => {
            check_error(&error, what);
            return Attempt::Unreadable;
        }
    };
    let options = ReplayOptions {
        max_ticks,
        ..ReplayOptions::default()
    };
    let mut replay = match Replay::new(&recording, options) {
        Ok(replay) => replay,
        Err(error) => {
            check_error(&error, what);
            return Attempt::Unreadable;
        }
    };
    let mut frames = 0_u64;
    while let ReplayStep::Running(_) = replay.step(&FrameInput::NONE, &mut NoopObserver) {
        frames += 1;
        assert!(
            frames <= frame_budget(max_ticks),
            "{what}: the playback still runs after {frames} frames"
        );
    }
    let report = replay.finish();
    assert!(
        report.ticks <= max_ticks.saturating_add(1),
        "{what}: {} ticks played with a limit of {max_ticks}",
        report.ticks
    );
    Attempt::Played(recording.kind(), report)
}

/// An error says where the problem is and holds no copy of a huge line.
fn check_error(error: &RecordingError, what: &str) {
    assert!(
        error.location().is_some(),
        "{what}: the error \"{error}\" has no place"
    );
    let message = error.to_string();
    assert!(
        message.len() <= 512,
        "{what}: a {}-byte error message",
        message.len()
    );
}

/// Runs `f`, turning a panic into a test failure that names `what`.
fn no_panic<T>(what: &str, f: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("(no message)");
            panic!("{what}: panicked: {message}")
        }
    }
}

/// Runs `job` for every index in `0..count`, spread over the machine's
/// threads (at most 8), and returns the results in index order.
fn in_parallel<T: Send>(count: usize, job: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .clamp(1, 8);
    let job = &job;
    let mut results: Vec<(usize, T)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|thread| {
                scope.spawn(move || {
                    (thread..count)
                        .step_by(threads)
                        .map(|index| (index, job(index)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| panic!("a worker thread panicked"))
            })
            .collect()
    });
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

#[test]
fn mutated_recordings_never_panic() {
    let files: Vec<Vec<u8>> = QUICK_FILES.iter().map(|name| corpus(name)).collect();
    let count = usize::try_from(MUTATIONS).expect("10000 fits");
    let played = in_parallel(count, |index| {
        let index = u64::try_from(index).expect("a usize fits");
        let (file, bytes) = mutated(&files, index);
        let what = format!("mutation {index} of {}", QUICK_FILES[file]);
        let result = no_panic(&what, || attempt(&bytes, MUTATION_MAX_TICKS, &what));
        matches!(result, Attempt::Played(..))
    });
    let parsed = played.iter().filter(|&&p| p).count();
    assert_eq!(played.len(), count);
    // The generator must exercise both paths, or it tests little.
    assert!(parsed > count / 10, "only {parsed} mutated inputs parsed");
    assert!(parsed < count, "every mutated input parsed");
}

#[test]
fn prefixes_are_never_verified() {
    let files: Vec<Vec<u8>> = QUICK_FILES.iter().map(|name| corpus(name)).collect();
    // Every prefix of 0, 97, 194, ... bytes, and the whole file.
    let jobs: Vec<(usize, usize)> = files
        .iter()
        .enumerate()
        .flat_map(|(file, bytes)| (0..=bytes.len()).step_by(97).map(move |len| (file, len)))
        .collect();
    in_parallel(jobs.len(), |job| {
        let (file, len) = jobs[job];
        let name = QUICK_FILES[file];
        let bytes = &files[file];
        let what = format!("{name}, prefix of {len} bytes");
        let whole = len == bytes.len();
        let result = no_panic(&what, || attempt(&bytes[..len], u32::MAX, &what));
        if let (Attempt::Played(RecordingKind::Edrf, report), false) = (&result, whole) {
            assert!(
                matches!(
                    report.outcome,
                    Outcome::Failed(_) | Outcome::Stopped | Outcome::Truncated
                ),
                "{what}: {:?}",
                report.outcome
            );
        }
    });
}

/// Runs `f` and fails the test if it took a second or more.
fn under_a_second<T>(what: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let value = no_panic(what, f);
    let took = start.elapsed();
    assert!(took < Duration::from_secs(1), "{what}: took {took:?}");
    value
}

/// The DRF header lines of a one-player game on the built-in levels.
fn drf_header() -> String {
    let mut text = String::from("DRF\nAJ DOS 19990506\n1\n20000\n");
    text.push_str(&"\n".repeat(80));
    text
}

#[test]
fn huge_counts_and_lines_are_cheap() {
    const LIMIT: u32 = 300;
    // A huge eDRF repeat count: read without expanding it, and played up
    // to the tick limit.
    let edrf = b"eDRF 2\nRS DIG 20261111\n1\n20000\nR 00000000\nI @4000000000\n";
    let result = under_a_second("I @4000000000", || attempt(edrf, LIMIT, "I @4000000000"));
    let Attempt::Played(_, report) = result else {
        panic!("I @4000000000 did not parse")
    };
    assert_eq!(report.outcome, Outcome::Stopped, "I @4000000000");

    // A huge DRF run count.
    let mut drf = drf_header();
    drf.push_str("00000000s4000000000EOG");
    let result = under_a_second("s4000000000", || {
        attempt(drf.as_bytes(), LIMIT, "s4000000000")
    });
    assert!(
        matches!(result, Attempt::Played(..)),
        "s4000000000 did not parse"
    );

    // A 10 MB line in each place a line can be: the mode line, an eDRF
    // record, the DRF body.
    let huge = "7".repeat(10 * 1024 * 1024);
    let cases = [
        format!("eDRF 2\nRS DIG 20261111\n{huge}\n20000\n"),
        format!("eDRF 2\nRS DIG 20261111\n1\n20000\nR 00000000\nI {huge}\n"),
        format!("eDRF 2\nRS DIG 20261111\n1\n20000\nR 00000000\nC {huge}\n"),
        format!("DRF\nAJ DOS 19990506\n{huge}\n20000\n"),
        format!("{}00000000s{huge}EOG", drf_header()),
        format!("{}0000000{huge}", drf_header()),
    ];
    for (index, text) in cases.iter().enumerate() {
        let what = format!("10 MB line, case {index}");
        let _ = under_a_second(&what, || attempt(text.as_bytes(), LIMIT, &what));
    }

    // A million blank lines, in an eDRF body and after a DRF header.
    let blank = "\n".repeat(1_000_000);
    let cases = [
        format!("eDRF 2\nRS DIG 20261111\n1\n20000\nR 00000000\n{blank}I @32\n"),
        format!("{}{blank}00000000s5EOG", drf_header()),
        blank.clone(),
    ];
    for (index, text) in cases.iter().enumerate() {
        let what = format!("a million blank lines, case {index}");
        let _ = under_a_second(&what, || attempt(text.as_bytes(), LIMIT, &what));
    }
}
