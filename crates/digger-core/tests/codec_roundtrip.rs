//! Round trips through the recording codecs (task 8.4 of
//! `add-recording-and-conformance`; game-recording spec, "Writing a DRF
//! recording", "Writing an eDRF recording", "Re-recording and conversion
//! between formats").
//!
//! A scripted live game in each mode is recorded as DRF and as eDRF at
//! once. Each recording is then played back while being recorded again in
//! its own format: the playback must end as the live game did and the
//! second text must be byte-for-byte the first. An eDRF played back and
//! recorded as DRF must replay to the same result as the eDRF.
//!
//! The live script presses fire only on ticks on which the slot's digger
//! takes a movement sample: a fire pressed on another tick takes effect
//! without being part of any DRF record (quirk Q15), so no DRF can hold
//! it.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

#[path = "common/inputs.rs"]
mod inputs;
#[path = "common/live.rs"]
mod live;

use digger_core::recording::{RecordingKind, parse};
use digger_core::replay::{Outcome, Replay, ReplayOptions, ReplayReport, ReplayStep};
use digger_core::{
    FrameInput, GameConfig, GameMode, GameSummary, GauntletSeconds, NoopObserver, StartLevel,
};

use live::live;

/// Plays `text` back while recording it as `kind`; the report and the
/// new recording.
fn replay(text: &str, kind: RecordingKind) -> (ReplayReport, String) {
    let recording = parse(text.as_bytes()).expect("a valid recording");
    let mut replay =
        Replay::recording_as(&recording, ReplayOptions::default(), kind).expect("playable");
    while let ReplayStep::Running(_) = replay.step(&FrameInput::NONE, &mut NoopObserver) {}
    let (report, text) = replay.finish_with_recording();
    (report, text.expect("a recorder"))
}

/// Whether `report` ends as `summary` does.
fn assert_same_end(report: &ReplayReport, summary: &GameSummary, what: &str) {
    assert_eq!(
        report.scores,
        summary.scores.map(|score| score.total),
        "{what}: scores"
    );
    assert_eq!(report.level, summary.level.get(), "{what}: level");
    assert_eq!(report.ticks, summary.ticks, "{what}: ticks");
}

fn config(mode: GameMode) -> GameConfig {
    GameConfig {
        mode,
        ..GameConfig::default()
    }
}

fn gauntlet() -> GameConfig {
    config(GameMode::Gauntlet {
        time: GauntletSeconds::new(90),
    })
}

fn start_level_5() -> GameConfig {
    GameConfig {
        start_level: StartLevel::new(5).expect("a start level"),
        ..GameConfig::default()
    }
}

fn roundtrip_drf(config: &GameConfig, seed: u64) {
    let (summary, drf, _) = live(config, seed);
    assert!(summary.ticks > 300, "a short game: {summary:?}");
    assert!(drf.matches("\nEOL\n").count() > 1, "one round only");
    let (report, again) = replay(&drf, RecordingKind::Drf);
    assert_eq!(report.outcome, Outcome::Completed, "{:?}", config.mode);
    assert_same_end(&report, &summary, "DRF");
    assert!(
        again == drf,
        "{:?}: the re-recorded DRF differs",
        config.mode
    );
}

fn roundtrip_edrf(config: &GameConfig, seed: u64) {
    let (summary, _, edrf) = live(config, seed);
    assert!(summary.ticks > 300, "a short game: {summary:?}");
    assert!(edrf.matches("\nE ").count() > 1, "one round only");
    assert!(edrf.contains("\nZ "), "no end record");
    let (report, again) = replay(&edrf, RecordingKind::Edrf);
    assert_eq!(report.outcome, Outcome::Verified, "{:?}", config.mode);
    assert_same_end(&report, &summary, "eDRF");
    assert!(
        again == edrf,
        "{:?}: the re-recorded eDRF differs",
        config.mode
    );
}

#[test]
fn roundtrip_drf_one_player() {
    roundtrip_drf(&GameConfig::default(), 21);
}

#[test]
fn roundtrip_drf_alternating() {
    roundtrip_drf(&config(GameMode::TwoPlayers), 22);
}

#[test]
fn roundtrip_drf_two_diggers() {
    roundtrip_drf(&config(GameMode::TwoDiggers), 23);
}

#[test]
fn roundtrip_drf_gauntlet() {
    roundtrip_drf(&gauntlet(), 24);
}

#[test]
fn roundtrip_drf_start_level_5() {
    roundtrip_drf(&start_level_5(), 25);
}

#[test]
fn roundtrip_edrf_one_player() {
    roundtrip_edrf(&GameConfig::default(), 21);
}

#[test]
fn roundtrip_edrf_alternating() {
    roundtrip_edrf(&config(GameMode::TwoPlayers), 22);
}

#[test]
fn roundtrip_edrf_two_diggers() {
    roundtrip_edrf(&config(GameMode::TwoDiggers), 23);
}

#[test]
fn roundtrip_edrf_gauntlet() {
    roundtrip_edrf(&gauntlet(), 24);
}

#[test]
fn roundtrip_edrf_start_level_5() {
    roundtrip_edrf(&start_level_5(), 25);
}

#[test]
fn edrf_to_drf_same_result() {
    let configs = [
        GameConfig::default(),
        config(GameMode::TwoPlayers),
        config(GameMode::TwoDiggers),
        gauntlet(),
        config(GameMode::GauntletTwoDiggers {
            time: GauntletSeconds::new(60),
        }),
        start_level_5(),
    ];
    for (config, seed) in configs.iter().zip(31..) {
        let (summary, _, edrf) = live(config, seed);
        let (edrf_report, drf) = replay(&edrf, RecordingKind::Drf);
        assert_eq!(edrf_report.outcome, Outcome::Verified, "{:?}", config.mode);
        let (drf_report, _) = replay(&drf, RecordingKind::Drf);
        assert_eq!(drf_report.outcome, Outcome::Completed, "{:?}", config.mode);
        assert_same_end(&drf_report, &summary, "eDRF to DRF");
        assert_eq!(
            (drf_report.scores, drf_report.level, drf_report.ticks),
            (edrf_report.scores, edrf_report.level, edrf_report.ticks),
            "{:?}",
            config.mode
        );
        // And back: the DRF converted to eDRF verifies against nothing
        // but replays to the same end.
        let (_, edrf_again) = replay(&drf, RecordingKind::Edrf);
        let (report, _) = replay(&edrf_again, RecordingKind::Edrf);
        assert_eq!(report.outcome, Outcome::Verified, "{:?}", config.mode);
        assert_same_end(&report, &summary, "eDRF to DRF to eDRF");
    }
}
