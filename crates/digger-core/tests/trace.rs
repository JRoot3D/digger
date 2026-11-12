//! The per-tick state trace (task 10.1 of `add-recording-and-conformance`;
//! replay-verification spec, "Per-tick state trace for triage"; design
//! D12).

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;
use std::path::Path;

use digger_core::recording::{Recording, parse};
use digger_core::replay::{
    Replay, ReplayOptions, ReplayReport, ReplayStep, TraceField, TraceLine, TraceOptions, verify,
};
use digger_core::{FrameInput, NoopObserver};

fn corpus(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/recordings/data")
        .join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Plays `recording` to its end with `trace` and returns the report and
/// the whole trace text.
fn traced(recording: &Recording, trace: TraceOptions) -> (ReplayReport, String) {
    let mut replay = Replay::new(recording, ReplayOptions::default()).expect("playable");
    replay.set_trace(trace);
    let mut text = String::new();
    while let ReplayStep::Running(_) = replay.step(&FrameInput::NONE, &mut NoopObserver) {
        text.push_str(&replay.take_trace());
    }
    text.push_str(&replay.take_trace());
    (replay.finish(), text)
}

#[test]
fn trace_window_has_256_lines() {
    let recording = parse(&corpus("dror0.edrf")).expect("a recording");
    let (report, text) = traced(&recording, TraceOptions::ticks(513, 768));
    assert_eq!(report.exit_status(), 0, "{report:?}");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 256);
    let parsed: Vec<TraceLine> = lines
        .iter()
        .map(|l| TraceLine::parse(l).unwrap_or_else(|| panic!("a trace line: {l}")))
        .collect();
    let ticks: Vec<u32> = parsed.iter().map(|l| l.tick).collect();
    assert_eq!(ticks, (513..=768).collect::<Vec<_>>());
    for line in &lines {
        for block in ["playfield=", "digger=", "monster=", "bag="] {
            assert!(line.contains(block), "{line}");
        }
    }
    // Tick 768 is the third tick checkpoint: its hash is the recorded one.
    let Recording::Edrf(edrf) = &recording else {
        panic!("an eDRF")
    };
    let recorded = edrf.checkpoints().get(2).copied();
    assert_eq!(parsed.last().map(|l| l.hash), recorded);
}

#[test]
fn trace_with_fields_decodes_every_value() {
    let recording = parse(&corpus("dror0.edrf")).expect("a recording");
    let (_, text) = traced(
        &recording,
        TraceOptions {
            from: 100,
            to: 101,
            fields: true,
        },
    );
    let tick_lines = text.lines().filter_map(TraceLine::parse).count();
    assert_eq!(tick_lines, 2);
    let fields: Vec<TraceField> = text.lines().filter_map(TraceField::parse).collect();
    assert_eq!(
        tick_lines + fields.len(),
        text.lines().count(),
        "every line is a tick line or a field line"
    );
    assert!(fields.iter().any(|f| f.name == "game.rng_state"), "{text}");
}

#[test]
fn tracing_is_passive() {
    // The decoded dump is the costly part; one recording traces it.
    for (name, fields) in [
        ("dror0.edrf", true),
        ("dror0.drf", false),
        ("2test01.edrf", false),
    ] {
        let bytes = corpus(name);
        let plain = verify(&bytes, ReplayOptions::default()).expect("playable");
        let recording = parse(&bytes).expect("a recording");
        let (with_trace, text) = traced(
            &recording,
            TraceOptions {
                from: 1,
                to: u32::MAX,
                fields,
            },
        );
        assert_eq!(with_trace, plain, "{name}");
        assert_eq!(with_trace.result_line(), plain.result_line(), "{name}");
        let lines = text.lines().filter_map(TraceLine::parse).count();
        assert_eq!(
            u32::try_from(lines).ok(),
            Some(plain.ticks),
            "{name}: one line per tick"
        );
    }
}
