//! Scenario tests of the gold-bags spec (design D11), one per spec
//! scenario, named after it, plus the digger-spec scenarios about meeting
//! bags.

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a test failure"
)]

use digger_core::assets::ImageId;
use digger_core::geom::Px;
use digger_core::testkit::{KillCause, MonsterSetup, Run, Scenario, Script};
use digger_core::{
    BagId, CellPos, DeathStage, DiggerId, Direction, GameMode, MonsterSlot, PenaltySite, PlayerId,
    ScoreId, SoundCommand,
};

const FIRST: DiggerId = DiggerId::ALL[0];
const SECOND: DiggerId = DiggerId::ALL[1];
const BAG0: BagId = BagId::ALL[0];
const BAG1: BagId = BagId::ALL[1];

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

/// A run in `mode` on the layout of `marks`, started so that the next frame
/// is the round's first tick.
fn started_in(mode: GameMode, marks: &[(u8, u8, u8)]) -> Run {
    let rows = rows_with(marks);
    let refs = rows.each_ref().map(String::as_str);
    let mut run = Scenario::new(mode).rows(refs).build();
    run.start();
    run
}

/// [`started_in`] for a one-player game.
fn started(marks: &[(u8, u8, u8)]) -> Run {
    started_in(GameMode::OnePlayer, marks)
}

/// The top-left pixel of cell (`h`, `v`).
fn px(h: u8, v: u8) -> Px {
    CellPos::new(h, v).expect("cell inside the grid").to_px()
}

/// Puts the first digger at (`x`, `y`) facing `facing`.
fn place(run: &mut Run, x: i32, y: i32, facing: Direction) {
    run.place_digger(FIRST, Px::new(x, y), facing)
        .expect("a reachable position");
}

/// Runs one tick of `entry` and returns the sound commands of that tick.
fn tick(run: &mut Run, entry: &str) -> Vec<SoundCommand> {
    run.take_sounds();
    run.set_script(Script::parse(entry).expect("valid script"));
    assert_eq!(run.run_ticks(1), 1);
    run.take_sounds()
}

/// Runs `n` ticks without input.
fn idle(run: &mut Run, n: u32) {
    run.set_script(Script::default());
    assert_eq!(run.run_ticks(n), n);
}

/// Runs ticks without input until tick `t` has run.
fn idle_until(run: &mut Run, t: u32) {
    let n = t - run.ticks();
    idle(run, n);
}

/// A vertical shaft in column 3 from row `from` to row `to`.
fn shaft(from: u8, to: u8) -> Vec<(u8, u8, u8)> {
    (from..=to).map(|v| (3, v, b'V')).collect()
}

/// A bag at cell (3,1) over a shaft down to row `to`: it starts wobbling on
/// tick 1 and takes its first fall step on tick 17.
fn bag_over_shaft(to: u8) -> Vec<(u8, u8, u8)> {
    let mut marks = vec![(3, 1, b'B')];
    marks.extend(shaft(2, to));
    marks
}

/// The tick a bag over a two-cell shaft lands and breaks into gold.
const TWO_CELL_LANDING: u32 = 23;

// Bag creation ---------------------------------------------------------

/// Puts a nobbin into `slot` at `at` that stays where it is: it has no
/// direction and is held in its spawn standstill.
fn place_still_monster(run: &mut Run, slot: MonsterSlot, at: Px) {
    let still = MonsterSetup {
        direction: None,
        standstill: u8::MAX,
        ..MonsterSetup::default()
    };
    run.place_monster_as(slot, at, still)
        .expect("inside the playfield");
}

#[test]
fn level_1_has_seven_bags_first_at_1_2_second_at_4_0() {
    let mut run = Scenario::new(GameMode::OnePlayer).build();
    run.start();
    let state = run.state();
    assert!(BagId::ALL.iter().all(|&id| state.bag(id).exists()));
    assert_eq!(state.bag(BAG0).position(), px(1, 2));
    assert_eq!(state.bag(BAG1).position(), px(4, 0));
    for id in BagId::ALL {
        let bag = state.bag(id);
        assert_eq!(bag.direction(), None, "at rest");
        assert_eq!(bag.wobble(), 15);
        assert!(!bag.is_gold() && !bag.wobbling() && bag.unfallen());
        assert_eq!(bag.cells_fallen(), 0);
        let sprites = state.sprites();
        assert!(sprites.is_enabled(id.sprite()), "drawn by the level draw");
        assert_eq!(sprites.position(id.sprite()), bag.position());
        assert_eq!(sprites.image(id.sprite()), ImageId::BAG_STILL);
    }
    assert_eq!(state.saved_bag(PlayerId::ALL[0], BAG1), state.bag(BAG1));
}

#[test]
fn ninth_bag_cell_is_ignored() {
    let cells = [
        (0, 3),
        (0, 7),
        (2, 0),
        (2, 9),
        (5, 5),
        (5, 6),
        (9, 1),
        (12, 2),
        (14, 8),
    ];
    let marks: Vec<_> = cells.iter().map(|&(h, v)| (h, v, b'B')).collect();
    let run = started(&marks);
    let state = run.state();
    let positions: Vec<Px> = BagId::ALL
        .iter()
        .map(|&id| state.bag(id).position())
        .collect();
    let expected: Vec<Px> = cells[..7].iter().map(|&(h, v)| px(h, v)).collect();
    assert_eq!(positions, expected, "column by column, top to bottom");
    assert!(BagId::ALL.iter().all(|&id| state.bag(id).exists()));
    for (h, v) in [(12, 2), (14, 8)] {
        assert!(
            BagId::ALL
                .iter()
                .all(|&id| state.bag(id).position() != px(h, v)),
            "no bag at ({h},{v})"
        );
    }
}

#[test]
fn resting_bag_survives_a_death() {
    let mut run = started(&[(4, 0, b'B')]);
    place(&mut run, 76, 18, Direction::Right);
    tick(&mut run, "R");
    idle(&mut run, 10);
    assert_eq!(run.state().bag(BAG0).position(), px(5, 0));
    assert_eq!(run.state().bag(BAG0).direction(), None);
    run.kill_digger(FIRST, KillCause::Monster).expect("in play");
    assert!(run.run_until(|s| s.round() == 1, 500));
    let state = run.state();
    let bag = state.bag(BAG0);
    assert!(bag.exists());
    assert_eq!(bag.position(), px(5, 0));
    assert_eq!(state.saved_bag(PlayerId::ALL[0], BAG0), bag);
    assert!(state.sprites().is_enabled(BAG0.sprite()));
}

// Wobble ---------------------------------------------------------------

/// A bag at (3,4) and an opened cell (2,5) to its lower left.
const UNDER_DIG: [(u8, u8, u8); 2] = [(3, 4, b'B'), (2, 5, b'H')];

#[test]
fn bag_wobbles_when_cell_below_is_dug() {
    let mut run = started(&UNDER_DIG);
    place(&mut run, 52, 108, Direction::Right);
    idle(&mut run, 3);
    assert!(!run.state().bag(BAG0).wobbling(), "untouched below");
    tick(&mut run, "R");
    let state = run.state();
    let below = CellPos::new(3, 5).expect("in the grid");
    assert!(state.field().cell(below).has_tunnel());
    let bag = state.bag(BAG0);
    assert!(bag.wobbling(), "on the update after the first slice went");
    assert_eq!(bag.wobble(), 15);
    assert_eq!(bag.position(), px(3, 4));
}

#[test]
fn quirk_q36_no_wobble_while_digger_climbs_underneath() {
    let mut marks = vec![(3, 4, b'B')];
    marks.extend(shaft(5, 9));
    let mut run = started(&marks);
    // In row 6 of the shaft, climbing into row 5 and then against the bag
    // in row 4.
    place(&mut run, 72, 126, Direction::Up);
    for n in 0..12 {
        tick(&mut run, "U");
        assert!(!run.state().bag(BAG0).wobbling(), "climbing, tick {n}");
    }
    let digger = run.state().digger(FIRST);
    assert_eq!(digger.position(), Px::new(72, 105), "held below the bag");
    assert_eq!(digger.moving(), Some(Direction::Up));
    tick(&mut run, ".");
    assert!(run.state().bag(BAG0).wobbling(), "the digger stopped");
}

#[test]
fn first_fall_step_16_ticks_after_wobble_starts() {
    let mut run = started(&UNDER_DIG);
    place(&mut run, 52, 108, Direction::Right);
    tick(&mut run, "R");
    let w = run.ticks();
    assert!(run.state().bag(BAG0).wobbling());
    let mut wobbles = 0;
    for t in w + 1..=w + 15 {
        let sounds = tick(&mut run, ".");
        wobbles += sounds
            .iter()
            .filter(|&&s| s == SoundCommand::WobbleStart)
            .count();
        assert!(!sounds.contains(&SoundCommand::FallStart));
        assert_eq!(run.state().bag(BAG0).position(), px(3, 4), "tick {t}");
    }
    assert_eq!(wobbles, 8, "redrawn on every even count");
    assert_eq!(run.state().bag(BAG0).wobble(), 0);
    let sounds = tick(&mut run, ".");
    assert_eq!(run.ticks(), w + 16);
    assert!(sounds.contains(&SoundCommand::FallStart));
    let bag = run.state().bag(BAG0);
    assert_eq!(bag.position(), Px::new(72, 96));
    assert_eq!(bag.direction(), Some(Direction::Down));
    assert!(bag.wobbling(), "a wobble's fall still counts as wobbling");
    assert_eq!(
        run.state().sprites().image(BAG0.sprite()),
        ImageId::BAG_FALLING
    );
}

// Falling and landing ----------------------------------------------------

#[test]
fn bag_falls_one_cell_per_3_ticks() {
    let mut run = started(&bag_over_shaft(9));
    idle_until(&mut run, 16);
    let mut ys = Vec::new();
    for _ in 0..24 {
        idle(&mut run, 1);
        ys.push(run.state().bag(BAG0).position().y);
    }
    let expected: Vec<i32> = (1..=24).map(|k| 36 + 6 * k).collect();
    assert_eq!(ys, expected);
    // Every third tick it is on the next cell corner.
    assert_eq!(ys.get(2), Some(&54));
    assert_eq!(ys.get(5), Some(&72));
    idle(&mut run, 1);
    let bag = run.state().bag(BAG0);
    assert_eq!(bag.direction(), None, "landed on the bottom row");
    assert!(bag.is_gold());
}

#[test]
fn one_cell_drop_stays_intact() {
    let mut run = started(&bag_over_shaft(2));
    idle_until(&mut run, 19);
    assert_eq!(run.state().bag(BAG0).position(), px(3, 2));
    let sounds = tick(&mut run, ".");
    let bag = run.state().bag(BAG0);
    assert_eq!(bag.direction(), None, "landed on tick 20");
    assert!(!bag.is_gold());
    assert_eq!(bag.cells_fallen(), 0);
    assert_eq!(bag.wobble(), 15);
    assert!(!bag.wobbling() && !bag.unfallen());
    assert!(sounds.contains(&SoundCommand::FallStop));
    idle(&mut run, 10);
    assert!(!run.state().bag(BAG0).is_gold(), "no gold appears later");
    assert_eq!(
        run.state().sprites().image(BAG0.sprite()),
        ImageId::BAG_STILL
    );
}

#[test]
fn two_cell_drop_breaks() {
    let mut run = started(&bag_over_shaft(3));
    idle_until(&mut run, TWO_CELL_LANDING - 1);
    assert!(!run.state().bag(BAG0).is_gold());
    let landing = tick(&mut run, ".");
    let bag = run.state().bag(BAG0);
    assert_eq!(bag.position(), px(3, 3));
    assert_eq!(bag.gold_age(), 1, "gold from the landing tick");
    assert!(!landing.contains(&SoundCommand::Break));
    let next = tick(&mut run, ".");
    assert!(next.contains(&SoundCommand::Break));
    assert_eq!(run.state().sprites().image(BAG0.sprite()), ImageId::GOLD_1);
}

#[test]
fn quirk_q34_landing_bag_removes_overlapped_bag() {
    let mut run = started(&bag_over_shaft(9));
    run.place_bag(BAG1, px(3, 9)).expect("in the playfield");
    assert!(run.run_until(|s| s.bag(BAG0).direction() == Some(Direction::Down), 100));
    assert!(run.run_until(|s| s.bag(BAG0).direction().is_none(), 100));
    let state = run.state();
    assert_eq!(state.bag(BAG0).position(), px(3, 9));
    assert!(state.bag(BAG0).exists());
    assert!(!state.bag(BAG1).exists(), "removed by the landing bag");
    assert!(!state.sprites().is_enabled(BAG1.sprite()));
}

#[test]
fn removed_bag_keeps_its_last_values() {
    // Compat-quirks U07: a removed bag's record is still readable and
    // keeps the values it had when it was removed.
    let mut run = started(&bag_over_shaft(9));
    run.place_bag(BAG1, px(3, 9)).expect("in the playfield");
    let before = *run.state().bag(BAG1);
    assert!(run.run_until(|s| !s.bag(BAG1).exists(), 100));
    idle(&mut run, 20);
    let after = run.state().bag(BAG1);
    assert_eq!(after.position(), before.position());
    assert_eq!(after.located(), before.located());
    assert_eq!(after.direction(), before.direction());
    assert_eq!(after.is_gold(), before.is_gold());
}

#[test]
fn falling_bag_removes_emerald_without_score() {
    // The emerald is in the cell under the bag; the digger opens one slice
    // of that cell without reaching the emerald.
    let mut run = started(&[(3, 1, b'B'), (3, 2, b'C'), (2, 2, b'H')]);
    let player = PlayerId::ALL[0];
    let emerald = CellPos::new(3, 2).expect("in the grid");
    place(&mut run, 52, 54, Direction::Right);
    tick(&mut run, "R");
    assert!(run.state().emeralds().has(player, emerald));
    assert!(run.state().bag(BAG0).wobbling());
    idle_until(&mut run, 17);
    let state = run.state();
    assert!(!state.emeralds().has(player, emerald), "crushed");
    assert_eq!(state.scores().visible(ScoreId::ALL[0]), 0);
    assert_eq!(state.bag(BAG0).position(), Px::new(72, 42));
}

// Gold -------------------------------------------------------------------

#[test]
fn level_1_gold_disappears_139_ticks_after_landing() {
    // With alternating players both levels are level 1, so the gold
    // lifetime is 140 (see quirk Q40 for one-player games).
    let mut run = started_in(GameMode::TwoPlayers, &bag_over_shaft(3));
    assert_eq!(run.state().gold_time(), 140);
    let t = TWO_CELL_LANDING;
    idle_until(&mut run, t);
    assert!(run.state().bag(BAG0).is_gold());
    idle_until(&mut run, t + 138);
    assert!(run.state().bag(BAG0).exists());
    assert_eq!(run.state().bag(BAG0).gold_age(), 139);
    idle(&mut run, 1);
    assert!(!run.state().bag(BAG0).exists(), "gone on T+139");
    assert!(!run.state().sprites().is_enabled(BAG0.sprite()));
}

#[test]
fn quirk_q40_one_player_gold_lifetime_is_150_on_the_first_level() {
    // The lifetime was last set up for the second player's level 0
    // placeholder (quirk Q06).
    let mut run = started(&bag_over_shaft(3));
    assert_eq!(run.state().gold_time(), 150);
    let t = TWO_CELL_LANDING;
    idle_until(&mut run, t + 148);
    assert!(run.state().bag(BAG0).exists());
    idle(&mut run, 1);
    assert!(!run.state().bag(BAG0).exists(), "gone on T+149");
}

#[test]
fn gold_over_dug_cell_gone_within_10_ticks() {
    let mut run = started_in(GameMode::TwoPlayers, &bag_over_shaft(3));
    idle_until(&mut run, TWO_CELL_LANDING);
    // A digger crosses the cell below the gold, (3,4), along row 4.
    place(&mut run, 52, 90, Direction::Right);
    let below = CellPos::new(3, 4).expect("in the grid");
    for _ in 0..4 {
        tick(&mut run, "R");
    }
    assert!(run.state().field().cell(below).is_untouched());
    tick(&mut run, "R");
    let d = run.ticks();
    assert!(!run.state().field().cell(below).is_untouched(), "crossed");
    assert_eq!(run.state().bag(BAG0).gold_age(), 130);
    idle_until(&mut run, d + 9);
    assert!(run.state().bag(BAG0).exists());
    idle(&mut run, 1);
    assert!(!run.state().bag(BAG0).exists(), "gone on D+10");
    assert_eq!(run.state().scores().visible(ScoreId::ALL[0]), 0);
}

// Pushing ------------------------------------------------------------------

#[test]
fn free_push_moves_bag_4_px_and_slides() {
    let mut run = started(&[(4, 0, b'B')]);
    place(&mut run, 76, 18, Direction::Right);
    tick(&mut run, "R");
    let state = run.state();
    let bag = state.bag(BAG0);
    assert_eq!(bag.position(), Px::new(96, 18));
    assert_eq!(bag.direction(), Some(Direction::Right), "sliding right");
    let digger = state.digger(FIRST);
    assert_eq!(digger.position(), Px::new(80, 18), "keeps its new position");
    assert_eq!(digger.bag_time(), 1, "pauses on the next tick");
}

#[test]
fn push_blocked_by_monster_on_far_side() {
    let mut run = started(&[(4, 0, b'B')]);
    place(&mut run, 76, 18, Direction::Right);
    place_still_monster(&mut run, MonsterSlot::ALL[0], Px::new(108, 18));
    tick(&mut run, "R");
    let state = run.state();
    assert_eq!(state.bag(BAG0).position(), px(4, 0), "stays where it was");
    let digger = state.digger(FIRST);
    assert_eq!(digger.position(), Px::new(76, 18), "the pusher is blocked");
    assert_eq!(digger.facing(), Some(Direction::Left));
    assert_eq!(
        state.penalty().count_of(PenaltySite::PushRestore),
        1,
        "restored once"
    );
}

#[test]
fn quirk_q32_failed_push_rolls_bag_back_to_corner() {
    use Direction::{Left, Right};
    let mut run = started(&[(4, 0, b'B')]);
    place(&mut run, 76, 18, Direction::Right);
    place_still_monster(&mut run, MonsterSlot::ALL[0], Px::new(116, 18));
    let mut seen = Vec::new();
    for entry in ["R", ".", "L", "L", "L", "L", "L", "L", "L"] {
        tick(&mut run, entry);
        let bag = run.state().bag(BAG0);
        seen.push((bag.position().x, bag.direction()));
    }
    assert_eq!(
        seen,
        [
            (96, Some(Right)),
            (100, Some(Right)),
            (100, Some(Right)),
            // Its own slide runs into the monster: it stays, now sliding
            // left, 8 pixels past the corner.
            (100, Some(Left)),
            (100, Some(Left)),
            (96, Some(Left)),
            (96, Some(Left)),
            (92, Some(Left)),
            (92, None),
        ]
    );
}

#[test]
fn pushed_wobbling_bag_stops_wobbling() {
    let mut run = started(&[(4, 0, b'B'), (4, 1, b'V')]);
    place(&mut run, 76, 18, Direction::Right);
    idle(&mut run, 2);
    let bag = run.state().bag(BAG0);
    assert!(bag.wobbling());
    assert_eq!(bag.wobble(), 14);
    tick(&mut run, "R");
    let bag = run.state().bag(BAG0);
    assert_eq!(bag.position(), Px::new(96, 18));
    assert!(!bag.wobbling());
    assert_eq!(bag.wobble(), 15);
}

#[test]
fn two_bags_pushed_together() {
    let mut run = started(&[(4, 0, b'B')]);
    run.place_bag(BAG1, Px::new(108, 18))
        .expect("in the playfield");
    place(&mut run, 76, 18, Direction::Right);
    tick(&mut run, "R");
    let state = run.state();
    assert_eq!(state.bag(BAG0).position(), Px::new(96, 18));
    assert_eq!(state.bag(BAG1).position(), Px::new(112, 18));
    assert_eq!(state.digger(FIRST).position(), Px::new(80, 18));
}

#[test]
fn chain_against_wall_fails_and_reverses_first_bag() {
    let mut run = started(&[]);
    run.place_bag(BAG0, Px::new(276, 18))
        .expect("in the playfield");
    run.place_bag(BAG1, Px::new(292, 18))
        .expect("in the playfield");
    place(&mut run, 260, 18, Direction::Right);
    tick(&mut run, "R");
    let state = run.state();
    assert_eq!(state.bag(BAG0).position(), Px::new(276, 18), "stays");
    assert_eq!(state.bag(BAG0).direction(), Some(Direction::Left));
    assert_eq!(state.bag(BAG1).position(), Px::new(292, 18));
    assert_eq!(
        state.bag(BAG1).direction(),
        None,
        "the edge changes nothing"
    );
    let digger = state.digger(FIRST);
    assert_eq!(digger.position(), Px::new(260, 18), "the pusher is blocked");
    assert_eq!(digger.facing(), Some(Direction::Left));
    assert_eq!(state.penalty().count_of(PenaltySite::PushRestore), 1);
}

#[test]
fn quirk_q32_failed_chain_that_touches_a_monster_restores_twice() {
    let mut run = started(&[]);
    run.place_bag(BAG0, Px::new(276, 18))
        .expect("in the playfield");
    run.place_bag(BAG1, Px::new(292, 18))
        .expect("in the playfield");
    place_still_monster(&mut run, MonsterSlot::ALL[0], Px::new(292, 30));
    place(&mut run, 260, 18, Direction::Right);
    tick(&mut run, "R");
    let state = run.state();
    assert_eq!(state.bag(BAG0).position(), Px::new(276, 18));
    assert_eq!(state.penalty().count_of(PenaltySite::PushRestore), 2);
}

#[test]
fn one_push_slides_one_cell_in_8_ticks() {
    let mut run = started(&[(4, 0, b'B')]);
    place(&mut run, 76, 18, Direction::Right);
    let mut seen = Vec::new();
    for entry in ["R", ".", ".", ".", ".", ".", ".", ".", "."] {
        tick(&mut run, entry);
        let bag = run.state().bag(BAG0);
        seen.push((bag.position().x, bag.direction()));
    }
    let right = Some(Direction::Right);
    // Every slide step sets the shared push counter, so a lone sliding bag
    // moves on every second tick (quirk Q32).
    assert_eq!(
        seen,
        [
            (96, right),
            (100, right),
            (100, right),
            (104, right),
            (104, right),
            (108, right),
            (108, right),
            (112, right),
            (112, None),
        ]
    );
}

#[test]
fn quirk_q32_shared_counter_lets_second_bag_move_twice() {
    let mut run = started(&[]);
    run.place_bag(BAG0, Px::new(84, 18))
        .expect("in the playfield");
    run.place_bag(BAG1, Px::new(100, 18))
        .expect("in the playfield");
    place(&mut run, 68, 18, Direction::Right);
    tick(&mut run, "R");
    let state = run.state();
    assert_eq!(state.bag(BAG0).position(), Px::new(88, 18), "skipped");
    assert_eq!(
        state.bag(BAG1).position(),
        Px::new(108, 18),
        "pushed, then slid"
    );
    assert_eq!(state.push_count(), 1);
}

#[test]
fn bag_pushed_over_shaft_drops_without_wobble() {
    let mut run = started(&[(4, 0, b'B'), (5, 1, b'V')]);
    place(&mut run, 76, 18, Direction::Right);
    tick(&mut run, "R");
    idle(&mut run, 7);
    assert_eq!(run.state().bag(BAG0).position(), px(5, 0));
    assert_eq!(run.state().bag(BAG0).direction(), Some(Direction::Right));
    let sounds = tick(&mut run, ".");
    let state = run.state();
    let bag = state.bag(BAG0);
    assert_eq!(bag.position(), Px::new(112, 24), "falls at once");
    assert_eq!(bag.direction(), Some(Direction::Down));
    assert!(!bag.wobbling());
    assert_eq!(bag.wobble(), 0);
    assert!(sounds.contains(&SoundCommand::FallStart));
    assert!(!sounds.contains(&SoundCommand::WobbleStart));
    assert_eq!(
        state.moving_bags(),
        0,
        "a drop without wobble does not count"
    );
}

#[test]
fn bag_pushed_onto_solid_ground_comes_to_rest() {
    let mut run = started(&[(4, 0, b'B')]);
    place(&mut run, 76, 18, Direction::Right);
    tick(&mut run, "R");
    idle(&mut run, 8);
    let bag = run.state().bag(BAG0);
    assert_eq!(bag.position(), px(5, 0));
    assert_eq!(bag.direction(), None);
    assert_eq!(bag.wobble(), 15);
}

// Gold collection and the digger's side of meeting bags -----------------

/// A run whose bag broke into gold at (3,3) on tick 23.
fn with_gold() -> Run {
    let mut run = started(&bag_over_shaft(3));
    idle_until(&mut run, TWO_CELL_LANDING + 1);
    assert!(run.state().bag(BAG0).is_gold());
    run
}

#[test]
fn digger_collects_gold_for_500() {
    let mut run = with_gold();
    place(&mut run, 52, 72, Direction::Right);
    tick(&mut run, "R");
    assert!(run.state().bag(BAG0).exists());
    let sounds = tick(&mut run, "R");
    let state = run.state();
    assert_eq!(state.scores().visible(ScoreId::ALL[0]), 500);
    assert!(sounds.contains(&SoundCommand::Gold));
    assert!(!state.bag(BAG0).exists(), "the gold disappears");
    assert!(!state.sprites().is_enabled(BAG0.sprite()));
    let digger = state.digger(FIRST);
    assert_eq!(digger.position(), Px::new(60, 72), "keeps its new position");
    assert_eq!(digger.facing(), Some(Direction::Right));
    assert_eq!(
        state.penalty().count_of(PenaltySite::GoldCollected),
        1,
        "one collection"
    );
}

#[test]
fn gold_collected_from_below_does_not_turn_digger() {
    let mut run = with_gold();
    place(&mut run, 72, 90, Direction::Up);
    tick(&mut run, "U");
    assert!(run.state().bag(BAG0).exists());
    let sounds = tick(&mut run, "U");
    let state = run.state();
    assert!(sounds.contains(&SoundCommand::Gold));
    assert_eq!(state.scores().visible(ScoreId::ALL[0]), 500);
    assert!(!state.bag(BAG0).exists());
    let digger = state.digger(FIRST);
    assert_eq!(digger.position(), Px::new(72, 84));
    assert_eq!(digger.facing(), Some(Direction::Up), "not turned back");
    assert_eq!(digger.bag_time(), 0, "no pause after a vertical move");
}

#[test]
fn quirk_q17_push_pause_after_failed_push() {
    let mut run = started(&[(14, 0, b'B')]);
    place(&mut run, 276, 18, Direction::Right);
    let mut seen = Vec::new();
    for _ in 0..3 {
        tick(&mut run, "R");
        let digger = run.state().digger(FIRST);
        seen.push((digger.position().x, digger.bag_time()));
    }
    // Blocked, paused, blocked again.
    assert_eq!(seen, [(276, 1), (276, 0), (276, 1)]);
    assert_eq!(run.state().bag(BAG0).position(), px(14, 0));
}

#[test]
fn quirk_q15_fire_pressed_during_push_pause_fires_without_a_sample() {
    let mut run = started(&[(14, 0, b'B')]);
    place(&mut run, 276, 18, Direction::Right);
    assert!(run.state().digger(FIRST).not_firing());
    tick(&mut run, "R");
    assert_eq!(run.state().digger(FIRST).bag_time(), 1, "push pause next");
    // Fire is held only on the push-pause tick, which takes no movement
    // sample: the fire control is still read on that tick and fires on the
    // next one, so a replay fed only movement samples would not fire here.
    tick(&mut run, "F");
    assert!(run.state().digger(FIRST).not_firing());
    let sounds = tick(&mut run, ".");
    assert!(!run.state().digger(FIRST).not_firing(), "fireball launched");
    assert!(sounds.iter().any(|s| matches!(s, SoundCommand::Fire(_))));
}

#[test]
fn quirk_q18_blocked_digger_faces_back() {
    let mut run = started(&[(3, 3, b'B')]);
    place(&mut run, 72, 90, Direction::Up);
    tick(&mut run, "U");
    tick(&mut run, "U");
    let state = run.state();
    let digger = state.digger(FIRST);
    assert_eq!(digger.position(), Px::new(72, 87), "back where it was");
    assert_eq!(digger.facing(), Some(Direction::Down));
    assert_eq!(
        state.penalty().count_of(PenaltySite::DiggerBlockedRedraw),
        1
    );
    assert_eq!(state.sprites().position(FIRST.sprite()), Px::new(72, 87));
    assert!(!state.bag(BAG0).wobbling());
}

/// A two-digger run whose second digger respawned invincible at (132,180)
/// on tick R and has a resting bag put above it at (6,8), over the opened
/// cell (6,9); returns the run and R.
fn respawned_under_bag() -> (Run, u32) {
    let mut run = started_in(GameMode::TwoDiggers, &[(6, 9, b'S')]);
    idle(&mut run, 1);
    run.kill_digger(SECOND, KillCause::Monster)
        .expect("in play");
    let r = 1 + 14 + 73;
    idle_until(&mut run, r);
    assert!(run.state().digger(SECOND).invincible());
    assert_eq!(run.state().digger(SECOND).position(), Px::new(132, 180));
    run.place_bag(BAG0, px(6, 8)).expect("in the playfield");
    (run, r)
}

#[test]
fn invincible_digger_ignores_falling_bag() {
    let (mut run, r) = respawned_under_bag();
    // Wobbling from R+1, falling from R+17, landed on R+20.
    idle_until(&mut run, r + 17);
    assert_eq!(run.state().bag(BAG0).position(), Px::new(132, 168));
    idle_until(&mut run, r + 20);
    let state = run.state();
    assert_eq!(state.bag(BAG0).position(), px(6, 9));
    assert_eq!(state.bag(BAG0).direction(), None);
    let digger = state.digger(SECOND);
    assert!(digger.alive(), "not killed by the bag");
    assert_eq!(digger.position(), Px::new(132, 180), "not blocked");
    assert_eq!(digger.facing(), Some(Direction::Left));
}

#[test]
fn stationary_overlap_leaves_digger_without_facing() {
    let (mut run, r) = respawned_under_bag();
    idle_until(&mut run, r + 20);
    let before = run.state().digger(SECOND).invincibility();
    idle(&mut run, 1);
    let digger = run.state().digger(SECOND);
    assert!(digger.alive());
    assert_eq!(digger.facing(), None, "left without a facing");
    assert_eq!(
        before - digger.invincibility(),
        2,
        "the blocked redraw counts down a second time (quirk Q18)"
    );
    // A fire request is ignored while the facing is none.
    tick(&mut run, "./F");
    tick(&mut run, ".");
    let digger = run.state().digger(SECOND);
    assert!(digger.not_firing(), "the fireball stays ready");
    assert!(digger.alive());
}

// Crushing -------------------------------------------------------------

#[test]
fn falling_bag_kills_digger_below() {
    let mut run = started(&bag_over_shaft(9));
    place(&mut run, 72, 108, Direction::Up);
    // The bag is at y = 90 after tick 25 and at 96 after tick 26.
    idle_until(&mut run, 25);
    assert!(run.state().digger(FIRST).alive());
    idle(&mut run, 1);
    let state = run.state();
    assert_eq!(state.bag(BAG0).position(), Px::new(72, 96));
    let digger = state.digger(FIRST);
    assert!(!digger.alive(), "death by bag begins on that tick");
    assert_eq!(digger.death_stage(), Some(DeathStage::Bag));
    assert_eq!(digger.death_bag(), BAG0);
}

#[test]
fn digger_above_falling_bag_is_safe() {
    let mut marks = vec![(3, 4, b'B')];
    marks.extend(shaft(5, 7));
    let mut run = started(&marks);
    place(&mut run, 56, 90, Direction::Right);
    idle_until(&mut run, 17);
    assert_eq!(run.state().bag(BAG0).position(), Px::new(72, 96));
    // Moving right it overlaps the falling bag, but its top (90) is above
    // the bag's top (96).
    tick(&mut run, "R");
    let state = run.state();
    assert!(state.digger(FIRST).alive());
    assert_eq!(state.digger(FIRST).position(), Px::new(60, 90));
    assert_eq!(state.bag(BAG0).position(), Px::new(72, 102));
    idle(&mut run, 5);
    assert!(run.state().digger(FIRST).alive());
}

#[test]
fn quirk_q33_walking_into_falling_bag_kills_without_moving_it() {
    let mut marks = vec![(3, 4, b'B')];
    marks.extend(shaft(5, 7));
    let mut run = started(&marks);
    place(&mut run, 56, 108, Direction::Right);
    idle_until(&mut run, 17);
    assert_eq!(run.state().bag(BAG0).position(), Px::new(72, 96));
    tick(&mut run, "R");
    let state = run.state();
    let digger = state.digger(FIRST);
    assert!(!digger.alive(), "killed by the bag");
    assert_eq!(digger.death_stage(), Some(DeathStage::Bag));
    assert_eq!(digger.position(), Px::new(60, 108), "not blocked");
    let bag = state.bag(BAG0);
    assert_eq!(bag.position().x, 72, "not moved sideways");
    assert_eq!(bag.position().y, 102, "its own fall step");
    assert_eq!(state.penalty().count_of(PenaltySite::FallingBagPushed), 1);
}

#[test]
fn quirk_q21_crushed_digger_carried_at_bag_plus_6() {
    let mut run = started(&bag_over_shaft(9));
    place(&mut run, 72, 108, Direction::Up);
    idle_until(&mut run, 26);
    assert!(!run.state().digger(FIRST).alive());
    // From the tick after the bag passed y = 102 the digger is drawn 6
    // pixels below the bag's top as it stood before the bag's update.
    let mut bag_y = run.state().bag(BAG0).position().y;
    for _ in 0..8 {
        idle(&mut run, 1);
        let state = run.state();
        let drawn = state.sprites().position(FIRST.sprite()).y;
        assert_eq!(drawn, (bag_y + 6).max(108));
        assert_eq!(state.digger(FIRST).position().y, drawn);
        assert_eq!(state.sprites().image(FIRST.sprite()), ImageId::DIGGER_DEAD);
        bag_y = state.bag(BAG0).position().y;
    }
    // Carried to the floor: drawn at 186, then raised to 180 for the grave.
    assert!(run.run_until(|s| s.bag(BAG0).direction().is_none(), 100));
    idle(&mut run, 1);
    let state = run.state();
    assert_eq!(state.sprites().position(FIRST.sprite()).y, 186);
    assert_eq!(state.digger(FIRST).position().y, 180);
    assert_eq!(state.digger(FIRST).death_stage(), Some(DeathStage::Grave));
    assert_eq!(state.scores().visible(ScoreId::ALL[0]), 0, "no points");
}

// Penalty sites of the bags (scoring spec, "Penalty counter") -----------

/// The count of `site` on each of the next `n` ticks without input.
fn site_counts(run: &mut Run, site: PenaltySite, n: u32) -> Vec<u16> {
    (0..n)
        .map(|_| {
            idle(run, 1);
            run.state().penalty().count_of(site)
        })
        .collect()
}

#[test]
fn penalty_table_for_monsters_and_bags_bag_sites() {
    // A bag over a two-cell shaft: wobble from tick 1, fall steps from
    // tick 17, landing as gold on tick 23, gold frames after it.
    let mut run = started(&bag_over_shaft(3));
    let wobble = site_counts(&mut run, PenaltySite::WobbleFrame, 16);
    let falls = site_counts(&mut run, PenaltySite::FallStepDraw, 6);
    assert_eq!(run.ticks(), TWO_CELL_LANDING - 1);
    let landing = site_counts(&mut run, PenaltySite::BagLanding, 1);
    let gold = site_counts(&mut run, PenaltySite::GoldFrame, 6);
    // One wobble redraw on every second wobble tick.
    assert_eq!(wobble, [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1]);
    // One fall-step draw on every tick of the fall, one for the landing.
    assert_eq!(falls, [1; 6]);
    assert_eq!(landing, [1]);
    // One per gold opening frame, on the 1st, 3rd and 5th tick after it.
    assert_eq!(gold, [1, 0, 1, 0, 1, 0]);

    // A bag pushed once slides to the next cell corner.
    let mut run = started(&[(4, 0, b'B')]);
    place(&mut run, 76, 18, Direction::Right);
    tick(&mut run, "R");
    let slides = site_counts(&mut run, PenaltySite::SlideStepDraw, 8);
    // One per slide step; the shared push counter makes a lone bag step
    // on every second tick (quirk Q32).
    assert_eq!(slides, [1, 0, 1, 0, 1, 0, 1, 0]);
}

#[test]
fn facing_recovered_on_a_cell_corner() {
    // A bag put overlapping the left part of the standing digger leaves it
    // without a facing.
    let mut run = started(&[]);
    assert_eq!(run.state().digger(FIRST).position(), Px::new(152, 180));
    run.place_bag(BAG0, Px::new(140, 180))
        .expect("in the playfield");
    idle(&mut run, 1);
    assert_eq!(run.state().digger(FIRST).facing(), None);
    // On its cell corner it takes right: it faces right, is drawn again
    // and moves normally, clear of the bag.
    tick(&mut run, "R");
    let digger = run.state().digger(FIRST);
    assert_eq!(digger.facing(), Some(Direction::Right));
    assert_eq!(digger.position(), Px::new(156, 180));
    assert_eq!(
        run.state().sprites().position(FIRST.sprite()),
        Px::new(156, 180)
    );
    assert!(run.state().sprites().is_enabled(FIRST.sprite()));
    tick(&mut run, "R");
    assert_eq!(run.state().digger(FIRST).position(), Px::new(160, 180));
    assert_eq!(run.state().bag(BAG0).position(), Px::new(140, 180));
}

#[test]
fn facing_none_digger_under_a_bag_stays_blocked() {
    // The respawned second digger lies exactly under the bag that fell
    // onto it and has no facing.
    let (mut run, r) = respawned_under_bag();
    idle_until(&mut run, r + 21);
    let digger = run.state().digger(SECOND);
    assert_eq!(digger.facing(), None);
    assert_eq!(digger.position(), run.state().bag(BAG0).position());
    // Its push to the right is refused because the bag's new position
    // overlaps it, the pusher: it returns and faces left.
    tick(&mut run, "./R");
    let digger = run.state().digger(SECOND);
    assert_eq!(digger.facing(), Some(Direction::Left));
    assert_eq!(digger.position(), Px::new(132, 180));
    assert_eq!(run.state().bag(BAG0).position(), Px::new(132, 180));
}
