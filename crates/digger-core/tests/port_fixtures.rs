//! Port-made regression fixtures for the modes upstream never recorded
//! (task 13.3 of `add-recording-and-conformance`; design, "Risks / Trade-
//! offs": no alternating two-player game and no start level in the
//! corpus).
//!
//! `tests/recordings/port/` holds one alternating two-player game and one
//! game started on level 5, each as DRF and eDRF, recorded by the port
//! from scripted controls: the alternating game from a bot that sweeps
//! the field row by row firing every third frame (player 1 clears level
//! 1), the level-5 game from the generator of `common/inputs.rs`. They are not
//! upstream-verified: they pin the port's own behaviour so that a later
//! change cannot alter these modes unnoticed.
//!
//! - `port_fixture_alternating` and `port_fixture_start_level` replay the
//!   files: the eDRF must verify against its own checkpoints, the DRF
//!   (which has none) must play to its end, and both must give the result
//!   line of the fixture's `.out` file.
//! - `port_fixtures_regenerate_identically` plays the script again and
//!   compares the recordings byte-for-byte with the files. Run it with
//!   `DIGGER_WRITE_PORT_FIXTURES=1` to rewrite the files instead, after a
//!   deliberate change to the simulation or the writers.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

#[path = "common/inputs.rs"]
mod inputs;
#[path = "common/live.rs"]
mod live;

use std::fs;
use std::path::{Path, PathBuf};

use digger_core::recording::parse;
use digger_core::replay::{Outcome, ReplayOptions, playback_config, verify};
use digger_core::{
    Controls, DiggerId, FrameInput, GameConfig, GameMode, GameSummary, StartLevel, StateView,
};

use inputs::InputGen;
use live::live_with;

/// Which controls a fixture's game is played with.
#[derive(Clone, Copy)]
enum Script {
    /// [`sweep`], firing on frames whose number plus this is a multiple
    /// of 3.
    Sweep(u64),
    /// The generator of `common/inputs.rs` with this seed.
    Generated(u64),
}

/// One fixture: its base name, the game it records, the game's seed and
/// its script.
struct Fixture {
    base: &'static str,
    config: fn() -> GameConfig,
    seed: u64,
    script: Script,
    mode_line: &'static str,
    min_rounds: usize,
}

/// Column `0..15` and row `0..10` of the cell at `index` of a serpentine
/// path over the field: row 0 left to right, row 1 right to left, ...
fn serpentine(index: usize) -> (i32, i32) {
    let row = (index / 15) % 10;
    let col = index % 15;
    let col = if row.is_multiple_of(2) { col } else { 14 - col };
    let (col, row) = (
        i32::try_from(col).unwrap_or_default(),
        i32::try_from(row).unwrap_or_default(),
    );
    (12 + 20 * col, 18 + 18 * row)
}

/// A bot for slot 0: it walks the serpentine path cell by cell (turning
/// vertically only on a column line) and presses fire on every third
/// frame. It plays whichever digger slot 0 controls, so in an
/// alternating game it plays both players' turns.
fn sweep(phase: u64) -> impl FnMut(StateView<'_>) -> FrameInput {
    let mut target = 0;
    let mut frame = phase;
    move |state| {
        frame += 1;
        let index = if state.mode().alternating() {
            usize::from(state.current_player().number())
        } else {
            0
        };
        let digger = DiggerId::ALL
            .get(index)
            .copied()
            .unwrap_or(DiggerId::ALL[0]);
        let at = state.digger(digger).position();
        if (at.x, at.y) == serpentine(target) {
            target = (target + 1) % 150;
        }
        let (x, y) = serpentine(target);
        let mut controls = Controls::NONE;
        if at.y != y && (at.x - 12) % 20 == 0 || at.x == x {
            if at.y < y {
                controls.down = true;
            } else {
                controls.up = true;
            }
        } else if at.x < x {
            controls.right = true;
        } else {
            controls.left = true;
        }
        controls.fire = frame.is_multiple_of(3);
        let mut input = FrameInput::NONE;
        input.slots[0] = controls;
        input
    }
}

/// The game of `fixture`: its summary, DRF and eDRF.
fn record(fixture: &Fixture) -> (GameSummary, String, String) {
    let config = (fixture.config)();
    match fixture.script {
        Script::Sweep(phase) => live_with(&config, fixture.seed, sweep(phase)),
        Script::Generated(seed) => {
            let mut generator = InputGen::new(seed);
            live_with(&config, fixture.seed, |_| generator.controls())
        }
    }
}

fn alternating() -> GameConfig {
    GameConfig {
        mode: GameMode::TwoPlayers,
        ..GameConfig::default()
    }
}

fn start_level_5() -> GameConfig {
    GameConfig {
        start_level: StartLevel::new(5).expect("a start level"),
        ..GameConfig::default()
    }
}

const ALTERNATING: Fixture = Fixture {
    base: "alternating",
    config: alternating,
    seed: 11,
    script: Script::Sweep(11),
    mode_line: "2",
    min_rounds: 7,
};

const START_LEVEL: Fixture = Fixture {
    base: "start-level-5",
    config: start_level_5,
    seed: 272,
    script: Script::Generated(272),
    mode_line: "1I5",
    min_rounds: 3,
};

/// `tests/recordings/port/`.
fn port_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/recordings/port")
}

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Replays both files of `fixture` and checks them against its `.out`.
fn check(fixture: &Fixture) {
    let dir = port_dir();
    let expected_path = dir.join(format!("{}.out", fixture.base));
    let expected = String::from_utf8(read(&expected_path)).expect("a UTF-8 result line");
    let config = (fixture.config)();
    for (ext, outcome) in [("edrf", Outcome::Verified), ("drf", Outcome::Completed)] {
        let file = format!("{}.{ext}", fixture.base);
        let bytes = read(&dir.join(&file));
        let text = String::from_utf8_lossy(&bytes);
        assert_eq!(
            text.lines().nth(2),
            Some(fixture.mode_line),
            "{file}: mode line"
        );
        let recording = parse(&bytes).unwrap_or_else(|e| panic!("{file}: {e}"));
        let played = playback_config(&recording);
        assert_eq!(played.mode, config.mode, "{file}: mode");
        assert_eq!(
            played.start_level, config.start_level,
            "{file}: start level"
        );
        let report = verify(&bytes, ReplayOptions::default())
            .unwrap_or_else(|e| panic!("{file}: not playable: {e}"));
        assert_eq!(
            report.exit_status(),
            0,
            "{file}\n{}",
            report.diagnostics(&file)
        );
        assert_eq!(report.outcome, outcome, "{file}: outcome");
        assert_eq!(report.result_line(), expected, "{file}: result line");
    }
}

#[test]
fn port_fixture_alternating() {
    check(&ALTERNATING);
}

#[test]
fn port_fixture_start_level() {
    check(&START_LEVEL);
}

#[test]
fn port_fixtures_regenerate_identically() {
    let write = std::env::var_os("DIGGER_WRITE_PORT_FIXTURES").is_some();
    let dir = port_dir();
    for fixture in [ALTERNATING, START_LEVEL] {
        let (summary, drf, edrf) = record(&fixture);
        let config = (fixture.config)();
        assert!(
            edrf.matches("\nE ").count() >= fixture.min_rounds,
            "{}: fewer than {} rounds",
            fixture.base,
            fixture.min_rounds
        );
        if config.mode == GameMode::TwoPlayers {
            assert!(
                summary.scores.iter().all(|score| score.total > 0),
                "{}: a player never scored: {summary:?}",
                fixture.base
            );
            assert!(
                summary.level.get() >= 2,
                "{}: no level cleared",
                fixture.base
            );
        }
        let report = verify(edrf.as_bytes(), ReplayOptions::default()).expect("playable");
        let files = [
            (format!("{}.drf", fixture.base), drf),
            (format!("{}.edrf", fixture.base), edrf),
            (format!("{}.out", fixture.base), report.result_line()),
        ];
        for (name, text) in files {
            let path = dir.join(&name);
            if write {
                fs::write(&path, &text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            } else {
                assert!(
                    read(&path) == text.as_bytes(),
                    "{name} differs from a fresh recording of its script \
                     (DIGGER_WRITE_PORT_FIXTURES=1 rewrites it)"
                );
            }
        }
    }
}
