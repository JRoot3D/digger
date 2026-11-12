//! DRF to eDRF conversion against upstream's twins (task 8.6 of
//! `add-recording-and-conformance`; game-recording spec, "Re-recording and
//! conversion between formats", scenario "DRF to eDRF matches the upstream
//! twin"; design D9).
//!
//! Each of the corpus's 23 `.drf` recordings is played and recorded as
//! eDRF, and the result is compared with the upstream `.edrf` of the same
//! base name **semantically**: both are parsed and must hold the same
//! level records (number and map), seeds, `C`, `E`, `Q` and `Z` records
//! and the same control value for every slot on every tick. The version
//! line is not compared.
//!
//! The list mirrors the DRF entries of `conformance.rs`, tier and state
//! included: a twin is `pending` (ignored) exactly while its DRF's
//! conformance test is, and `full` twins run with `-- --ignored`
//! (`conversion_twin_list_matches_conformance` keeps the two in step).
//!
//! `twin_byte_identity_*` only report how many converted files are also
//! byte-identical to their twins once line 2 is replaced: byte identity is
//! not a gate (design D9, research 05 §10.4).

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;
use std::path::{Path, PathBuf};

use digger_core::recording::{EdrfRecording, Recording, RecordingKind, parse};
use digger_core::replay::{Replay, ReplayOptions, ReplayStep};
use digger_core::{FrameInput, NoopObserver, SlotId};

/// The corpus's recordings, `tests/recordings/data/`.
fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/recordings/data")
}

/// `<base>.drf` played and recorded as eDRF.
fn convert(base: &str) -> String {
    let path = data().join(format!("{base}.drf"));
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let recording = parse(&bytes).unwrap_or_else(|e| panic!("{base}.drf: {e}"));
    let mut replay =
        Replay::recording_as(&recording, ReplayOptions::default(), RecordingKind::Edrf)
            .unwrap_or_else(|e| panic!("{base}.drf: {e}"));
    while let ReplayStep::Running(_) = replay.step(&FrameInput::NONE, &mut NoopObserver) {}
    let (_, text) = replay.finish_with_recording();
    text.expect("a recorder")
}

/// The upstream twin `<base>.edrf`, as text.
fn twin(base: &str) -> String {
    let path = data().join(format!("{base}.edrf"));
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn edrf(text: &str, what: &str) -> EdrfRecording {
    match parse(text.as_bytes()) {
        Ok(Recording::Edrf(edrf)) => *edrf,
        Ok(Recording::Drf(_)) => panic!("{what}: a DRF"),
        Err(error) => panic!("{what}: {error}"),
    }
}

/// Every control value of `slot`, first tick first.
fn values(recording: &EdrfRecording, slot: SlotId) -> Vec<u8> {
    let mut runs = recording.controls(slot).clone();
    std::iter::from_fn(|| {
        runs.next_value()
            .map(digger_core::recording::ControlValue::bits)
    })
    .collect()
}

/// Where two queues of a kind first differ, if they do.
fn first_difference<T: PartialEq + std::fmt::Debug>(
    what: &str,
    ours: &[T],
    theirs: &[T],
) -> Option<String> {
    if ours == theirs {
        return None;
    }
    let at = ours
        .iter()
        .zip(theirs)
        .position(|(a, b)| a != b)
        .unwrap_or(ours.len().min(theirs.len()));
    Some(format!(
        "{what}: first difference at index {at} (converted {:?}, upstream {:?}; lengths {} and {})",
        ours.get(at),
        theirs.get(at),
        ours.len(),
        theirs.len()
    ))
}

/// The differences between the converted recording and its twin.
fn differences(ours: &EdrfRecording, theirs: &EdrfRecording) -> Vec<String> {
    let mut found = Vec::new();
    let levels = |r: &EdrfRecording| {
        r.levels()
            .iter()
            .map(|l| (l.level, l.layout))
            .collect::<Vec<_>>()
    };
    found.extend(first_difference(
        "level records",
        &levels(ours),
        &levels(theirs),
    ));
    found.extend(first_difference("seeds", ours.seeds(), theirs.seeds()));
    found.extend(first_difference(
        "C records",
        ours.checkpoints(),
        theirs.checkpoints(),
    ));
    found.extend(first_difference(
        "E records",
        ours.round_ends(),
        theirs.round_ends(),
    ));
    if ours.quit() != theirs.quit() {
        found.push(format!(
            "Q: converted {:?}, upstream {:?}",
            ours.quit(),
            theirs.quit()
        ));
    }
    if ours.end() != theirs.end() {
        found.push(format!(
            "Z: converted {:?}, upstream {:?}",
            ours.end(),
            theirs.end()
        ));
    }
    for slot in SlotId::ALL {
        found.extend(first_difference(
            &format!("slot {} controls", slot.number()),
            &values(ours, slot),
            &values(theirs, slot),
        ));
    }
    found
}

/// Converts `<base>.drf` and compares it with its twin.
fn check(base: &str) {
    let converted = convert(base);
    let ours = edrf(&converted, "the converted recording");
    let theirs = edrf(&twin(base), &format!("{base}.edrf"));
    let found = differences(&ours, &theirs);
    assert!(found.is_empty(), "{base}:\n{}", found.join("\n"));
}

/// Whether the converted `<base>.drf` equals its twin byte for byte once
/// both version lines are replaced.
fn byte_identical(base: &str) -> bool {
    let normalise = |text: &str| {
        text.split_inclusive('\n')
            .enumerate()
            .map(|(i, line)| if i == 1 { "VERSION\n" } else { line })
            .collect::<String>()
    };
    normalise(&convert(base)) == normalise(&twin(base))
}

/// Which test run converts a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tier {
    Quick,
    Full,
}

/// Whether the DRF passes its conformance test yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Pending,
    Passing,
}

macro_rules! tier {
    (quick) => {
        Tier::Quick
    };
    (full) => {
        Tier::Full
    };
}

macro_rules! state {
    (pending) => {
        State::Pending
    };
    (passing) => {
        State::Passing
    };
}

/// One test: ignored in the full tier and while pending.
macro_rules! case {
    ($test:ident, $base:literal, quick, passing) => {
        #[test]
        fn $test() {
            check($base);
        }
    };
    ($test:ident, $base:literal, quick, pending) => {
        #[test]
        #[ignore = "pending: the DRF does not pass its conformance test yet"]
        fn $test() {
            check($base);
        }
    };
    ($test:ident, $base:literal, full, $state:ident) => {
        #[test]
        #[ignore = "full tier"]
        fn $test() {
            check($base);
        }
    };
}

macro_rules! twins {
    ($( twin!($test:ident, $base:literal, $tier:ident, $state:ident); )*) => {
        /// Every DRF of the corpus: base name, tier and state.
        const TWINS: &[(&str, Tier, State)] = &[ $( ($base, tier!($tier), state!($state)), )* ];
        $( case!($test, $base, $tier, $state); )*
    };
}

twins! {
    twin!(twin_2test01, "2test01", quick, passing);
    twin!(twin_dror0, "dror0", quick, passing);
    twin!(twin_muu27700, "muu27700", quick, passing);
    twin!(twin_ramon, "ramon", quick, passing);
    twin!(twin_sobodig, "sobodig", quick, passing);
    twin!(twin_thevoid, "thevoid", quick, passing);
    twin!(twin_thevoids, "thevoids", quick, passing);
    twin!(twin_a1104675, "a1104675", full, passing);
    twin!(twin_a1195000, "a1195000", full, passing);
    twin!(twin_a1262500, "a1262500", full, passing);
    twin!(twin_b1072175, "b1072175", full, passing);
    twin!(twin_f1172525, "f1172525", full, passing);
    twin!(twin_g1322150, "g1322150", full, passing);
    twin!(twin_h1266775, "h1266775", full, passing);
    twin!(twin_k1859075, "k1859075", full, passing);
    twin!(twin_k2223700, "k2223700", full, passing);
    twin!(twin_m1143925, "m1143925", full, passing);
    twin!(twin_p1092525, "p1092525", full, passing);
    twin!(twin_r2222400, "r2222400", full, passing);
    twin!(twin_r3333300, "r3333300", full, passing);
    twin!(twin_s1058800, "s1058800", full, passing);
    twin!(twin_s1177650, "s1177650", full, passing);
    twin!(twin_s1302025, "s1302025", full, passing);
}

#[test]
fn conversion_twin_list_matches_conformance() {
    let source = include_str!("conformance.rs");
    let mut listed: Vec<(String, Tier, State)> = source
        .lines()
        .filter_map(|line| {
            let args = line
                .trim()
                .strip_prefix("recording!(")?
                .strip_suffix(");")?;
            let fields: Vec<&str> = args.split(", ").collect();
            let [_, file, tier, state] = fields.as_slice() else {
                return None;
            };
            let base = file.trim_matches('"').strip_suffix(".drf")?;
            let tier = if *tier == "quick" {
                Tier::Quick
            } else {
                Tier::Full
            };
            let state = if *state == "pending" {
                State::Pending
            } else {
                State::Passing
            };
            Some((base.to_owned(), tier, state))
        })
        .collect();
    listed.sort_by(|a, b| a.0.cmp(&b.0));
    let mut twins: Vec<(String, Tier, State)> = TWINS
        .iter()
        .map(|&(base, tier, state)| (base.to_owned(), tier, state))
        .collect();
    twins.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(twins.len(), 23);
    assert_eq!(twins, listed, "the twin list and conformance.rs differ");
}

/// Reports how many of `tier`'s passing twins are byte-identical.
fn report_byte_identity(tiers: &[Tier]) {
    let bases: Vec<&str> = TWINS
        .iter()
        .filter(|(_, tier, state)| tiers.contains(tier) && *state == State::Passing)
        .map(|(base, _, _)| *base)
        .collect();
    let identical: Vec<&str> = bases
        .iter()
        .copied()
        .filter(|b| byte_identical(b))
        .collect();
    println!(
        "byte-identical after replacing line 2: {} of {} ({identical:?})",
        identical.len(),
        bases.len()
    );
}

#[test]
fn twin_byte_identity_quick() {
    report_byte_identity(&[Tier::Quick]);
}

#[test]
#[ignore = "full tier"]
fn twin_byte_identity_full() {
    report_byte_identity(&[Tier::Quick, Tier::Full]);
}
