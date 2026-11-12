//! Scenario tests of the game-flow spec (design D11), one per spec
//! scenario, named after it.

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a test failure"
)]

use digger_core::geom::{Px, Rect};
use digger_core::testkit::{KillCause, MonsterSetup, Run, Scenario, Script};
use digger_core::video::{Framebuffer, TextColor};
use digger_core::{
    BagId, CellPos, DiggerId, Direction, FinishReason, FrameInput, FrameKind, GameMode,
    GauntletSeconds, LevelLayout, LevelNumber, MonsterSlot, Movement, Observer, PlayerId, ScoreId,
    SlotId, SoundCommand, StateView, StepOutcome, Tempo, Tune,
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
    // The round end erases the monsters' sprites; nothing else may change.
    let monsters: Vec<Rect> = MonsterSlot::ALL
        .into_iter()
        .filter(|slot| run.state().sprites().is_enabled(slot.sprite()))
        .map(|slot| run.state().sprites().rect(slot.sprite()))
        .collect();
    assert!(
        !monsters.is_empty(),
        "monsters are on screen after 300 ticks"
    );
    // Quirk P17: the digger's sprite is erased as well.
    let digger = run.state().sprites().rect(DiggerId::ALL[0].sprite());
    let erased: Vec<Rect> = monsters.iter().copied().chain([digger]).collect();
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
    // The music stops and then all sound, the digger is hidden (quirk
    // P17), the monsters are erased and no game-over notice is drawn.
    assert_eq!(
        run.take_sounds(),
        [SoundCommand::MusicOff, SoundCommand::StopAll]
    );
    assert!(!run.state().diggers_visible());
    assert!(!run.state().sprites().is_enabled(DiggerId::ALL[0].sprite()));
    for slot in MonsterSlot::ALL {
        assert!(!run.state().sprites().is_enabled(slot.sprite()));
    }
    assert_eq!(
        changed_outside(run.game().framebuffer(), &picture, &erased),
        0
    );
}

/// Whether the diggers were visible at each round end the observer saw.
#[derive(Debug, Default)]
struct VisibleAtRoundEnd(Vec<bool>);

impl Observer for VisibleAtRoundEnd {
    fn round_end(&mut self, state: StateView<'_>) {
        self.0.push(state.diggers_visible());
    }
}

/// Quirk P17 (exposed by `2riptest01.edrf` at tick 1024 and `2test03.edrf`
/// at tick 2391): a quit during the main phase, or during its pause,
/// skips the wind-down ticks but not the wind-down's start, so the round
/// end sees both diggers erased and hidden.
#[test]
fn quirk_p17_quit_hides_diggers_before_round_end() {
    for pause in [false, true] {
        let mut run = Scenario::new(GameMode::TwoDiggers)
            .seed(3)
            .build_observed(VisibleAtRoundEnd::default());
        assert_eq!(run.run_ticks(40), 40);
        assert!(run.state().diggers_visible());
        if pause {
            let paused = FrameInput {
                pause: true,
                ..FrameInput::NONE
            };
            run.step(&paused);
            assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Paused);
        }
        let ticks = run.ticks();
        let summary = run.step(&QUIT).finished.expect("the game finished");
        assert_eq!(summary.ticks, ticks, "no wind-down tick runs");
        assert_eq!(run.observer().0, [false], "pause {pause}");
        for digger in DiggerId::ALL {
            assert!(!run.state().sprites().is_enabled(digger.sprite()));
        }
    }
}

/// The number of pixels that differ between `a` and `b` outside the
/// logical rectangles `skip`.
fn changed_outside(a: &Framebuffer, b: &Framebuffer, skip: &[Rect]) -> usize {
    let width = Framebuffer::WIDTH;
    a.pixels()
        .iter()
        .zip(b.pixels().iter())
        .enumerate()
        .filter(|&(i, (pa, pb))| {
            // Two real pixels per logical pixel in each direction.
            let (x, y) = ((i % width) / 2, (i / width) / 2);
            let (x, y) = (i32::try_from(x).unwrap_or(0), i32::try_from(y).unwrap_or(0));
            let inside = skip.iter().any(|r| {
                (r.left()..=r.right()).contains(&x) && (r.top()..=r.bottom()).contains(&y)
            });
            !inside && pa != pb
        })
        .count()
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

// --- Shared helpers for the round-flow scenarios ----------------------------

const FIRST: DiggerId = DiggerId::ALL[0];
const SECOND: DiggerId = DiggerId::ALL[1];
const P1: PlayerId = PlayerId::ALL[0];
const P2: PlayerId = PlayerId::ALL[1];

const PAUSE: FrameInput = FrameInput {
    pause: true,
    ..FrameInput::NONE
};

/// The main music at standard tempo.
const MAIN: SoundCommand = SoundCommand::Music {
    tune: Tune::Main,
    tempo: Tempo::Standard,
};

/// The last-life music at standard tempo.
const LAST_LIFE: SoundCommand = SoundCommand::Music {
    tune: Tune::LastLife,
    tempo: Tempo::Standard,
};

/// Ten rows of solid earth with the characters of `marks`, each a column,
/// a row and a layout character.
fn rows_with(marks: &[(u8, u8, u8)]) -> [String; 10] {
    let mut rows = [[b' '; 15]; 10];
    for &(h, v, c) in marks {
        let at = rows
            .get_mut(usize::from(v))
            .and_then(|row| row.get_mut(usize::from(h)))
            .expect("mark inside the grid");
        *at = c;
    }
    rows.map(|row| String::from_utf8(row.to_vec()).expect("ASCII"))
}

/// A scenario in `mode` on the layout of `marks`.
fn scene(mode: GameMode, marks: &[(u8, u8, u8)]) -> Scenario {
    let rows = rows_with(marks);
    Scenario::new(mode).rows(rows.each_ref().map(String::as_str))
}

/// Builds `scenario` and steps to the round's first tick.
fn started(scenario: &Scenario) -> Run {
    let mut run = scenario.build();
    run.start();
    run
}

/// Runs one tick of `entry` and returns the tick's outcome and sounds.
fn tick<O: Observer>(run: &mut Run<O>, entry: &str) -> (StepOutcome, Vec<SoundCommand>) {
    run.set_script(Script::parse(entry).expect("valid script"));
    run.take_sounds();
    loop {
        let outcome = run.frame();
        if outcome.frame == FrameKind::Tick {
            return (outcome, run.take_sounds());
        }
        assert!(run.summary().is_none(), "the game finished");
    }
}

/// Runs `n` ticks without input.
fn idle<O: Observer>(run: &mut Run<O>, n: u32) {
    run.set_script(Script::default());
    assert_eq!(run.run_ticks(n), n);
}

/// Steps frames without input until the main phase ends (the diggers are
/// hidden), at most `max` frames; returns the ticks run.
fn run_to_main_end<O: Observer>(run: &mut Run<O>, max: u64) -> u32 {
    let start = run.ticks();
    run.set_script(Script::default());
    assert!(
        run.run_until(|s| !s.diggers_visible(), max),
        "the main phase ended within {max} frames"
    );
    run.ticks() - start
}

/// Steps wind-down ticks without input until the round ends; returns the
/// number of wind-down ticks.
fn wind_down<O: Observer>(run: &mut Run<O>) -> u32 {
    assert!(run.in_wind_down(), "a wind-down is running");
    let round = run.state().round();
    let mut ticks = 0;
    while run.state().round() == round {
        assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Tick);
        ticks += 1;
        assert!(ticks <= 1000, "the wind-down ends");
    }
    ticks
}

/// Steps waiting frames without input until the next tick frame would run
/// or the game finishes; returns the number of waiting frames.
fn waits_to_next_tick<O: Observer>(run: &mut Run<O>) -> u32 {
    let mut waits = 0;
    while !run.in_main_phase() && run.summary().is_none() {
        assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Wait);
        waits += 1;
        assert!(waits < 10_000, "a round starts");
    }
    waits
}

/// The layout of the level-completion scenarios: the start cell (7,9)
/// opened, and one emerald in cell (8,9), which the digger eats on its
/// third step to the right (at x = 164).
const ONE_EMERALD: [(u8, u8, u8); 2] = [(7, 9, b'H'), (8, 9, b'C')];

/// Walks the digger of a [`ONE_EMERALD`] game into the emerald; returns
/// the tick on which it was eaten.
fn eat_last_emerald<O: Observer>(run: &mut Run<O>) -> u32 {
    let player = run.state().current_player();
    for _ in 0..10 {
        tick(run, "R");
        if run.state().emeralds().count(player) == 0 {
            break;
        }
    }
    assert_eq!(
        run.state().emeralds().count(player),
        0,
        "the emerald was eaten"
    );
    run.ticks()
}

/// The logical rectangle of the top line's message area from `x`, `cells`
/// characters wide, in a fresh picture with `text` drawn there.
fn top_line_with(text: &str, x: i32, color: TextColor) -> Framebuffer {
    let mut fb = Framebuffer::new();
    fb.draw_text(text, Px::new(x, 0), color);
    fb
}

/// Whether the real pixels of the logical rectangle at (`x`, 0), `w` wide
/// and 12 high, are equal in `a` and `b`.
fn same_block(a: &Framebuffer, b: &Framebuffer, x: usize, w: usize) -> bool {
    let width = Framebuffer::WIDTH;
    (0..24).all(|row| {
        let from = row * width + x * 2;
        a.pixels().get(from..from + w * 2) == b.pixels().get(from..from + w * 2)
    })
}

// --- 14.1 Main-phase tick order and level completion -------------------------

#[test]
fn digger_moves_before_monsters() {
    let marks: Vec<_> = (0..15)
        .map(|h| (h, 9, b'H'))
        .chain([(0, 0, b'C')])
        .collect();
    let still_digger = |moving: bool| {
        let mut run = started(&scene(GameMode::OnePlayer, &marks));
        run.place_digger(FIRST, Px::new(92, 180), Direction::Right)
            .expect("reachable");
        run.place_monster_as(
            MonsterSlot::ALL[0],
            Px::new(112, 180),
            MonsterSetup {
                direction: Some(Direction::Left),
                ..MonsterSetup::default()
            },
        )
        .expect("playing");
        tick(&mut run, if moving { "R" } else { "." });
        (
            run.state().digger(FIRST).position(),
            run.state().digger(FIRST).alive(),
            run.state().monster(MonsterSlot::ALL[0]).position().x,
        )
    };
    // Standing at x = 92, the digger is not touched by the monster's move
    // to x = 108 or less.
    let (at, alive, monster_x) = still_digger(false);
    assert_eq!(at, Px::new(92, 180));
    assert!(alive, "the monster's move alone misses a digger at 92");
    assert!(monster_x <= 108);
    // Moving to x = 96 first, it is where the monster's move meets it.
    let (at, alive, _) = still_digger(true);
    assert_eq!(at, Px::new(96, 180));
    assert!(
        !alive,
        "the monster's move sees the digger at its new place"
    );
}

#[test]
fn bag_wobbles_same_tick_as_dig() {
    let mut run = started(&scene(
        GameMode::OnePlayer,
        &[(3, 4, b'B'), (2, 5, b'H'), (0, 0, b'C')],
    ));
    run.place_digger(FIRST, Px::new(52, 108), Direction::Right)
        .expect("reachable");
    let below = CellPos::new(3, 5).expect("in the grid");
    assert!(!run.state().field().cell(below).has_tunnel());
    tick(&mut run, "R");
    let state = run.state();
    assert!(state.field().cell(below).has_tunnel(), "first slice dug");
    assert!(
        state.bag(BagId::ALL[0]).wobbling(),
        "wobbling on the same tick"
    );
}

#[test]
fn last_emerald_ends_main_phase() {
    let mut run = started(&scene(GameMode::OnePlayer, &ONE_EMERALD));
    let n = eat_last_emerald(&mut run);
    let state = run.state();
    assert!(state.level_done(P1), "marked complete on tick {n}");
    assert!(!state.diggers_visible(), "tick {n} ended the main phase");
    assert!(run.in_wind_down());
}

#[test]
fn killing_last_monster_completes_level() {
    let mut run = Scenario::new(GameMode::OnePlayer).build();
    run.start();
    let slots = MonsterSlot::ALL;
    // Every monster is removed as it spawns, until all six have come.
    loop {
        tick(&mut run, ".");
        assert!(!run.state().level_done(P1));
        let occupied: Vec<_> = slots
            .into_iter()
            .filter(|&slot| run.state().monster(slot).occupied())
            .collect();
        if run.state().monsters().spawned() == 6 {
            assert_eq!(occupied.len(), 1, "the last monster is on screen");
            assert!(run.state().emeralds().count(P1) > 0);
            let last = *occupied.first().expect("one monster");
            run.remove_monster(last).expect("playing");
            break;
        }
        for slot in occupied {
            run.remove_monster(slot).expect("playing");
        }
        assert!(run.ticks() < 1000);
    }
    assert_eq!(run.state().monsters_left(), 0);
    tick(&mut run, ".");
    assert!(run.state().level_done(P1), "no monster left: complete");
    assert!(run.in_wind_down());
}

// --- 14.2 Wind-down -------------------------------------------------------

#[test]
fn quiet_field_winds_down_in_1_tick() {
    let mut run = started(&scene(GameMode::OnePlayer, &ONE_EMERALD));
    run.take_sounds();
    let n = eat_last_emerald(&mut run);
    assert_eq!(run.state().round(), 0);
    assert_eq!(wind_down(&mut run), 1);
    assert_eq!(run.ticks(), n + 1);
    assert_eq!(run.state().round(), 1);
}

#[test]
fn busy_field_winds_down_in_20_ticks() {
    let mut run = started(&scene(GameMode::OnePlayer, &ONE_EMERALD));
    eat_last_emerald(&mut run);
    let round = run.state().round();
    let mut ticks = 0;
    while run.state().round() == round {
        // Six hobbins digging down through the earth between cell corners,
        // put back before every tick, keep every wind-down tick's penalty
        // counter at 8 or more; no bag is moving.
        for (n, slot) in MonsterSlot::ALL.into_iter().enumerate() {
            let x = 12 + 40 * i32::try_from(n).expect("small");
            run.place_monster_as(
                slot,
                Px::new(x, 57),
                MonsterSetup {
                    hobbin: true,
                    direction: Some(Direction::Down),
                    ..MonsterSetup::default()
                },
            )
            .expect("in the wind-down");
        }
        assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Tick);
        ticks += 1;
        assert!(run.state().penalty().value() >= 8, "busy tick {ticks}");
        assert_eq!(run.state().moving_bags(), 0);
        assert!(ticks <= 20, "at most 20 ticks");
    }
    assert_eq!(ticks, 20);
}

#[test]
fn wobbling_bag_extends_wind_down() {
    // A bag over an open shaft at (3,4)–(3,9).
    let mut marks: Vec<_> = (5..10).map(|v| (3, v, b'V')).collect();
    marks.extend(ONE_EMERALD);
    let mut run = started(&scene(GameMode::OnePlayer, &marks));
    eat_last_emerald(&mut run);
    run.place_bag(BagId::ALL[0], Px::new(72, 18 + 18 * 4))
        .expect("in the wind-down");
    let round = run.state().round();
    let mut moving_after = Vec::new();
    while run.state().round() == round {
        assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Tick);
        moving_after.push(run.state().moving_bags());
        assert!(moving_after.len() < 200);
    }
    // The wind-down ran while the bag wobbled, fell and broke, and ended
    // on the first tick at which no bag was moving.
    let (last, before) = moving_after.split_last().expect("at least one tick");
    assert_eq!(*last, 0);
    assert!(before.len() > 10, "the bag kept the wind-down going");
    assert!(before.iter().all(|&n| n > 0));
}

/// An observer counting movement samples.
#[derive(Debug, Default)]
struct Samples {
    count: u32,
}

impl Observer for Samples {
    fn movement(&mut self, _slot: SlotId, sample: Movement) -> Movement {
        self.count += 1;
        sample
    }
}

#[test]
fn quirk_q04_monsters_draw_random_numbers_in_wind_down() {
    let marks: Vec<_> = (0..15).map(|h| (h, 4, b'H')).chain(ONE_EMERALD).collect();
    let mut run = scene(GameMode::OnePlayer, &marks).build_observed(Samples::default());
    run.start();
    run.set_script(Script::parse("R*10").expect("valid script"));
    let player = run.state().current_player();
    assert!(run.run_until(|s| s.emeralds().count(player) == 0, 100));
    assert!(run.in_wind_down());
    // A living nobbin with no delay in the row 4 tunnel, between cells.
    run.place_monster_as(
        MonsterSlot::ALL[0],
        Px::new(116, 90),
        MonsterSetup::default(),
    )
    .expect("in the wind-down");
    let samples = run.observer().count;
    let draws = run.state().rng_draws();
    let at = run.state().monster(MonsterSlot::ALL[0]).position();
    assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Tick);
    assert!(run.state().rng_draws() > draws, "the monster drew");
    assert_ne!(run.state().monster(MonsterSlot::ALL[0]).position(), at);
    assert_eq!(run.observer().count, samples, "no movement sample");
}

// --- 14.3 Round end ---------------------------------------------------------

#[test]
fn flying_fireball_gone_at_round_end() {
    // A shaft up from the start cell to fire into, and the last emerald to
    // the right of the start cell.
    let mut marks: Vec<_> = (0..10).map(|v| (7, v, b'V')).collect();
    marks.push((8, 9, b'C'));
    let mut run = started(&scene(GameMode::OnePlayer, &marks));
    run.place_digger(FIRST, Px::new(152, 180), Direction::Up)
        .expect("reachable");
    tick(&mut run, "F");
    tick(&mut run, ".");
    assert!(!run.state().digger(FIRST).not_firing(), "a fireball flies");
    eat_last_emerald(&mut run);
    assert!(
        !run.state().digger(FIRST).not_firing(),
        "still flying when the main phase ends"
    );
    wind_down(&mut run);
    let state = run.state();
    assert!(state.digger(FIRST).not_firing(), "removed at the round end");
    assert!(!state.sprites().is_enabled(FIRST.fireball().sprite()));
    // The next round starts with the fireball ready.
    waits_to_next_tick(&mut run);
    let digger = run.state().digger(FIRST);
    assert!(digger.not_firing());
    assert!(digger.fire_ready());
}

/// What the observer's round-end hook saw.
#[derive(Debug, Default)]
struct RoundEnds {
    /// Per round end: the bags that exist, and the monster sprites shown.
    seen: Vec<(Vec<BagId>, usize)>,
}

impl Observer for RoundEnds {
    fn round_end(&mut self, state: StateView<'_>) {
        let bags = BagId::ALL
            .into_iter()
            .filter(|&id| state.bag(id).exists())
            .collect();
        let monsters = MonsterSlot::ALL
            .into_iter()
            .filter(|slot| state.sprites().is_enabled(slot.sprite()))
            .count();
        self.seen.push((bags, monsters));
    }
}

#[test]
fn observer_round_end_sees_tidied_bags_and_no_monsters() {
    let marks: Vec<_> = ONE_EMERALD
        .into_iter()
        .chain([(2, 2, b'B'), (12, 2, b'B')])
        .collect();
    let mut run = scene(GameMode::OnePlayer, &marks).build_observed(RoundEnds::default());
    run.start();
    // Gold lying in the field is tidied away; the resting bags stay.
    run.place_gold(BagId::ALL[2], Px::new(212, 126))
        .expect("playing");
    run.place_monster_as(
        MonsterSlot::ALL[3],
        Px::new(12, 18),
        MonsterSetup {
            direction: None,
            standstill: u8::MAX,
            ..MonsterSetup::default()
        },
    )
    .expect("playing");
    assert!(
        run.state()
            .sprites()
            .is_enabled(MonsterSlot::ALL[3].sprite())
    );
    eat_last_emerald(&mut run);
    wind_down(&mut run);
    assert_eq!(
        run.observer().seen,
        [(vec![BagId::ALL[0], BagId::ALL[1]], 0)]
    );
}

// --- 14.4 Level completion --------------------------------------------------

#[test]
fn completing_level_2_draws_level_3_without_life_loss() {
    let mut run = started(&scene(GameMode::OnePlayer, &ONE_EMERALD).start_level(2));
    assert_eq!(run.state().level(P1), LevelNumber::new(2));
    eat_last_emerald(&mut run);
    wind_down(&mut run);
    assert_eq!(run.state().round(), 1);
    // The pause, then a full draw of the new level: waiting frames, then
    // the first tick of round 1 at the start cell with the emerald back.
    assert!(waits_to_next_tick(&mut run) > 40);
    assert_eq!(run.state().level(P1), LevelNumber::new(3));
    assert_eq!(run.state().lives(FIRST), 3);
    assert_eq!(run.state().emeralds().count(P1), 1);
    assert_eq!(run.state().digger(FIRST).position(), Px::new(152, 180));
}

#[test]
fn quirk_q14_last_emerald_on_death_tick_advances_and_costs_a_life() {
    // The digger, put at x = 156, eats the emerald of cell (8,9) on its
    // second step, to x = 164 (remainder 12 moving right); on the same
    // tick a nobbin coming down the shaft in column 7 reaches it.
    let mut marks: Vec<_> = (0..9).map(|v| (7, v, b'V')).collect();
    marks.extend([(6, 9, b'H'), (7, 9, b'H'), (8, 9, b'C')]);
    let scenario = scene(GameMode::OnePlayer, &marks);
    let found = (150..=171).find_map(|y| {
        let mut run = started(&scenario);
        run.place_digger(FIRST, Px::new(156, 180), Direction::Right)
            .expect("reachable");
        run.place_monster_as(
            MonsterSlot::ALL[0],
            Px::new(152, y),
            MonsterSetup {
                direction: Some(Direction::Down),
                ..MonsterSetup::default()
            },
        )
        .ok()?;
        tick(&mut run, "R");
        let before = (
            run.state().digger(FIRST).alive(),
            run.state().emeralds().count(P1),
        );
        if before != (true, 1) {
            return None;
        }
        tick(&mut run, "R");
        let state = run.state();
        (!state.digger(FIRST).alive() && state.emeralds().count(P1) == 0).then_some(run)
    });
    let mut run = found.expect("a start height that kills on the emerald's tick");
    assert!(!run.state().level_done(P1), "not complete: nobody alive");
    run_to_main_end(&mut run, 1000);
    let sounds_before = run.take_sounds();
    assert!(!sounds_before.contains(&SoundCommand::LevelDone));
    wind_down(&mut run);
    assert!(!run.take_sounds().contains(&SoundCommand::LevelDone));
    assert_eq!(run.state().lives(FIRST), 2);
    assert_eq!(run.state().level(P1), LevelNumber::new(2));
    // No 40-frame pause: the draw of level 2 starts on the next frame.
    let before = run.game().framebuffer().clone();
    assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Wait);
    assert_ne!(run.game().framebuffer(), &before, "the draw began");
}

#[test]
fn level_done_pause_is_40_wait_frames() {
    let mut run = started(&scene(GameMode::OnePlayer, &ONE_EMERALD));
    eat_last_emerald(&mut run);
    run.take_sounds();
    wind_down(&mut run);
    let sounds = run.take_sounds();
    assert_eq!(sounds.last(), Some(&SoundCommand::LevelDone));
    let ticks = run.ticks();
    let picture = run.game().framebuffer().clone();
    for frame in 1..=40 {
        assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Wait);
        assert_eq!(run.ticks(), ticks);
        assert_eq!(run.game().framebuffer(), &picture, "pause frame {frame}");
    }
    // Frame 41 is the first frame of the next level's draw.
    assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Wait);
    assert_ne!(run.game().framebuffer(), &picture);
    assert_eq!(run.ticks(), ticks);
}

/// An observer that replaces level 2's layout and records every call.
#[derive(Debug, Default)]
struct PlanSwap {
    calls: Vec<(PlayerId, Option<LevelNumber>)>,
}

impl Observer for PlanSwap {
    fn level_plan(
        &mut self,
        player: PlayerId,
        level: LevelNumber,
        _layout: &LevelLayout,
    ) -> Option<LevelLayout> {
        self.calls.push((player, Some(level)));
        (level == LevelNumber::new(2)?).then(|| {
            let rows = rows_with(&[(7, 9, b'H'), (1, 1, b'C'), (2, 2, b'C'), (3, 3, b'C')]);
            LevelLayout::parse_rows(rows.each_ref().map(String::as_str)).expect("valid rows")
        })
    }
}

#[test]
fn observer_can_replace_level_plan() {
    let mut run = scene(GameMode::OnePlayer, &ONE_EMERALD).build_observed(PlanSwap::default());
    run.start();
    eat_last_emerald(&mut run);
    wind_down(&mut run);
    waits_to_next_tick(&mut run);
    assert_eq!(
        run.observer().calls,
        [(P1, LevelNumber::new(1)), (P1, LevelNumber::new(2))]
    );
    let state = run.state();
    assert_eq!(
        state.emeralds().count(P1),
        3,
        "the supplied layout's emeralds"
    );
    for n in 1..=3 {
        let cell = CellPos::new(n, n).expect("in the grid");
        assert!(state.emeralds().has(P1, cell));
    }
}

// --- 14.5 Death outcome and passing the turn ----------------------------------

/// The waiting frames of the full draw of an untouched built-in level 1:
/// 33 opened cells and 30 emeralds, three items per frame.
const LEVEL_1_DRAW_FRAMES: u32 = 21;

/// Kills the current player's first digger and runs its death and the
/// wind-down; returns the outcome of the round's last tick.
fn die_and_end_round<O: Observer>(run: &mut Run<O>) -> StepOutcome {
    run.kill_digger(FIRST, KillCause::Monster).expect("playing");
    run_to_main_end(run, 1000);
    wind_down(run);
    run.last_outcome().expect("a frame ran")
}

#[test]
fn turn_passes_with_45_frame_announcement() {
    let mut run = Scenario::new(GameMode::TwoPlayers).build();
    // The first round of an alternating game is announced too.
    assert_eq!(run.start(), LEVEL_1_DRAW_FRAMES + 45);
    let outcome = die_and_end_round(&mut run);
    assert_eq!(outcome.player_out, None);
    assert_eq!(run.state().current_player(), P2);
    assert_eq!(run.state().lives(FIRST), 2);
    let ticks = run.ticks();
    for _ in 0..LEVEL_1_DRAW_FRAMES {
        assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Wait);
    }
    let colors = [TextColor::One, TextColor::Two, TextColor::Three];
    for frame in 0..44 {
        assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Wait);
        let color = *colors.get(frame % 3).expect("three colours");
        let expected = top_line_with("PLAYER 2", 108, color);
        assert!(
            same_block(run.game().framebuffer(), &expected, 108, 96),
            "PLAYER 2 in colour {color:?} on announcement frame {frame}"
        );
    }
    // The 45th frame ends the announcement and starts the round.
    assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Wait);
    assert!(run.in_main_phase());
    assert_eq!(run.ticks(), ticks, "no tick during the announcement");
    assert!(same_block(
        run.game().framebuffer(),
        &Framebuffer::new(),
        108,
        96
    ));
    assert_eq!(tick(&mut run, ".").0.frame, FrameKind::Tick);
}

#[test]
fn no_announcement_in_one_player_game() {
    let mut run = Scenario::new(GameMode::OnePlayer).build();
    assert_eq!(run.start(), LEVEL_1_DRAW_FRAMES);
    die_and_end_round(&mut run);
    // The next round starts on the same field at once.
    assert!(run.in_main_phase());
    assert_eq!(run.state().lives(FIRST), 2);
}

#[test]
fn same_player_continues_when_other_is_out() {
    let mut run = Scenario::new(GameMode::TwoPlayers).build();
    run.start();
    run.set_lives(SECOND, 0).expect("player 2's record");
    die_and_end_round(&mut run);
    assert_eq!(run.state().current_player(), P1);
    assert_eq!(run.state().lives(FIRST), 2);
    assert!(run.in_main_phase(), "no redraw and no announcement");
    assert_eq!(waits_to_next_tick(&mut run), 0);
}

#[test]
fn returning_player_gets_saved_field_normalised() {
    let mut run = started(&scene(
        GameMode::TwoPlayers,
        &[(7, 8, b'H'), (7, 9, b'H'), (0, 0, b'C')],
    ));
    // Player 1 takes one step down from cell (7,8), digging a vertical
    // slice of the opened cell (7,9), and dies.
    run.place_digger(FIRST, Px::new(152, 162), Direction::Down)
        .expect("reachable");
    tick(&mut run, "D");
    let cell = CellPos::new(7, 9).expect("in the grid");
    die_and_end_round(&mut run);
    let saved = run.state().saved_field(P1).cell(cell);
    assert!(!saved.is_untouched());
    assert!(
        !saved.has_all_v_slices() && !saved.has_no_v_slices(),
        "partly dug"
    );
    // Player 2 plays and dies; player 1 returns to the saved field.
    waits_to_next_tick(&mut run);
    assert_eq!(run.state().current_player(), P2);
    die_and_end_round(&mut run);
    waits_to_next_tick(&mut run);
    assert_eq!(run.state().current_player(), P1);
    assert!(
        run.state().field().cell(cell).has_no_v_slices(),
        "the redraw opened the partly dug cell"
    );
}

// --- 14.6 Game over -----------------------------------------------------------

#[test]
fn one_player_game_over_after_last_life() {
    let mut run = Scenario::new(GameMode::OnePlayer).build();
    run.start();
    run.set_lives(FIRST, 1).expect("in play");
    let outcome = die_and_end_round(&mut run);
    let result = outcome.player_out.expect("player 1 is out");
    assert_eq!(result.player, P1);
    assert_eq!(result.score, 0);
    assert_eq!(outcome.finished, None);
    let notice = top_line_with("GAME OVER", 104, TextColor::Three);
    let ticks = run.ticks();
    for frame in 1..50 {
        assert!(same_block(run.game().framebuffer(), &notice, 104, 108));
        let outcome = run.step(&FrameInput::NONE);
        assert_eq!(outcome.frame, FrameKind::Wait, "notice frame {frame}");
        assert_eq!(outcome.finished, None);
    }
    let outcome = run.step(&FrameInput::NONE);
    let summary = outcome.finished.expect("finished after 50 frames");
    assert_eq!(summary.reason, FinishReason::AllPlayersOut);
    assert_eq!(summary.ticks, ticks);
    assert!(same_block(
        run.game().framebuffer(),
        &Framebuffer::new(),
        104,
        108
    ));
}

#[test]
fn alternating_game_continues_after_player_1_is_out() {
    let mut run = Scenario::new(GameMode::TwoPlayers).build();
    run.start();
    run.set_lives(FIRST, 1).expect("in play");
    let outcome = die_and_end_round(&mut run);
    assert_eq!(outcome.player_out.map(|out| out.player), Some(P1));
    // 50 frames of the notice, then player 2's draw and announcement.
    assert_eq!(waits_to_next_tick(&mut run), 50 + LEVEL_1_DRAW_FRAMES + 45);
    assert_eq!(run.state().current_player(), P2);
    assert_eq!(run.summary(), None);
    assert_eq!(tick(&mut run, ".").0.frame, FrameKind::Tick);
}

#[test]
fn quirk_q39_zero_point_call_awards_pending_bonus() {
    let mut run = Scenario::new(GameMode::OnePlayer).build();
    run.start();
    run.set_lives(FIRST, 1).expect("in play");
    // A score at the bonus threshold that no scoring call has checked yet.
    run.set_score(ScoreId::ALL[0], 20_000).expect("in range");
    run.take_sounds();
    let outcome = die_and_end_round(&mut run);
    assert_eq!(outcome.player_out.map(|out| out.score), Some(20_000));
    assert!(
        run.sounds().contains(&SoundCommand::ExtraLife),
        "the game end's zero-point call awards the life"
    );
    assert_eq!(run.state().lives(FIRST), 1);
    // The notice is shown, and then the player plays on with that life.
    assert_eq!(waits_to_next_tick(&mut run), 50);
    assert_eq!(run.summary(), None);
    assert_eq!(tick(&mut run, ".").0.frame, FrameKind::Tick);
}

// --- 14.7 Gauntlet ------------------------------------------------------------

fn gauntlet(seconds: u32) -> GameMode {
    GameMode::Gauntlet {
        time: GauntletSeconds::new(seconds),
    }
}

#[test]
fn quirk_q13_120s_gauntlet_times_out_on_tick_1790_clock_4294949016() {
    let mut run = Scenario::new(gauntlet(120)).build();
    assert!(run.run_until(|s: StateView<'_>| s.timed_out(), 100_000));
    assert_eq!(run.ticks(), 1790);
    assert_eq!(run.state().gauntlet_clock(), Some(4_294_949_016));
    // The notice runs to the end of the game.
    assert!(!run.run_until(|_| false, 100));
    let summary = run.summary().expect("finished");
    assert_eq!(summary.reason, FinishReason::TimeUp);
    assert_eq!(summary.ticks, 1790);
}

#[test]
fn gauntlet_bonus_adds_17897715() {
    let mut run = started(&scene(
        gauntlet(120),
        &[(7, 9, b'H'), (8, 9, b'C'), (0, 0, b'C')],
    ));
    run.set_score(ScoreId::ALL[0], 19_975).expect("in range");
    let clock = |run: &Run| run.state().gauntlet_clock().expect("gauntlet");
    loop {
        let before = clock(&run);
        let (_, sounds) = tick(&mut run, "R");
        if run.state().scores().visible(ScoreId::ALL[0]) >= 20_000 {
            assert_eq!(clock(&run), before - 80_000 + 17_897_715);
            assert!(!sounds.contains(&SoundCommand::ExtraLife));
            break;
        }
        assert_eq!(clock(&run), before - 80_000);
        assert!(run.ticks() < 20, "the emerald is eaten");
    }
}

#[test]
fn gauntlet_clock_runs_while_dying() {
    let mut run = Scenario::new(gauntlet(120)).build();
    run.start();
    run.kill_digger(FIRST, KillCause::Monster).expect("playing");
    for _ in 0..30 {
        let before = run.state().gauntlet_clock().expect("gauntlet");
        tick(&mut run, ".");
        assert!(run.state().digger(FIRST).death_stage().is_some());
        assert_eq!(run.state().gauntlet_clock(), Some(before - 80_000));
    }
    assert_eq!(run.state().lives(FIRST), 3, "gauntlet lives never change");
}

#[test]
fn time_up_shown_for_50_frames_then_finished() {
    // One second: 1,193,181 units run out on tick 15.
    let mut run = Scenario::new(gauntlet(1)).build();
    run.start();
    run.take_sounds();
    idle(&mut run, 14);
    let (outcome, sounds) = tick(&mut run, ".");
    assert_eq!(run.ticks(), 15);
    assert!(run.state().timed_out());
    assert_eq!(outcome.player_out.map(|out| out.player), Some(P1));
    assert_eq!(outcome.finished, None);
    let tail = sounds.get(sounds.len() - 2..).expect("two cues");
    assert_eq!(tail, [SoundCommand::MusicOff, SoundCommand::StopAll]);
    let notice = top_line_with("TIME UP", 120, TextColor::Three);
    for frame in 1..50 {
        assert!(same_block(run.game().framebuffer(), &notice, 120, 84));
        let outcome = run.step(&FrameInput::NONE);
        assert_eq!(outcome.frame, FrameKind::Wait, "notice frame {frame}");
        assert_eq!(outcome.finished, None);
    }
    let summary = run.step(&FrameInput::NONE).finished.expect("finished");
    assert_eq!(summary.reason, FinishReason::TimeUp);
    assert_eq!(summary.ticks, 15);
    assert_eq!(run.ticks(), 15, "no tick after the time-out");
}

// --- 14.8 Pause ---------------------------------------------------------------

#[test]
fn paused_frames_change_no_state() {
    let mut run = Scenario::new(GameMode::OnePlayer).seed(4).build();
    run.start();
    idle(&mut run, 30);
    run.take_sounds();
    let outcome = run.step(&PAUSE);
    assert_eq!(outcome.frame, FrameKind::Tick, "the pause follows the tick");
    assert!(run.is_paused());
    assert_eq!(run.take_sounds().last(), Some(&SoundCommand::Pause));
    let line = trace(&run);
    let picture = run.game().framebuffer().clone();
    let (ticks, rng) = (run.ticks(), run.state().rng_state());
    for _ in 0..200 {
        assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Paused);
        assert_eq!(trace(&run), line);
        assert_eq!(run.game().framebuffer(), &picture);
    }
    assert_eq!(run.sounds(), []);
    assert_eq!(run.step(&PAUSE).frame, FrameKind::Paused, "resumed");
    assert_eq!(run.ticks(), ticks);
    assert_eq!(run.state().rng_state(), rng);
    assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Tick);
}

#[test]
fn pause_ignored_during_wind_down() {
    let mut run = started(&scene(GameMode::OnePlayer, &ONE_EMERALD));
    eat_last_emerald(&mut run);
    assert!(run.in_wind_down());
    run.take_sounds();
    assert_eq!(run.step(&PAUSE).frame, FrameKind::Tick);
    assert!(!run.is_paused());
    assert!(!run.take_sounds().contains(&SoundCommand::Pause));
}

#[test]
fn resume_refreshes_scores_and_lives() {
    let mut run = started(&scene(
        GameMode::OnePlayer,
        &[(7, 9, b'H'), (8, 9, b'C'), (0, 0, b'C')],
    ));
    // Score 25 on the emerald, then a pause.
    run.set_script(Script::parse("R*3").expect("valid script"));
    run.run_ticks(3);
    assert_eq!(run.state().scores().visible(ScoreId::ALL[0]), 25);
    let status = run.game().framebuffer().clone();
    run.step(&PAUSE);
    let message = top_line_with("PRESS ANY KEY", 80, TextColor::One);
    assert!(same_block(run.game().framebuffer(), &message, 80, 156));
    assert!(same_block(
        run.game().framebuffer(),
        &Framebuffer::new(),
        0,
        80
    ));
    run.take_sounds();
    assert_eq!(run.step(&PAUSE).frame, FrameKind::Paused);
    assert_eq!(run.take_sounds(), [SoundCommand::Resume]);
    // The top line is as before the pause: score and lives, no message.
    assert!(same_block(run.game().framebuffer(), &status, 0, 320));
    assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Tick);
}

// --- 14.9 Music and command order -----------------------------------------------

#[test]
fn last_life_round_requests_last_life_tune() {
    let mut run = Scenario::new(GameMode::OnePlayer).build();
    run.start();
    run.set_lives(FIRST, 2).expect("in play");
    run.take_sounds();
    die_and_end_round(&mut run);
    assert_eq!(run.state().lives(FIRST), 1);
    let sounds = run.take_sounds();
    assert_eq!(sounds.last(), Some(&LAST_LIFE), "round start's request");
    assert!(!sounds.contains(&MAIN));
}

#[test]
fn bonus_life_on_last_life_rerequests_main_tune() {
    let mut run = started(&scene(
        GameMode::OnePlayer,
        &[(7, 9, b'H'), (8, 9, b'C'), (0, 0, b'C')],
    ));
    run.set_lives(FIRST, 2).expect("in play");
    die_and_end_round(&mut run);
    assert_eq!(run.take_sounds().last(), Some(&LAST_LIFE));
    run.set_score(ScoreId::ALL[0], 19_975).expect("in range");
    let mut sounds = Vec::new();
    for _ in 0..3 {
        sounds.extend(tick(&mut run, "R").1);
    }
    assert_eq!(run.state().lives(FIRST), 2);
    let life = sounds
        .iter()
        .position(|&s| s == SoundCommand::ExtraLife)
        .expect("a bonus life");
    assert_eq!(sounds.get(life + 1), Some(&MAIN));
}

#[test]
fn commands_arrive_in_event_order() {
    // A bag over an open shaft in column 3, which wobbles, falls and
    // breaks; an emerald at (3,1) that a digger put at (52,36) eats on its
    // third step to the right.
    let mut marks: Vec<_> = (5..10).map(|v| (3, v, b'V')).collect();
    marks.extend([(3, 4, b'B'), (3, 1, b'C'), (0, 0, b'C')]);
    let scenario = scene(GameMode::OnePlayer, &marks);
    let set_up = || {
        let mut run = started(&scenario);
        run.place_digger(FIRST, Px::new(52, 36), Direction::Right)
            .expect("reachable");
        run
    };
    let mut probe = set_up();
    let mut breaks = None;
    for _ in 0..100 {
        if tick(&mut probe, ".").1.contains(&SoundCommand::Break) {
            breaks = Some(probe.ticks());
            break;
        }
    }
    let b = breaks.expect("the bag breaks");
    let mut run = set_up();
    idle(&mut run, b - 3);
    tick(&mut run, "R");
    tick(&mut run, "R");
    let (_, sounds) = tick(&mut run, "R");
    assert_eq!(run.ticks(), b);
    let cues: Vec<_> = sounds
        .into_iter()
        .filter(|s| {
            matches!(
                s,
                SoundCommand::EmeraldClick | SoundCommand::EmeraldNote(_) | SoundCommand::Break
            )
        })
        .collect();
    assert!(matches!(
        cues.as_slice(),
        [
            SoundCommand::EmeraldClick,
            SoundCommand::EmeraldNote(_),
            SoundCommand::Break
        ]
    ));
}

// --- 15.5 Remaining game-flow scenarios ----------------------------------------

/// `player`'s level number, 0 for the placeholder.
fn level_of(run: &Run, player: PlayerId) -> u16 {
    run.state().level(player).map_or(0, LevelNumber::get)
}

#[test]
fn alternating_players_keep_separate_progress() {
    let mut run = started(&scene(GameMode::TwoPlayers, &ONE_EMERALD));
    // Player 1 completes level 1 and plays on on level 2.
    eat_last_emerald(&mut run);
    run.start();
    assert_eq!(run.state().current_player(), P1);
    assert_eq!(level_of(&run, P1), 2);
    assert_eq!(run.state().scores().visible(ScoreId::ALL[0]), 25);

    // Player 1 dies: player 2's turn, on its own level 1 with its own
    // emerald, score and lives.
    die_and_end_round(&mut run);
    run.start();
    let state = run.state();
    assert_eq!(state.current_player(), P2);
    assert_eq!(level_of(&run, P2), 1);
    assert_eq!(state.emeralds().count(P2), 1);
    assert_eq!(state.scores().visible(ScoreId::ALL[1]), 0);
    assert_eq!(state.lives(SECOND), 3);

    // Player 2 dies: player 1 is back on level 2 with 25 points and the 2
    // lives left after the death.
    die_and_end_round(&mut run);
    run.start();
    let state = run.state();
    assert_eq!(state.current_player(), P1);
    assert_eq!(level_of(&run, P1), 2);
    assert_eq!(level_of(&run, P2), 1);
    assert_eq!(state.scores().visible(ScoreId::ALL[0]), 25);
    assert_eq!(state.lives(FIRST), 2);
    assert_eq!(state.lives(SECOND), 2);
}

#[test]
fn simultaneous_diggers_share_the_field() {
    let mut run = started(&scene(GameMode::TwoDiggers, &[(0, 0, b'C')]));
    let between = CellPos::new(7, 9).expect("in the grid");
    assert!(run.state().field().cell(between).is_untouched());
    // Digger 1 digs left from (172,180) through cell (7,9).
    for _ in 0..5 {
        tick(&mut run, "L/.");
    }
    assert_eq!(run.state().digger(FIRST).position(), Px::new(152, 180));
    let dug = *run.state().field();
    assert!(dug.cell(between).has_no_h_slices());
    // Digger 2 walks right from (132,180) into the same tunnel, which it
    // finds already dug: the one field does not change.
    for _ in 0..5 {
        tick(&mut run, "./R");
    }
    assert_eq!(run.state().digger(SECOND).position(), Px::new(152, 180));
    assert_eq!(run.state().field(), &dug);
}

#[test]
fn one_player_start_at_level_5() {
    let config = digger_core::GameConfig {
        start_level: digger_core::StartLevel::new(5).expect("in range"),
        ..digger_core::GameConfig::default()
    };
    let layout_5 = *config
        .levels
        .layout(LevelNumber::new(5).expect("a level").plan());
    let emeralds = layout_5
        .rows()
        .iter()
        .flatten()
        .filter(|&&c| c == b'C')
        .count();
    let mut run = Run::new(digger_core::Game::new(config, 0));
    run.start();
    let state = run.state();
    assert_eq!(level_of(&run, P1), 5);
    assert_eq!(usize::from(state.emeralds().count(P1)), emeralds);
    assert_eq!(state.scores().visible(ScoreId::ALL[0]), 0);
    assert_eq!(state.lives(FIRST), 3);
    // The field is layout 5's: its bag cells hold bags.
    let bag_cells = layout_5
        .rows()
        .iter()
        .flatten()
        .filter(|&&c| c == b'B')
        .count();
    let bags = BagId::ALL
        .iter()
        .filter(|&&id| state.bag(id).exists())
        .count();
    assert_eq!(bags, bag_cells.min(7));
}

#[test]
fn one_player_game_after_a_two_player_game() {
    // A fresh one-player game, as in a freshly started program.
    let fresh = |run: &mut Run| {
        run.start();
        let state = run.state();
        (
            trace(run),
            *state.saved_field(P2),
            state.level(P2),
            *state.emeralds(),
        )
    };
    let mut first = Scenario::new(GameMode::OnePlayer).build();
    let expected = fresh(&mut first);
    assert_eq!(expected.2, None, "level 0 placeholder (quirk Q06)");

    // A two-player game is played for a while first.
    let mut two = Scenario::new(GameMode::TwoPlayers).build();
    two.start();
    two.set_script(Script::parse("R*20 U*20 L*20").expect("valid script"));
    two.run_ticks(60);
    die_and_end_round(&mut two);
    two.start();
    assert!(two.state().level(P2).is_some());

    let mut after = Scenario::new(GameMode::OnePlayer).build();
    assert_eq!(fresh(&mut after), expected);
}

#[test]
fn death_then_completion_numbers_rounds_0_1_2() {
    let mut run = started(&scene(GameMode::OnePlayer, &ONE_EMERALD));
    assert_eq!(run.state().round(), 0);
    die_and_end_round(&mut run);
    run.start();
    assert_eq!(run.state().round(), 1, "the death ended round 0");
    assert_eq!(level_of(&run, P1), 1);
    eat_last_emerald(&mut run);
    run.start();
    assert_eq!(run.state().round(), 2, "the completion ended round 1");
    assert_eq!(level_of(&run, P1), 2);
}

#[test]
fn round_after_a_death_keeps_field_emeralds_and_bags() {
    // A bag, two emeralds, and a tunnel dug part of the way into a cell.
    let marks = [(5, 9, b'C'), (0, 0, b'C'), (10, 0, b'B')];
    let mut run = started(&scene(GameMode::OnePlayer, &marks));
    for _ in 0..2 {
        tick(&mut run, "U");
    }
    let partly = CellPos::new(7, 8).expect("in the grid");
    let cell = run.state().field().cell(partly);
    assert!(cell.has_tunnel() && cell.is_untouched(), "partly dug");
    let field = *run.state().field();
    let emeralds = *run.state().emeralds();
    let bag = run.state().bag(BagId::ALL[0]).position();

    die_and_end_round(&mut run);
    run.start();
    let state = run.state();
    assert_eq!(state.round(), 1);
    // Same tunnels (partly dug cells keep exactly their slices: no
    // redraw), same emeralds, the surviving bag, the digger back at its
    // start cell.
    assert_eq!(state.field(), &field);
    assert_eq!(state.emeralds(), &emeralds);
    assert_eq!(state.bag(BagId::ALL[0]).position(), bag);
    assert_eq!(state.digger(FIRST).position(), Px::new(152, 180));
    assert_eq!(state.lives(FIRST), 2);
}

#[test]
fn tick_count_across_a_level_change() {
    let mut run = started(&scene(GameMode::OnePlayer, &ONE_EMERALD));
    eat_last_emerald(&mut run);
    let main_end = run.ticks();
    let wind_down_ticks = wind_down(&mut run);
    assert_eq!(run.ticks(), main_end + wind_down_ticks);
    // Waiting frames: the level-complete pause and the next level's draw.
    let waits = waits_to_next_tick(&mut run);
    assert!(waits >= 40);
    assert_eq!(run.ticks(), main_end + wind_down_ticks);
    assert_eq!(run.step(&FrameInput::NONE).frame, FrameKind::Tick);
    assert_eq!(run.ticks(), main_end + wind_down_ticks + 1);
    assert_eq!(level_of(&run, P1), 2);
}
