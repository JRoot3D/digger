//! Scenario tests of the monsters spec (design D11), one per spec scenario,
//! named after it, plus the quirks the monsters reproduce.
//!
//! Where a scenario needs particular random draws (a double move, or
//! none), its seed is found by [`Scenario::search_seed`] from a condition
//! on the round's generator, never hard-coded.

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a test failure"
)]

use digger_core::geom::Px;
use digger_core::testkit::{KillCause, MonsterSetup, Run, Scenario, Script};
use digger_core::{
    BagId, CellPos, DeathStage, DiggerId, Direction, GameMode, GameRng, Modulus, MonsterDeath,
    MonsterSlot, PenaltySite, ScoreId, SoundCommand,
};

const FIRST: DiggerId = DiggerId::ALL[0];
const BAG0: BagId = BagId::ALL[0];
const SEARCH: u64 = 10_000;

fn slot(n: u8) -> MonsterSlot {
    MonsterSlot::new(n).expect("a monster slot")
}

/// Ten rows of solid earth with the characters of `marks`, each a column,
/// a row and a layout character. Unless the marks place an emerald, one is
/// added in the first of the bottom-right, top-right and top-left cells
/// left as earth, so that the level is not complete before anything
/// happens (game-flow spec, "Level completion test").
fn rows_with(marks: &[(u8, u8, u8)]) -> [String; 10] {
    let mut rows = [[b' '; 15]; 10];
    for &(h, v, c) in marks {
        let at = rows
            .get_mut(usize::from(v))
            .and_then(|row| row.get_mut(usize::from(h)))
            .expect("mark inside the grid");
        *at = c;
    }
    if !rows.iter().flatten().any(|&c| c == b'C') {
        for (v, h) in [(9, 14), (0, 14), (0, 0)] {
            if let Some(c) = rows.get_mut(v).and_then(|row| row.get_mut(h))
                && *c == b' '
            {
                *c = b'C';
                break;
            }
        }
    }
    rows.map(|row| String::from_utf8(row.to_vec()).expect("ASCII"))
}

/// Marks for a horizontal tunnel along row `v`.
fn h_row(v: u8) -> Vec<(u8, u8, u8)> {
    (0..15).map(|h| (h, v, b'H')).collect()
}

/// Marks for a vertical tunnel down column `h` from row `top` to `bottom`.
fn v_column(h: u8, top: u8, bottom: u8) -> Vec<(u8, u8, u8)> {
    (top..=bottom).map(|v| (h, v, b'V')).collect()
}

/// A one-player scenario at `level` on the layout of `marks`.
fn scenario(level: i64, marks: &[(u8, u8, u8)]) -> Scenario {
    let rows = rows_with(marks);
    Scenario::new(GameMode::OnePlayer)
        .rows(rows.each_ref().map(String::as_str))
        .start_level(level)
}

/// Builds `scenario` with `seed` and steps to the round's first tick.
fn start(scenario: &Scenario, seed: u64) -> Run {
    let mut run = scenario.clone().seed(seed).build();
    run.start();
    run
}

/// The first draws of the round just started, with `moduli`.
fn first_draws(run: &Run, moduli: &[u8]) -> Vec<i16> {
    let mut rng = GameRng::new(run.state().rng_state());
    moduli
        .iter()
        .map(|&m| rng.draw(Modulus::new(m).expect("non-zero")))
        .collect()
}

/// The first seed for which the round's first draws, with `moduli`,
/// satisfy `wanted`.
fn seed_where(scenario: &Scenario, moduli: &[u8], wanted: impl Fn(&[i16]) -> bool) -> u64 {
    scenario
        .search_seed(SEARCH, |mut run| {
            run.start();
            wanted(&first_draws(&run, moduli))
        })
        .expect("a seed exists")
}

/// Puts a monster into slot `n` at (`x`, `y`).
fn place(run: &mut Run, n: u8, x: i32, y: i32, setup: MonsterSetup) {
    run.place_monster_as(slot(n), Px::new(x, y), setup)
        .expect("inside the playfield");
}

/// A nobbin heading `direction`.
fn heading(direction: Direction) -> MonsterSetup {
    MonsterSetup {
        direction: Some(direction),
        ..MonsterSetup::default()
    }
}

/// A nobbin that stays where it is: no direction, held in its standstill.
fn still() -> MonsterSetup {
    MonsterSetup {
        direction: None,
        standstill: u8::MAX,
        ..MonsterSetup::default()
    }
}

/// Runs `n` ticks without input.
fn idle(run: &mut Run, n: u32) {
    run.set_script(Script::default());
    assert_eq!(run.run_ticks(n), n);
}

/// Runs one tick of `entry`.
fn tick(run: &mut Run, entry: &str) {
    run.set_script(Script::parse(entry).expect("valid script"));
    assert_eq!(run.run_ticks(1), 1);
}

/// Steps `run` one tick at a time for `n` ticks and returns the number of
/// random draws each took.
fn draws_per_tick(run: &mut Run, n: u32) -> Vec<u64> {
    (0..n)
        .map(|_| {
            let before = run.state().rng_draws();
            idle(run, 1);
            run.state().rng_draws() - before
        })
        .collect()
}

fn occupied(run: &Run) -> usize {
    MonsterSlot::ALL
        .into_iter()
        .filter(|&s| run.state().monster(s).occupied())
        .count()
}

// --- 12.1 Slots, parameters and the spawn schedule -----------------------

#[test]
fn level_1_monsters_spawn_on_ticks_11_and_55() {
    let mut run = Scenario::new(GameMode::OnePlayer).seed(3).build();
    run.start();
    let mut spawned_on = Vec::new();
    for _ in 0..60 {
        let before = run.state().monsters().spawned();
        idle(&mut run, 1);
        if run.state().monsters().spawned() > before {
            spawned_on.push(run.ticks());
        }
    }
    assert!(run.state().digger(FIRST).alive());
    assert_eq!(spawned_on, [11, 55]);
    let monster = run.state().monster(slot(0));
    assert!(monster.spawned() && monster.nobbin());
    assert_eq!(monster.chase(), FIRST);
    assert_eq!(run.state().monsters().spawn_gap(), 43);
    assert_eq!(run.state().monsters().left(), 6);
}

#[test]
fn no_spawn_at_on_screen_limit() {
    let mut run = start(&scenario(1, &[]), 0);
    for n in 0..3 {
        place(&mut run, n, 100 + 40 * i32::from(n), 90, still());
    }
    run.set_spawn_timer(0).expect("playing");
    idle(&mut run, 5);
    assert_eq!(occupied(&run), 3);
    assert_eq!(run.state().monsters().spawned(), 0);
    assert_eq!(run.state().monsters().spawn_timer(), 0);

    run.remove_monster(slot(1)).expect("playing");
    idle(&mut run, 1);
    assert_eq!(occupied(&run), 3, "one spawns on the next tick");
    let monster = run.state().monster(slot(1));
    assert_eq!(monster.position(), Px::new(292, 18), "in the freed slot");
    assert_eq!(run.state().monsters().spawned(), 1);
    assert_eq!(run.state().monsters().spawn_timer(), 43);
}

#[test]
fn no_spawning_in_bonus_mode() {
    let mut run = start(&scenario(1, &[]), 0);
    run.set_spawn_timer(0).expect("playing");
    run.set_bonus_mode(true).expect("playing");
    idle(&mut run, 5);
    assert_eq!(occupied(&run), 0);
    run.set_bonus_mode(false).expect("playing");
    idle(&mut run, 1);
    assert_eq!(occupied(&run), 1);
}

#[test]
fn new_monster_stands_still_for_five_moves() {
    // Row 0 is open, so the new nobbin keeps heading left; a seed without
    // a double move in its first six moves.
    let scene = scenario(1, &h_row(0));
    let seed = scene
        .search_seed(SEARCH, |mut run| {
            run.start();
            idle(&mut run, 10);
            draws_per_tick(&mut run, 6) == [2, 2, 2, 2, 2, 2]
        })
        .expect("a seed exists");
    let mut run = start(&scene, seed);
    idle(&mut run, 10);
    let mut observed = Vec::new();
    for _ in 0..6 {
        idle(&mut run, 1);
        let monster = run.state().monster(slot(0));
        observed.push((monster.position().x, monster.standstill()));
    }
    assert_eq!(
        observed,
        [(292, 4), (292, 3), (292, 2), (292, 1), (292, 0), (288, 0)]
    );
}

#[test]
fn schedule_restarts_after_a_death() {
    let mut run = start(&scenario(1, &[]), 0);
    idle(&mut run, 60);
    assert_eq!(run.state().monsters().spawned(), 2);
    run.kill_digger(FIRST, KillCause::Monster).expect("playing");
    let round = run.state().round();
    assert!(run.run_until(|s| s.round() > round, 1000));
    run.start();
    let monsters = run.state().monsters();
    assert_eq!(occupied(&run), 0);
    assert_eq!(monsters.spawned(), 0);
    assert_eq!(monsters.total(), 6);
    assert_eq!(monsters.spawn_timer(), 10);
    assert_eq!(monsters.left(), 6);
}

// --- 12.2 The slot scheduler -------------------------------------------

#[test]
fn delayed_monster_waits_two_ticks() {
    let scene = scenario(1, &h_row(4));
    // Between corners only the double-move draw is taken.
    let seed = seed_where(&scene, &[14], |d| d != [0]);
    let mut run = start(&scene, seed);
    let delayed = MonsterSetup {
        delay: 2,
        ..MonsterSetup::default()
    };
    place(&mut run, 0, 116, 90, delayed);
    let mut observed = Vec::new();
    for _ in 0..3 {
        let before = run.state().rng_draws();
        idle(&mut run, 1);
        let monster = run.state().monster(slot(0));
        observed.push((
            monster.position().x,
            monster.delay(),
            run.state().rng_draws() - before,
        ));
    }
    assert_eq!(observed, [(116, 1, 0), (116, 0, 0), (112, 0, 1)]);
}

#[test]
fn nobbin_double_moves_when_draw_is_zero() {
    let scene = scenario(1, &h_row(4));
    let seed = seed_where(&scene, &[14], |d| d == [0]);
    let mut run = start(&scene, seed);
    // From remainder 8 to 4 and on to the corner at x = 112: neither move
    // starts on a corner, so neither decides.
    place(&mut run, 0, 120, 90, MonsterSetup::default());
    idle(&mut run, 1);
    let monster = run.state().monster(slot(0));
    assert_eq!(monster.position(), Px::new(112, 90), "two moves of 4");
    assert_eq!(run.state().rng_draws(), 1, "no decision: one draw");
    assert_eq!(
        run.state().penalty().count_of(PenaltySite::MonsterDraw),
        2,
        "drawn once per move"
    );
}

#[test]
fn quirk_q03_hobbin_draws_but_never_double_moves() {
    let scene = scenario(1, &h_row(4));
    let seed = seed_where(&scene, &[14], |d| d == [0]);
    let mut run = start(&scene, seed);
    let hobbin = MonsterSetup {
        hobbin: true,
        ..MonsterSetup::default()
    };
    place(&mut run, 0, 120, 90, hobbin);
    idle(&mut run, 1);
    let monster = run.state().monster(slot(0));
    assert_eq!(monster.position(), Px::new(116, 90), "one move only");
    assert_eq!(run.state().rng_draws(), 1, "the draw is still taken");
    assert_eq!(run.state().penalty().count_of(PenaltySite::MonsterDraw), 1);
}

#[test]
fn eaten_nobbin_makes_no_second_move() {
    // In bonus mode a nobbin whose first move runs into the digger is
    // eaten; a double-move draw of 0 then gives it no second move.
    let scene = scenario(1, &h_row(9));
    let seed = seed_where(&scene, &[14], |d| d == [0]);
    let mut run = start(&scene, seed);
    run.set_bonus_mode(true).expect("playing");
    place(&mut run, 0, 168, 180, heading(Direction::Left));
    idle(&mut run, 1);
    assert!(!run.state().monster(slot(0)).occupied());
    assert_eq!(run.state().rng_draws(), 1);
}

// --- 12.3 Decisions ------------------------------------------------------

#[test]
fn turning_costs_a_tick() {
    // A nobbin heading left on the crossing of row 4 and column 7, with
    // the digger straight below, turns down.
    let mut marks = h_row(4);
    marks.extend(v_column(7, 5, 9));
    marks.push((7, 4, b'S'));
    let scene = scenario(1, &marks);
    let seed = seed_where(&scene, &[6, 14], |d| {
        d.first() != Some(&1) && d.get(1) != Some(&0)
    });
    let mut run = start(&scene, seed);
    place(&mut run, 0, 152, 90, heading(Direction::Left));
    idle(&mut run, 1);
    let monster = run.state().monster(slot(0));
    assert_eq!(monster.direction(), Some(Direction::Down));
    assert_eq!(monster.position(), Px::new(152, 93));
    assert_eq!(monster.delay(), 1);
    idle(&mut run, 1);
    assert_eq!(
        run.state().monster(slot(0)).position(),
        Px::new(152, 93),
        "skips the next tick"
    );
}

#[test]
fn adjacent_but_unconnected_tunnel_does_not_qualify() {
    // Columns 5 and 6 each hold a vertical tunnel over rows 3 to 5, side
    // by side but not joined: both cells keep their facing horizontal
    // slices. A nobbin heading up on the corner of (6,4), with the digger
    // below and to the right, prefers right (solid), then left (the
    // unconnected neighbour), then up.
    let mut marks = v_column(5, 3, 5);
    marks.extend(v_column(6, 3, 5));
    let scene = scenario(1, &marks);
    let seed = seed_where(&scene, &[6, 14], |d| {
        d.first() != Some(&1) && d.get(1) != Some(&0)
    });
    let mut run = start(&scene, seed);
    place(&mut run, 0, 132, 90, heading(Direction::Up));
    idle(&mut run, 1);
    let monster = run.state().monster(slot(0));
    assert_eq!(
        monster.direction(),
        Some(Direction::Up),
        "left did not qualify"
    );
    assert_eq!(monster.delay(), 0);
}

#[test]
fn target_dead_chases_the_other_digger() {
    // Two diggers; the monster chases digger 1, on the left and dying, so
    // its decision heads for digger 2 on the right.
    let rows = rows_with(&h_row(4));
    let scene = Scenario::new(GameMode::TwoDiggers).rows(rows.each_ref().map(String::as_str));
    let seed = seed_where(&scene, &[6, 14], |d| {
        d.first() != Some(&1) && d.get(1) != Some(&0)
    });
    let mut run = start(&scene, seed);
    run.place_digger(FIRST, Px::new(12, 90), Direction::Left)
        .expect("reachable");
    run.place_digger(DiggerId::ALL[1], Px::new(292, 90), Direction::Left)
        .expect("reachable");
    run.kill_digger(FIRST, KillCause::Monster).expect("playing");
    let undecided = MonsterSetup {
        direction: None,
        ..MonsterSetup::default()
    };
    place(&mut run, 0, 152, 90, undecided);
    idle(&mut run, 1);
    assert_eq!(run.state().monster(slot(0)).chase(), FIRST);
    assert_eq!(
        run.state().monster(slot(0)).direction(),
        Some(Direction::Right)
    );
}

#[test]
fn alternating_targets_with_two_diggers() {
    let mut run = Scenario::new(GameMode::TwoDiggers).seed(1).build();
    run.start();
    run.set_spawn_timer(0).expect("playing");
    // Make room so three monsters spawn quickly.
    let mut chases = Vec::new();
    for n in 0..3 {
        idle(&mut run, 1);
        chases.push(run.state().monster(slot(n)).chase());
        run.set_spawn_timer(0).expect("playing");
    }
    assert_eq!(
        chases,
        [DiggerId::ALL[0], DiggerId::ALL[1], DiggerId::ALL[0]]
    );
}

// --- 12.4 The move -------------------------------------------------------

#[test]
fn nobbin_advances_4_px_per_tick_in_corridor() {
    // Row 4 open; the digger is to the left, so the nobbin keeps heading
    // left. Corners at x = 252 and 232 take two draws, other ticks one;
    // the seed gives no double move.
    let scene = scenario(1, &h_row(4));
    let setup = |run: &mut Run| {
        run.place_digger(FIRST, Px::new(12, 180), Direction::Left)
            .expect("reachable");
        place(run, 0, 252, 90, MonsterSetup::default());
    };
    let expected = [2, 1, 1, 1, 1, 2, 1, 1];
    let seed = scene
        .search_seed(SEARCH, |mut run| {
            run.start();
            setup(&mut run);
            draws_per_tick(&mut run, 8) == expected
        })
        .expect("a seed exists");
    let mut run = start(&scene, seed);
    setup(&mut run);
    let mut xs = Vec::new();
    for _ in 0..8 {
        idle(&mut run, 1);
        xs.push(run.state().monster(slot(0)).position().x);
    }
    assert_eq!(xs, [248, 244, 240, 236, 232, 228, 224, 220]);
}

#[test]
fn monster_stops_at_playfield_edge() {
    let scene = scenario(1, &h_row(4));
    let seed = seed_where(&scene, &[14], |d| d != [0]);
    let mut run = start(&scene, seed);
    place(&mut run, 0, 16, 90, heading(Direction::Left));
    idle(&mut run, 1);
    let monster = run.state().monster(slot(0));
    assert_eq!(monster.position(), Px::new(12, 90));
}

#[test]
fn hobbin_digs_down_through_earth() {
    // A hobbin on the corner of (7,3) heading down, the digger straight
    // below: it digs down 3 pixels per move.
    let mut run = start(&scenario(1, &[]), 0);
    let hobbin = MonsterSetup {
        hobbin: true,
        direction: Some(Direction::Down),
        ..MonsterSetup::default()
    };
    place(&mut run, 0, 152, 72, hobbin);
    let below = CellPos::new(7, 4).expect("in the grid");
    assert!(run.state().field().cell(below).is_untouched());
    let mut ys = Vec::new();
    for _ in 0..4 {
        idle(&mut run, 1);
        ys.push(run.state().monster(slot(0)).position().y);
        assert_eq!(run.state().penalty().count_of(PenaltySite::DigStep), 1);
    }
    assert_eq!(ys, [75, 78, 81, 84]);
    let cell = run.state().field().cell(below);
    for slice in 0..4 {
        assert!(!cell.has_v_slice(slice), "slice {slice} dug");
    }
    assert!(cell.has_v_slice(4) && cell.has_v_slice(5));
    let monster = run.state().monster(slot(0));
    assert!(!monster.nobbin());
    assert_eq!(monster.counter(), 4, "one per move as a hobbin");
}

#[test]
fn quirk_q38_hobbin_destroys_emerald_without_score() {
    let mut run = start(&scenario(1, &[(7, 4, b'C')]), 0);
    let hobbin = MonsterSetup {
        hobbin: true,
        direction: Some(Direction::Down),
        ..MonsterSetup::default()
    };
    place(&mut run, 0, 152, 72, hobbin);
    let player = run.state().current_player();
    let cell = CellPos::new(7, 4).expect("in the grid");
    let count = run.state().emeralds().count(player);
    assert!(run.state().emeralds().has(player, cell));
    // Moving down, an emerald is eaten at remainder 9: the third move.
    idle(&mut run, 2);
    assert!(run.state().emeralds().has(player, cell));
    idle(&mut run, 1);
    assert!(!run.state().emeralds().has(player, cell));
    assert_eq!(run.state().emeralds().count(player), count - 1);
    assert_eq!(run.state().scores().visible(ScoreId::ALL[0]), 0);
    let penalty = run.state().penalty();
    assert_eq!(penalty.count_of(PenaltySite::EmeraldEaten), 1);
    assert_eq!(penalty.count_of(PenaltySite::ScoreCall), 0);
}

#[test]
fn quirk_q38_hobbin_dig_charged_without_direction() {
    let mut run = start(&scenario(1, &[]), 0);
    let hobbin = MonsterSetup {
        hobbin: true,
        direction: None,
        ..MonsterSetup::default()
    };
    // Between corners, so no decision gives it a direction.
    place(&mut run, 0, 156, 72, hobbin);
    idle(&mut run, 1);
    let monster = run.state().monster(slot(0));
    assert_eq!(monster.position(), Px::new(156, 72));
    assert_eq!(run.state().penalty().count_of(PenaltySite::DigStep), 1);
}

#[test]
fn quirk_q27_frozen_monsters_still_draw_random_numbers() {
    // The only digger is dying: monsters keep deciding and drawing but do
    // not move; a hobbin still digs.
    let mut run = start(&scenario(1, &h_row(4)), 0);
    place(&mut run, 0, 112, 90, MonsterSetup::default());
    let hobbin = MonsterSetup {
        hobbin: true,
        direction: Some(Direction::Down),
        ..MonsterSetup::default()
    };
    place(&mut run, 1, 212, 108, hobbin);
    run.kill_digger(FIRST, KillCause::Monster).expect("playing");
    let field = *run.state().field();
    let draws = draws_per_tick(&mut run, 6);
    assert!(
        draws.iter().all(|&d| d >= 2),
        "both draw every tick: {draws:?}"
    );
    assert_eq!(run.state().monster(slot(0)).position(), Px::new(112, 90));
    assert_eq!(run.state().monster(slot(1)).position(), Px::new(212, 108));
    assert_ne!(
        run.state().field(),
        &field,
        "the hobbin dug although it stayed"
    );
}

#[test]
fn quirk_q28_hobbin_stuck_at_edge() {
    // Fleeing in bonus mode at the left edge: its first priority points
    // off the playfield, so it stays, re-deciding at every move. The seed
    // keeps the noise from moving left out of first place.
    let scene = scenario(1, &[]);
    let seed = seed_where(&scene, &[6, 14, 6, 14, 6, 14], |d| {
        d.iter().step_by(2).all(|&n| n != 1)
    });
    let mut run = start(&scene, seed);
    run.set_bonus_mode(true).expect("playing");
    let hobbin = MonsterSetup {
        hobbin: true,
        direction: Some(Direction::Left),
        ..MonsterSetup::default()
    };
    place(&mut run, 0, 12, 108, hobbin);
    let mut observed = Vec::new();
    for _ in 0..4 {
        idle(&mut run, 1);
        let monster = run.state().monster(slot(0));
        observed.push((monster.position(), monster.direction(), monster.delay()));
    }
    let at = Px::new(12, 108);
    assert_eq!(
        observed,
        [(at, None, 0), (at, None, 1), (at, None, 0), (at, None, 1)]
    );
}

// --- 12.5 Monsters meeting monsters; transformations ----------------------

#[test]
fn follower_reverses_leader() {
    let scene = scenario(1, &h_row(4));
    // Slot 0 moves between corners: its draw is the round's first.
    let seed = seed_where(&scene, &[14, 14], |d| {
        d.first() != Some(&0) && d.get(1) != Some(&0)
    });
    let mut run = start(&scene, seed);
    place(&mut run, 0, 108, 90, MonsterSetup::default());
    place(&mut run, 1, 96, 90, MonsterSetup::default());
    idle(&mut run, 1);
    let state = run.state();
    assert_eq!(state.monster(slot(0)).position(), Px::new(104, 90));
    assert_eq!(state.monster(slot(0)).direction(), Some(Direction::Left));
    assert_eq!(state.monster(slot(0)).delay(), 1, "the mover's delay");
    assert_eq!(
        state.monster(slot(1)).direction(),
        Some(Direction::Right),
        "reversed between cells"
    );
    assert_eq!(state.monster(slot(1)).position(), Px::new(100, 90));
}

#[test]
fn standstill_monsters_are_not_reversed() {
    let mut run = start(&scenario(1, &h_row(4)), 0);
    place(&mut run, 0, 108, 90, MonsterSetup::default());
    let young = MonsterSetup {
        standstill: 3,
        ..MonsterSetup::default()
    };
    place(&mut run, 1, 96, 90, young);
    idle(&mut run, 1);
    assert_eq!(
        run.state().monster(slot(1)).direction(),
        Some(Direction::Left)
    );
}

#[test]
fn head_on_monsters_keep_directions() {
    let scene = scenario(1, &h_row(4));
    let seed = seed_where(&scene, &[14, 14], |d| {
        d.first() != Some(&0) && d.get(1) != Some(&0)
    });
    let mut run = start(&scene, seed);
    place(&mut run, 0, 108, 90, heading(Direction::Left));
    place(&mut run, 1, 96, 90, heading(Direction::Right));
    idle(&mut run, 1);
    let state = run.state();
    assert_eq!(state.monster(slot(0)).direction(), Some(Direction::Left));
    assert_eq!(state.monster(slot(1)).direction(), Some(Direction::Right));
    assert_eq!(state.monster(slot(0)).delay(), 1);
    assert_eq!(state.monster(slot(1)).delay(), 1);
    assert_eq!(
        state.penalty().count_of(PenaltySite::MonsterOverlap),
        2,
        "one per overlapped monster per move"
    );
}

#[test]
fn two_overlaps_on_level_9_make_a_hobbin() {
    // Head-on at level 9: each overlapping move counts 1; a counter above
    // 10 − 9 = 1 makes a hobbin on the following tick.
    let scene = scenario(9, &h_row(4));
    let setup = |run: &mut Run| {
        place(run, 0, 108, 90, heading(Direction::Left));
        place(run, 1, 96, 90, heading(Direction::Right));
    };
    // No double moves on the first three ticks (no corners are reached).
    let seed = scene
        .search_seed(SEARCH, |mut run| {
            run.start();
            setup(&mut run);
            draws_per_tick(&mut run, 3) == [2, 0, 2]
        })
        .expect("a seed exists");
    let mut run = start(&scene, seed);
    setup(&mut run);
    idle(&mut run, 1);
    assert_eq!(run.state().monster(slot(0)).counter(), 1);
    idle(&mut run, 1);
    assert_eq!(run.state().monster(slot(0)).counter(), 1, "waiting");
    idle(&mut run, 1);
    let monster = run.state().monster(slot(0));
    assert_eq!(monster.counter(), 2);
    assert!(monster.nobbin());
    idle(&mut run, 1);
    assert!(!run.state().monster(slot(0)).nobbin(), "a hobbin now");
}

// --- 12.6 Monsters meeting bags -------------------------------------------

#[test]
fn nobbin_pushes_bag_and_gains_2_delay() {
    let scene = scenario(1, &h_row(4));
    let seed = seed_where(&scene, &[14], |d| d != [0]);
    let mut run = start(&scene, seed);
    run.place_bag(BAG0, Px::new(112, 90))
        .expect("in the playfield");
    place(&mut run, 0, 96, 90, heading(Direction::Right));
    idle(&mut run, 1);
    let state = run.state();
    assert_eq!(state.bag(BAG0).position(), Px::new(116, 90));
    let monster = state.monster(slot(0));
    assert_eq!(
        monster.position(),
        Px::new(100, 90),
        "keeps its new position"
    );
    assert_eq!(monster.delay(), 2);
}

#[test]
fn nobbin_under_bag_turns_back() {
    let scene = scenario(1, &v_column(7, 2, 5));
    let seed = seed_where(&scene, &[14], |d| d != [0]);
    let mut run = start(&scene, seed);
    // The bag rests at (152, 36); moving up from y = 51 to 48 touches it.
    run.place_bag(BAG0, Px::new(152, 36))
        .expect("in the playfield");
    place(&mut run, 0, 152, 51, heading(Direction::Up));
    idle(&mut run, 1);
    let state = run.state();
    let monster = state.monster(slot(0));
    assert_eq!(monster.position(), Px::new(152, 51), "back where it was");
    assert_eq!(monster.direction(), Some(Direction::Down));
    assert_eq!(monster.counter(), 1);
    assert_eq!(monster.delay(), 1);
    assert_eq!(state.bag(BAG0).position(), Px::new(152, 36));
    assert_eq!(
        state.penalty().count_of(PenaltySite::MonsterBlockedRedraw),
        1
    );
}

#[test]
fn hobbin_eats_bag_on_second_move() {
    let scene = scenario(1, &v_column(7, 2, 5));
    let mut run = start(&scene, 0);
    run.place_bag(BAG0, Px::new(152, 36))
        .expect("in the playfield");
    let hobbin = MonsterSetup {
        hobbin: true,
        direction: Some(Direction::Up),
        ..MonsterSetup::default()
    };
    place(&mut run, 0, 152, 51, hobbin);
    // First move: blocked, the bag stays.
    idle(&mut run, 1);
    assert!(run.state().bag(BAG0).exists());
    assert_eq!(run.state().monster(slot(0)).counter(), 1);
    assert_eq!(run.state().monster(slot(0)).delay(), 1);
    // The delay is waited out, then the second move eats the bag.
    idle(&mut run, 1);
    assert!(run.state().bag(BAG0).exists());
    idle(&mut run, 1);
    assert!(!run.state().bag(BAG0).exists());
    assert!(!run.state().sprites().is_enabled(BAG0.sprite()));
}

#[test]
fn monster_takes_gold_without_score() {
    let scene = scenario(1, &h_row(4));
    let seed = seed_where(&scene, &[14], |d| d != [0]);
    let mut run = start(&scene, seed);
    run.place_gold(BAG0, Px::new(100, 90))
        .expect("in the playfield");
    place(&mut run, 0, 116, 90, heading(Direction::Left));
    idle(&mut run, 1);
    let state = run.state();
    assert!(!state.bag(BAG0).exists(), "the gold is gone");
    assert_eq!(state.scores().visible(ScoreId::ALL[0]), 0);
    let monster = state.monster(slot(0));
    assert_eq!(monster.delay(), 0, "gold clears the delay");
    assert_eq!(monster.position(), Px::new(112, 90));
    assert_eq!(state.penalty().count_of(PenaltySite::GoldCollected), 1);
}

// --- 12.7 Monsters meeting diggers ----------------------------------------

#[test]
fn quirk_q26_digger_walking_into_monster_dies_on_monsters_move() {
    let mut run = start(&scenario(1, &h_row(9)), 0);
    run.place_digger(FIRST, Px::new(80, 180), Direction::Right)
        .expect("reachable");
    let waiting = MonsterSetup {
        delay: 2,
        ..still()
    };
    place(&mut run, 0, 100, 180, waiting);
    // Tick 1: to 84, no overlap. Tick 2: to 88, overlapping the monster,
    // which is still waiting: the digger lives.
    tick(&mut run, "R");
    tick(&mut run, "R");
    let digger = run.state().digger(FIRST);
    assert_eq!(digger.position(), Px::new(88, 180));
    assert!(
        digger.alive(),
        "its own move into the monster does not kill"
    );
    // Tick 3: the monster's move draw overlaps it.
    tick(&mut run, ".");
    let digger = run.state().digger(FIRST);
    assert!(!digger.alive());
    assert_eq!(digger.death_stage(), Some(DeathStage::Monster));
    assert!(
        run.state().monster(slot(0)).alive(),
        "the monster is unharmed"
    );
}

#[test]
fn monster_running_into_digger_in_bonus_mode_is_eaten() {
    let scene = scenario(1, &h_row(9));
    let seed = seed_where(&scene, &[14], |d| d != [0]);
    let mut run = start(&scene, seed);
    run.set_bonus_mode(true).expect("playing");
    place(&mut run, 0, 168, 180, heading(Direction::Left));
    run.take_sounds();
    idle(&mut run, 1);
    let state = run.state();
    assert!(!state.monster(slot(0)).occupied(), "removed");
    assert!(!state.sprites().is_enabled(slot(0).sprite()));
    assert_eq!(state.scores().visible(ScoreId::ALL[0]), 200);
    assert_eq!(state.digger(FIRST).eat_multiplier(), 2);
    assert!(state.digger(FIRST).alive());
    let eats = run
        .take_sounds()
        .into_iter()
        .filter(|&c| c == SoundCommand::EatMonster)
        .count();
    assert_eq!(eats, 1);
    // Quirk Q31: the eaten monster is due again.
    assert_eq!(run.state().monsters().total(), 7);
}

#[test]
fn eaten_by_two_diggers_scores_both_once_removed() {
    let rows = rows_with(&h_row(9));
    let scene = Scenario::new(GameMode::TwoDiggers).rows(rows.each_ref().map(String::as_str));
    let seed = seed_where(&scene, &[14], |d| d != [0]);
    let mut run = start(&scene, seed);
    run.set_bonus_mode(true).expect("playing");
    // Both diggers stand where the monster's move, not their own draw,
    // first overlaps them: a digger whose own draw touches a monster in
    // bonus mode eats it before the monster moves.
    run.place_digger(FIRST, Px::new(152, 180), Direction::Right)
        .expect("reachable");
    run.place_digger(DiggerId::ALL[1], Px::new(152, 180), Direction::Left)
        .expect("reachable");
    place(&mut run, 0, 168, 180, heading(Direction::Left));
    run.take_sounds();
    idle(&mut run, 1);
    let state = run.state();
    assert!(!state.monster(slot(0)).occupied());
    assert_eq!(state.scores().visible(ScoreId::ALL[0]), 200);
    assert_eq!(state.scores().visible(ScoreId::ALL[1]), 200);
    assert_eq!(state.monsters().total(), 7, "removed once");
    let eats = run
        .take_sounds()
        .into_iter()
        .filter(|&c| c == SoundCommand::EatMonster)
        .count();
    assert_eq!(eats, 1);
}

#[test]
fn invincible_digger_not_killed_by_monster() {
    let scene = scenario(1, &h_row(9));
    let seed = seed_where(&scene, &[14], |d| d != [0]);
    let mut run = start(&scene, seed);
    run.set_invincible(FIRST, 50).expect("playing");
    place(&mut run, 0, 168, 180, heading(Direction::Left));
    idle(&mut run, 1);
    let state = run.state();
    assert!(state.digger(FIRST).alive());
    assert!(state.monster(slot(0)).alive());
}

// --- 12.8 Squashing --------------------------------------------------------

/// A bag over a shaft in column 7 from row 1 down to row 5, with a still
/// nobbin on the corner of (7,4).
fn shaft_with_nobbin() -> Run {
    let mut marks = v_column(7, 2, 5);
    marks.push((7, 1, b'B'));
    let mut run = start(&scenario(1, &marks), 0);
    run.set_spawn_timer(1000).expect("playing");
    place(&mut run, 0, 152, 90, still());
    run
}

#[test]
fn squashed_nobbin_scores_250_two_ticks_after_bag_stops() {
    let mut run = shaft_with_nobbin();
    assert!(run.run_until(|s| s.monster(slot(0)).dead(), 200));
    assert_eq!(run.state().monster(slot(0)).death(), MonsterDeath::Carried);
    assert!(run.run_until(|s| s.monster(slot(0)).death() == MonsterDeath::Ending, 200));
    let found_stopped = run.ticks();
    assert!(!run.state().bag(BAG0).is_falling());
    idle(&mut run, 1);
    assert!(run.state().monster(slot(0)).occupied(), "the delay");
    assert_eq!(run.state().scores().visible(ScoreId::ALL[0]), 0);
    idle(&mut run, 1);
    assert_eq!(run.ticks(), found_stopped + 2);
    assert!(!run.state().monster(slot(0)).occupied());
    assert_eq!(run.state().scores().visible(ScoreId::ALL[0]), 250);
}

#[test]
fn dead_monster_still_blocks_spawning() {
    let mut run = shaft_with_nobbin();
    place(&mut run, 1, 252, 18, still());
    place(&mut run, 2, 272, 18, still());
    assert!(run.run_until(|s| s.monster(slot(0)).dead(), 200));
    run.set_spawn_timer(0).expect("playing");
    let mut removed_on = None;
    for _ in 0..100 {
        let before = occupied(&run);
        idle(&mut run, 1);
        if !run.state().monster(slot(0)).occupied() && removed_on.is_none() {
            removed_on = Some(run.ticks());
            assert_eq!(run.state().monsters().spawned(), 0, "none while it counted");
        }
        if removed_on.is_some() && occupied(&run) > before {
            break;
        }
    }
    let removed_on = removed_on.expect("the monster was removed");
    assert_eq!(run.ticks(), removed_on + 1, "one spawns on the next tick");
    assert_eq!(run.state().monsters().spawned(), 1);
}

#[test]
fn nobbin_under_falling_bag_turns_down() {
    let mut marks = v_column(4, 2, 9);
    marks.push((4, 1, b'B'));
    let mut run = start(&scenario(1, &marks), 0);
    run.set_spawn_timer(1000).expect("playing");
    assert!(run.run_until(|s| s.bag(BAG0).is_falling(), 100));
    // A nobbin heading up lower in column 4, between corners.
    place(&mut run, 0, 92, 165, heading(Direction::Up));
    idle(&mut run, 1);
    assert!(run.state().bag(BAG0).is_falling());
    assert_eq!(
        run.state().monster(slot(0)).direction(),
        Some(Direction::Down)
    );
}

// --- 12.9 Penalty overflow and the draw pattern ----------------------------

#[test]
fn quirk_q02_slot_0_never_delayed_by_overflow() {
    // Five still nobbins stacked between corners: each move draw overlaps
    // the four others, 25 charges, so the overflow delays slots 1 to 5.
    let scene = scenario(1, &h_row(4));
    let setup = |run: &mut Run| {
        for n in 0..5 {
            place(run, n, 116, 90, still());
        }
    };
    let seed = scene
        .search_seed(SEARCH, |mut run| {
            run.start();
            setup(&mut run);
            draws_per_tick(&mut run, 1) == [5]
        })
        .expect("a seed exists");
    let mut run = start(&scene, seed);
    setup(&mut run);
    idle(&mut run, 1);
    assert!(run.state().penalty().value() >= 14);
    let delays: Vec<u16> = MonsterSlot::ALL
        .into_iter()
        .map(|s| run.state().monster(s).delay())
        .collect();
    // Every mover has 1 for its overlap; slots 1 to 5 one more, the empty
    // slot 5 included; slot 0 none.
    assert_eq!(delays, [1, 2, 2, 2, 2, 1]);
}

#[test]
fn draw_pattern_two_draws_on_corner_one_between_cells() {
    let scene = scenario(1, &h_row(4));
    // On a corner: the noise draw, then the double-move draw.
    let seed = seed_where(&scene, &[6, 14], |d| d.get(1) != Some(&0));
    let mut run = start(&scene, seed);
    place(&mut run, 0, 112, 90, MonsterSetup::default());
    assert_eq!(draws_per_tick(&mut run, 1), [2]);

    // Between corners: the double-move draw only.
    let seed = seed_where(&scene, &[14], |d| d != [0]);
    let mut run = start(&scene, seed);
    place(&mut run, 0, 116, 90, MonsterSetup::default());
    assert_eq!(draws_per_tick(&mut run, 1), [1]);
}

#[test]
fn no_draws_without_undelayed_living_monster() {
    // A waiting monster, a dead one carried by a falling bag, empty slots,
    // and a digger that moves and digs: the generator does not move.
    let mut marks = v_column(7, 2, 5);
    marks.push((7, 1, b'B'));
    marks.extend(h_row(9));
    let mut run = start(&scenario(1, &marks), 0);
    run.set_spawn_timer(1000).expect("playing");
    place(&mut run, 0, 152, 90, still());
    assert!(run.run_until(|s| s.monster(slot(0)).dead(), 200));
    let waiting = MonsterSetup {
        delay: 50,
        ..MonsterSetup::default()
    };
    place(&mut run, 1, 252, 90, waiting);
    let state = run.state().rng_state();
    let draws = run.state().rng_draws();
    run.set_script(Script::parse("L*5").expect("valid script"));
    assert_eq!(run.run_ticks(5), 5);
    assert_eq!(run.state().rng_state(), state);
    assert_eq!(run.state().rng_draws(), draws);
}

#[test]
fn second_move_from_a_corner_decides() {
    // The first move ends on the corner at x = 112, so the second move
    // decides and takes a noise draw after the double-move draw.
    let scene = scenario(1, &h_row(4));
    let seed = seed_where(&scene, &[14], |d| d == [0]);
    let mut run = start(&scene, seed);
    place(&mut run, 0, 116, 90, MonsterSetup::default());
    assert_eq!(draws_per_tick(&mut run, 1), [2]);
    assert_eq!(run.state().monster(slot(0)).position(), Px::new(108, 90));
}
