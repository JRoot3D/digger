//! Soak test (design D11; task 15.3).
//!
//! Every mode is run for 50,000 frames on inputs from a test-local
//! generator, in the debug build the test suite uses, so integer overflow
//! and bounds checks are on. Whenever a game finishes, the next one starts
//! with a fresh base seed and a random start level, lives option and
//! gauntlet time, so long runs reach high levels and every way a game can
//! end. Besides "no panic", every frame is checked against the stepping
//! contract: a tick frame adds exactly one tick, other frames add none, and
//! the game never stalls without ticking or finishing.

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a test failure"
)]

#[path = "common/inputs.rs"]
mod inputs;

use digger_core::{FrameKind, Game, GameConfig, GameMode, GauntletSeconds, StartLevel};

use inputs::InputGen;

/// Frames run per mode.
const FRAMES: u32 = 50_000;

/// The longest run of waiting frames a game may have between two ticks:
/// a full level draw of a busy level, the level-complete pause and a
/// notice, with room to spare. Paused frames are not counted.
const MAX_WAIT_FRAMES: u32 = 400;

/// What happened during a soak.
#[derive(Debug, Default)]
struct Totals {
    games: u32,
    finished: u32,
    ticks: u64,
    highest_level: u16,
}

/// The mode of `kind` with a gauntlet time drawn from `generator`.
fn mode_for(kind: u8, generator: &mut InputGen) -> GameMode {
    let time = GauntletSeconds::new(generator.below(200));
    match kind {
        0 => GameMode::OnePlayer,
        1 => GameMode::TwoPlayers,
        2 => GameMode::TwoDiggers,
        3 => GameMode::Gauntlet { time },
        _ => GameMode::GauntletTwoDiggers { time },
    }
}

/// A new game of mode `kind` with settings drawn from `generator`.
fn new_game(kind: u8, generator: &mut InputGen) -> Box<Game> {
    let start = match generator.below(4) {
        0 => 1 + generator.below(1000),
        1 => 1 + generator.below(20),
        _ => 1,
    };
    let config = GameConfig {
        mode: mode_for(kind, generator),
        start_level: StartLevel::new(i64::from(start)).expect("1..=1000"),
        unlimited_lives: generator.below(3) == 0,
        ..GameConfig::default()
    };
    Game::new(config, generator.next_u64())
}

fn soak(kind: u8) -> Totals {
    let mut generator = InputGen::new(0x50AC + u64::from(kind));
    let mut totals = Totals::default();
    let mut game = new_game(kind, &mut generator);
    totals.games = 1;
    let mut waits = 0;
    let mut drain = true;
    let mut paused = false;
    for frame in 0..FRAMES {
        let mut input = generator.controls();
        // Now and then a pause, resumed a few dozen frames later, and
        // rarely a quit.
        input.pause = generator.below(if paused { 40 } else { 400 }) == 0;
        input.quit = generator.below(20_000) == 0;

        let before = game.state().ticks();
        let outcome = game.step(&input);
        let after = game.state().ticks();
        paused = outcome.frame == FrameKind::Paused;
        let expected = u32::from(outcome.frame == FrameKind::Tick);
        assert_eq!(
            after,
            before + expected,
            "mode {kind}, frame {frame}: {outcome:?}"
        );

        for player in digger_core::PlayerId::ALL {
            if let Some(level) = game.state().level(player) {
                totals.highest_level = totals.highest_level.max(level.get());
            }
        }
        if drain {
            game.drain_sound().for_each(drop);
        }
        match outcome.frame {
            FrameKind::Tick => waits = 0,
            FrameKind::Paused => {}
            FrameKind::Wait => waits += 1,
        }

        if let Some(summary) = outcome.finished {
            assert_eq!(summary.ticks, after);
            totals.finished += 1;
            totals.ticks += u64::from(after);
            game = new_game(kind, &mut generator);
            totals.games += 1;
            drain = !drain;
            waits = 0;
        } else {
            assert!(
                waits <= MAX_WAIT_FRAMES,
                "mode {kind}, frame {frame}: {waits} waiting frames without a tick"
            );
        }
    }
    totals.ticks += u64::from(game.state().ticks());
    totals
}

fn check(kind: u8) {
    let totals = soak(kind);
    // Many games were played to their end, most frames were ticks, and
    // some games reached high levels.
    assert!(totals.finished >= 5, "mode {kind}: {totals:?}");
    assert!(
        totals.ticks > u64::from(FRAMES) / 3,
        "mode {kind}: {totals:?}"
    );
    assert!(totals.highest_level > 100, "mode {kind}: {totals:?}");
}

#[test]
fn soak_one_player() {
    check(0);
}

#[test]
fn soak_two_players() {
    check(1);
}

#[test]
fn soak_two_diggers() {
    check(2);
}

#[test]
fn soak_gauntlet() {
    check(3);
}

#[test]
fn soak_gauntlet_two_diggers() {
    check(4);
}
