//! The observer seam that recording, playback and verification are built
//! on (task group 1 of `add-recording-and-conformance`, design D4).
//!
//! These tests pin the call points of [`Observer`]'s hooks that the
//! recorders, the verifier and the playback sources rely on, and check that
//! [`ReplayObserver`] with no components changes nothing. The mapping of
//! upstream's recorder and player hooks onto these call points is the
//! "Simulation seam" section of `docs/conformance.md`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

#[path = "common/inputs.rs"]
mod inputs;

use digger_core::replay::ReplayObserver;
use digger_core::testkit::{KillCause, Run, Scenario, Script};
use digger_core::{
    Controls, DiggerId, Direction, FrameInput, FrameKind, Game, GameConfig, GameMode,
    GauntletSeconds, LevelLayout, LevelNumber, MonsterSlot, Movement, Observer, PlayerId, SlotId,
    SoundCommand, StateView, StepOutcome,
};

use digger_core::geom::Px;
use inputs::InputGen;

const FIRST: DiggerId = DiggerId::ALL[0];
const P1: PlayerId = PlayerId::ALL[0];
const P2: PlayerId = PlayerId::ALL[1];
const S0: SlotId = SlotId::ALL[0];
const S1: SlotId = SlotId::ALL[1];

/// The most frames a scripted game may take; every one finishes well
/// before.
const MAX_FRAMES: usize = 8000;

fn modes() -> [(GameMode, u64); 5] {
    let time = GauntletSeconds::new(60);
    [
        (GameMode::OnePlayer, 21),
        (GameMode::TwoPlayers, 22),
        (GameMode::TwoDiggers, 23),
        (GameMode::Gauntlet { time }, 24),
        (GameMode::GauntletTwoDiggers { time }, 25),
    ]
}

fn trace(state: StateView<'_>) -> String {
    let mut line = String::new();
    state
        .trace_line(&mut line)
        .unwrap_or_else(|_| unreachable!("writing to a String cannot fail"));
    line
}

/// Everything one frame produced.
#[derive(Debug, PartialEq, Eq)]
struct Frame {
    outcome: StepOutcome,
    trace: String,
    crc: u32,
    sounds: Vec<SoundCommand>,
}

/// Plays a scripted game in `mode`, through [`Game::step_observed`] with
/// `obs` when one is given and through [`Game::step`] otherwise.
fn play(mode: GameMode, seed: u64, mut obs: Option<&mut dyn Observer>) -> Vec<Frame> {
    let config = GameConfig {
        mode,
        ..GameConfig::default()
    };
    let mut game = Game::new(config, seed);
    let mut generator = InputGen::new(seed);
    let mut frames = Vec::new();
    for _ in 0..MAX_FRAMES {
        let input = generator.controls();
        let outcome = match obs.as_deref_mut() {
            Some(obs) => game.step_observed(&input, obs),
            None => game.step(&input),
        };
        frames.push(Frame {
            outcome,
            trace: trace(game.state()),
            crc: game.framebuffer().crc32(),
            sounds: game.drain_sound().collect(),
        });
        if outcome.finished.is_some() {
            return frames;
        }
    }
    panic!("{mode:?}: the game did not finish in {MAX_FRAMES} frames");
}

#[test]
fn empty_replay_observer_changes_nothing() {
    for (mode, seed) in modes() {
        let plain = play(mode, seed, None);
        let mut obs = ReplayObserver::new();
        let observed = play(mode, seed, Some(&mut obs));
        assert!(plain.len() > 500, "{mode:?}: a short game");
        assert_eq!(plain.len(), observed.len(), "{mode:?}: frame counts");
        for (n, (a, b)) in plain.iter().zip(&observed).enumerate() {
            assert_eq!(a, b, "{mode:?}: frame {n} differs");
        }
        let summary = plain.last().and_then(|f| f.outcome.finished);
        assert!(summary.is_some(), "{mode:?}: finished");
    }
}

// --- 1.3 Hook order of one round --------------------------------------------

/// One observer call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    LevelPlan(PlayerId, LevelNumber),
    Seed(u32),
    Controls(u32, SlotId),
    Movement(SlotId),
    TickEnd(u32),
    /// The round end, with the tick count and whether any monster was
    /// still shown.
    RoundEnd {
        ticks: u32,
        monsters: bool,
    },
    GameEnd {
        ticks: u32,
    },
}

/// Logs every observer call and replaces nothing.
#[derive(Debug, Default)]
struct Log(Vec<Event>);

impl Observer for Log {
    fn round_seed(&mut self, round: u32, derived: i32) -> i32 {
        self.0.push(Event::Seed(round));
        derived
    }

    fn level_plan(
        &mut self,
        player: PlayerId,
        level: LevelNumber,
        _layout: &LevelLayout,
    ) -> Option<LevelLayout> {
        self.0.push(Event::LevelPlan(player, level));
        None
    }

    fn controls(&mut self, tick: u32, slot: SlotId, raw: Controls) -> Controls {
        self.0.push(Event::Controls(tick, slot));
        raw
    }

    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement {
        self.0.push(Event::Movement(slot));
        sample
    }

    fn tick_end(&mut self, tick: u32, _state: StateView<'_>) {
        self.0.push(Event::TickEnd(tick));
    }

    fn round_end(&mut self, state: StateView<'_>) {
        self.0.push(Event::RoundEnd {
            ticks: state.ticks(),
            monsters: MonsterSlot::ALL
                .into_iter()
                .any(|slot| state.sprites().is_enabled(slot.sprite())),
        });
    }

    fn game_end(&mut self, state: StateView<'_>) {
        self.0.push(Event::GameEnd {
            ticks: state.ticks(),
        });
    }
}

/// Runs frames with no input until the game finishes.
fn run_to_finish<O: Observer>(run: &mut Run<O>) {
    for _ in 0..MAX_FRAMES {
        if run.summary().is_some() {
            return;
        }
        run.step(&FrameInput::NONE);
    }
    panic!("the game did not finish in {MAX_FRAMES} frames");
}

#[test]
fn observer_event_order_one_round() {
    let mut run = Scenario::new(GameMode::OnePlayer)
        .seed(3)
        .build_observed(Log::default())
        .script(Script::parse("R*10").expect("valid script"));
    run.start();
    run.set_lives(FIRST, 1).expect("the first digger plays");
    assert_eq!(run.run_ticks(10), 10);
    // Wait for a monster, so that the round end's clean-up has something
    // to remove.
    let out = run.run_until(|state| state.monsters().on_screen() > 0, 1000);
    assert!(out, "a monster comes out");
    assert!(run.state().digger(FIRST).alive(), "still alive");
    let walked = run.ticks();
    run.kill_digger(FIRST, KillCause::Monster).expect("playing");
    run_to_finish(&mut run);
    let ticks = run.ticks();
    assert!(ticks > walked + 20, "a death and a wind-down follow");

    let lv1 = LevelNumber::new(1).expect("level 1");
    let mut expected = vec![Event::LevelPlan(P1, lv1), Event::Seed(0)];
    for tick in 1..=ticks {
        expected.push(Event::Controls(tick, S0));
        // The digger is sampled while it walks; dying and hidden, it is
        // not.
        if tick <= walked {
            expected.push(Event::Movement(S0));
        }
        // The slot no digger reads is offered once per tick as well.
        expected.push(Event::Controls(tick, S1));
        expected.push(Event::TickEnd(tick));
    }
    // The round end comes after its clean-up has erased the monsters,
    // the game end after the round end.
    expected.push(Event::RoundEnd {
        ticks,
        monsters: false,
    });
    expected.push(Event::GameEnd { ticks });

    let log = &run.observer().0;
    let first_diff = log.iter().zip(&expected).position(|(a, b)| a != b);
    assert_eq!(
        log.len(),
        expected.len(),
        "event counts; first difference at {first_diff:?}"
    );
    if let Some(at) = first_diff {
        let window = at.saturating_sub(4)..at + 4;
        panic!(
            "event order differs at {at}:\n  got      {:?}\n  expected {:?}",
            log.iter()
                .skip(window.start)
                .take(window.len())
                .collect::<Vec<_>>(),
            expected
                .iter()
                .skip(window.start)
                .take(window.len())
                .collect::<Vec<_>>()
        );
    }
}

// --- 1.4 Hook points playback relies on -------------------------------------

/// Counts the observer calls.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Counts {
    controls: [u32; 2],
    movements: [u32; 2],
    ticks: u32,
}

impl Observer for Counts {
    fn controls(&mut self, _tick: u32, slot: SlotId, raw: Controls) -> Controls {
        if let Some(n) = self.controls.get_mut(slot.index()) {
            *n += 1;
        }
        raw
    }

    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement {
        if let Some(n) = self.movements.get_mut(slot.index()) {
            *n += 1;
        }
        sample
    }

    fn tick_end(&mut self, _tick: u32, _state: StateView<'_>) {
        self.ticks += 1;
    }
}

/// The counts of `run` since `since`.
fn delta(run: &Run<Counts>, since: Counts) -> Counts {
    let now = *run.observer();
    Counts {
        controls: [
            now.controls[0] - since.controls[0],
            now.controls[1] - since.controls[1],
        ],
        movements: [
            now.movements[0] - since.movements[0],
            now.movements[1] - since.movements[1],
        ],
        ticks: now.ticks - since.ticks,
    }
}

fn right() -> FrameInput {
    FrameInput {
        slots: [
            Controls {
                right: true,
                ..Controls::NONE
            },
            Controls::NONE,
        ],
        ..FrameInput::NONE
    }
}

#[test]
fn movement_hook_only_when_alive_visible_unpaused() {
    let mut run = Scenario::new(GameMode::OnePlayer).build_observed(Counts::default());
    run.start();

    // Alive and visible: one sample per tick.
    let since = *run.observer();
    for _ in 0..5 {
        assert_eq!(run.step(&right()).frame, FrameKind::Tick);
    }
    let walked = delta(&run, since);
    assert_eq!(walked.movements, [5, 0]);
    assert_eq!(walked.ticks, 5);

    // Paused: the request's own frame is a tick, then no tick, no
    // controls and no sample until the resume.
    let pause = FrameInput {
        pause: true,
        ..right()
    };
    assert_eq!(run.step(&pause).frame, FrameKind::Tick);
    let since = *run.observer();
    for _ in 0..30 {
        assert_eq!(run.step(&right()).frame, FrameKind::Paused);
    }
    assert_eq!(run.step(&pause).frame, FrameKind::Paused, "the resume");
    assert_eq!(
        delta(&run, since),
        Counts::default(),
        "nothing while paused"
    );
    let since = *run.observer();
    assert_eq!(run.step(&right()).frame, FrameKind::Tick);
    assert_eq!(delta(&run, since).movements, [1, 0], "sampling resumes");

    // Dying: no sample, though the controls are still offered.
    run.kill_digger(FIRST, KillCause::Monster).expect("playing");
    let since = *run.observer();
    for _ in 0..10 {
        assert_eq!(run.step(&right()).frame, FrameKind::Tick);
    }
    let dying = delta(&run, since);
    assert_eq!(dying.movements, [0, 0]);
    assert_eq!(dying.controls, [10, 10]);

    // Hidden in the wind-down: no sample either.
    while !run.in_wind_down() {
        run.step(&right());
    }
    let since = *run.observer();
    run.step(&right());
    let wind_down = delta(&run, since);
    assert_eq!(wind_down.ticks, 1);
    assert_eq!(wind_down.movements, [0, 0]);
    assert_eq!(wind_down.controls, [1, 1]);
}

#[test]
fn movement_hook_not_called_in_push_pause() {
    // A bag in the top-right cell, the digger just left of it.
    let mut rows: [String; 10] = std::array::from_fn(|_| " ".repeat(15));
    rows[0].replace_range(14..15, "B");
    rows[9].replace_range(0..1, "C");
    let mut run = Scenario::new(GameMode::OnePlayer)
        .rows(rows.each_ref().map(String::as_str))
        .build_observed(Counts::default());
    run.start();
    run.place_digger(FIRST, Px::new(276, 18), Direction::Right)
        .expect("a reachable position");
    let since = *run.observer();
    run.step(&right());
    assert_eq!(run.state().digger(FIRST).bag_time(), 1, "push pause next");
    run.step(&right());
    let pushed = delta(&run, since);
    assert_eq!(pushed.ticks, 2);
    assert_eq!(pushed.movements, [1, 0], "no sample in the push pause");
    assert_eq!(pushed.controls, [2, 2], "controls offered on both ticks");
}

/// Replaces slot 1's controls with everything pressed.
#[derive(Debug, Default)]
struct Mash(u32);

impl Observer for Mash {
    fn controls(&mut self, _tick: u32, slot: SlotId, raw: Controls) -> Controls {
        if slot != S1 {
            return raw;
        }
        self.0 += 1;
        Controls {
            up: true,
            down: true,
            left: true,
            right: true,
            fire: true,
        }
    }
}

#[test]
fn controls_hook_called_for_unread_slot() {
    for (mode, seed) in modes() {
        let mut counts = Counts::default();
        let frames = play(mode, seed, Some(&mut counts));
        let ticks = counts.ticks;
        assert!(ticks > 500, "{mode:?}: a short game");
        // Every slot once per tick, read or not.
        assert_eq!(counts.controls, [ticks, ticks], "{mode:?}");
        if !mode.two_diggers() {
            assert_eq!(counts.movements[1], 0, "{mode:?}: slot 1 is never read");

            // What is returned for the unread slot has no effect.
            let mut mash = Mash::default();
            let mashed = play(mode, seed, Some(&mut mash));
            assert_eq!(mash.0, ticks, "{mode:?}");
            assert_eq!(frames, mashed, "{mode:?}: slot 1 changed the game");
        }
    }
}

/// Records the level-plan calls.
#[derive(Debug, Default)]
struct Plans(Vec<(PlayerId, LevelNumber)>);

impl Observer for Plans {
    fn level_plan(
        &mut self,
        player: PlayerId,
        level: LevelNumber,
        _layout: &LevelLayout,
    ) -> Option<LevelLayout> {
        self.0.push((player, level));
        None
    }
}

#[test]
fn level_plan_hook_only_for_participating_players() {
    let lv = |n| LevelNumber::new(n).expect("a level");
    // At game start: every participating player, in player order; never
    // the placeholder state of a second player who does not play (Q06).
    for (mode, _) in modes() {
        let mut run = Scenario::new(mode).build_observed(Plans::default());
        run.start();
        let expected = if mode.alternating() {
            vec![(P1, lv(1)), (P2, lv(1))]
        } else {
            vec![(P1, lv(1))]
        };
        assert_eq!(run.observer().0, expected, "{mode:?}");
    }

    // On a level advance: the advancing player only.
    let mut rows: [String; 10] = std::array::from_fn(|_| " ".repeat(15));
    rows[9].replace_range(7..9, "HC");
    let mut run = Scenario::new(GameMode::TwoPlayers)
        .rows(rows.each_ref().map(String::as_str))
        .build_observed(Plans::default())
        .script(Script::parse("R*20").expect("valid script"));
    run.start();
    let level_up = run.run_until(|state| state.level(P1) == Some(lv(2)), 1000);
    assert!(level_up, "player 1 eats the emerald and advances");
    // The level is set up as the round's outcome is decided.
    assert_eq!(run.observer().0, [(P1, lv(1)), (P2, lv(1)), (P1, lv(2))]);
}
