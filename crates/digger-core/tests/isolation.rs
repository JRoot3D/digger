//! Configuration isolation during playback (task 9.2 of
//! `add-recording-and-conformance`; replay-verification spec,
//! "Configuration isolation during playback"; game-recording spec,
//! "Playback settings come only from the recording"; design D7, quirk P9).
//!
//! The playback driver builds its configuration from the recording alone,
//! so there is no way to hand it the caller's settings. These tests make
//! that observable: each corpus recording is verified once as a baseline
//! and once while the caller holds a deliberately hostile configuration
//! and plays a live game with it, frame for frame, beside the playback.
//! The reports must be identical and the caller's configuration
//! untouched afterwards.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;
use std::path::Path;

use digger_core::recording::parse;
use digger_core::replay::{Replay, ReplayOptions, ReplayReport, ReplayStep, verify};
use digger_core::{
    DiggerId, FrameInput, Game, GameConfig, GameMode, GauntletSeconds, LevelLayout, LevelNumber,
    LevelSet, Observer, PlayerId, StartLevel, StateView,
};

/// Recordings of the corpus that pass: one and two players, DRF and eDRF.
const RECORDINGS: [&str; 4] = ["sobodig.edrf", "sobodig.drf", "2test01.edrf", "ramon.drf"];

fn corpus(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/recordings/data")
        .join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The baseline report: `name` verified with nothing else going on.
fn baseline(name: &str) -> ReplayReport {
    verify(&corpus(name), ReplayOptions::default()).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// A quick-tier recording in which the player reaches the five-life cap and
/// would earn more lives without it (replay-verification spec, "Unlimited
/// lives does not leak in").
const AT_THE_CAP: &str = "thevoids.edrf";

/// The levels the playback starts, in order, and the most lives any digger
/// had at the end of a tick.
#[derive(Default)]
struct Levels(Vec<u16>, u32);

impl Observer for Levels {
    fn tick_end(&mut self, _tick: u32, state: StateView<'_>) {
        for digger in DiggerId::ALL {
            self.1 = self.1.max(state.lives(digger));
        }
    }

    fn level_plan(
        &mut self,
        _player: PlayerId,
        level: LevelNumber,
        _layout: &LevelLayout,
    ) -> Option<LevelLayout> {
        self.0.push(level.get());
        None
    }
}

/// `name` played back while the caller holds `caller` and steps a live
/// game under it beside the playback; the report, the levels played and
/// the most lives a digger had.
fn beside(name: &str, caller: &GameConfig) -> (ReplayReport, Vec<u16>, u32) {
    let before = caller.clone();
    let recording = parse(&corpus(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    let mut replay = Replay::new(&recording, ReplayOptions::default()).expect("playable");
    let mut live = Game::new(caller.clone(), 12345);
    let mut levels = Levels::default();
    loop {
        live.step(&FrameInput::NONE);
        if let ReplayStep::Finished(_) = replay.step(&FrameInput::NONE, &mut levels) {
            break;
        }
    }
    let report = replay.finish();
    assert_eq!(
        *caller, before,
        "{name}: the caller's configuration changed"
    );
    (report, levels.0, levels.1)
}

/// Asserts that every recording replays under `caller` exactly as in the
/// baseline.
fn assert_isolated(caller: &GameConfig) {
    for name in RECORDINGS {
        let (report, levels, _) = beside(name, caller);
        assert_eq!(report, baseline(name), "{name} under {caller:?}");
        assert_eq!(levels.first(), Some(&1), "{name}: starts on level 1");
    }
}

#[test]
fn isolation_unlimited_lives() {
    let caller = GameConfig {
        unlimited_lives: true,
        ..GameConfig::default()
    };
    assert_isolated(&caller);

    // A recording that reaches the cap: the cap stays in force.
    let (report, _, most) = beside(AT_THE_CAP, &caller);
    assert_eq!(report, baseline(AT_THE_CAP), "{AT_THE_CAP}");
    assert_eq!(most, 5, "{AT_THE_CAP}: the five-life cap");
}

#[test]
fn isolation_start_level() {
    let caller = GameConfig {
        start_level: StartLevel::new(5).expect("a start level"),
        ..GameConfig::default()
    };
    assert_isolated(&caller);
}

#[test]
fn isolation_mode() {
    for mode in [
        GameMode::Gauntlet {
            time: GauntletSeconds::new(30),
        },
        GameMode::TwoDiggers,
        GameMode::TwoPlayers,
    ] {
        let caller = GameConfig {
            mode,
            ..GameConfig::default()
        };
        assert_isolated(&caller);
    }
}

#[test]
fn isolation_bonus_interval() {
    let builtin = LevelSet::builtin();
    let caller = GameConfig {
        levels: LevelSet::from_plans(*builtin.layouts(), 500, false),
        ..GameConfig::default()
    };
    assert_isolated(&caller);
}

#[test]
fn isolation_everything_at_once() {
    let builtin = LevelSet::builtin();
    let caller = GameConfig {
        mode: GameMode::GauntletTwoDiggers {
            time: GauntletSeconds::new(30),
        },
        start_level: StartLevel::new(5).expect("a start level"),
        unlimited_lives: true,
        levels: LevelSet::from_plans(*builtin.layouts(), 500, false),
    };
    assert_isolated(&caller);
}
