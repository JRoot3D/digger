//! Scenario tests of the game-flow spec (design D11), one per spec
//! scenario, named after it.

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a test failure"
)]

use digger_core::testkit::{Run, Scenario, Script};
use digger_core::{
    FinishReason, FrameInput, FrameKind, GameMode, LevelNumber, ScoreId, SoundCommand,
};

const QUIT: FrameInput = FrameInput {
    quit: true,
    ..FrameInput::NONE
};

/// The run's tick trace line.
fn trace(run: &Run) -> String {
    let mut line = String::new();
    run.state()
        .trace_line(&mut line)
        .expect("writing to a String cannot fail");
    line
}

#[test]
fn quit_during_play_reports_300_ticks() {
    let mut run = Scenario::new(GameMode::OnePlayer)
        .seed(5)
        .build()
        .script(Script::parse("R*40 D*40 L*40 .*180").expect("valid script"));
    assert_eq!(run.run_ticks(300), 300);
    let picture = run.game().framebuffer().clone();
    let reached = ScoreId::ALL.map(|id| {
        let scores = run.state().scores();
        (scores.visible(id), scores.total(id))
    });
    // The script's walk along the bottom row eats emeralds, so the scores
    // reached are not zero.
    assert!(reached[0].0 > 0);
    run.take_sounds();

    let outcome = run.step(&QUIT);
    assert_eq!(
        outcome.frame,
        FrameKind::Wait,
        "the quit frame runs no tick"
    );
    assert_eq!(outcome.player_out, None);
    let summary = outcome.finished.expect("the game finished");
    assert_eq!(summary.reason, FinishReason::Quit);
    assert_eq!(summary.ticks, 300);
    assert_eq!(summary.level, LevelNumber::FIRST);
    for (score, (visible, total)) in summary.scores.into_iter().zip(reached) {
        assert_eq!(score.visible, visible);
        assert_eq!(score.total, total);
    }
    assert_eq!(run.ticks(), 300);
    assert_eq!(run.summary(), Some(summary));
    // All sound stops and no game-over notice is drawn.
    assert_eq!(run.take_sounds(), [SoundCommand::StopAll]);
    assert_eq!(run.game().framebuffer(), &picture);
}

#[test]
fn stepping_a_finished_game_changes_nothing() {
    let mut run = Scenario::new(GameMode::TwoDiggers).seed(9).build();
    run.run_ticks(25);
    let summary = run.step(&QUIT).finished.expect("the game finished");
    run.take_sounds();
    let picture = run.game().framebuffer().clone();
    let line = trace(&run);

    let inputs = Script::parse("RF/LF UP DQ Q . P*3").expect("valid script");
    for input in inputs.inputs() {
        let outcome = run.step(input);
        assert_eq!(outcome.frame, FrameKind::Wait);
        assert_eq!(outcome.player_out, None);
        assert_eq!(outcome.finished, Some(summary));
    }
    assert_eq!(run.game().framebuffer(), &picture);
    assert_eq!(trace(&run), line);
    assert_eq!(run.summary(), Some(summary));
    assert_eq!(run.sounds(), []);
}

#[test]
fn quit_during_level_draw_reports_0_ticks() {
    let mut run = Scenario::new(GameMode::OnePlayer).build();
    assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Wait);
    let summary = run.step(&QUIT).finished.expect("the game finished");
    assert_eq!(summary.reason, FinishReason::Quit);
    assert_eq!(summary.ticks, 0);
    assert_eq!(run.state().round(), 0, "no round was played");
    assert_eq!(run.take_sounds(), [SoundCommand::StopAll]);
}
