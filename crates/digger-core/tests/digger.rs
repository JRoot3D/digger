//! Scenario tests of the digger spec (design D11), one per spec scenario,
//! named after it.

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a test failure"
)]

use digger_core::geom::Px;
use digger_core::testkit::{Run, Scenario, Script};
use digger_core::{
    CellPos, Controls, DiggerId, Direction, EmeraldStep, GameMode, Movement, Observer, PenaltySite,
    ScoreId, SlotId, SoundCommand,
};

const FIRST: DiggerId = DiggerId::ALL[0];
const SECOND: DiggerId = DiggerId::ALL[1];

/// Solid earth everywhere.
const EARTH: [&str; 10] = ["               "; 10];

/// A one-player run on `rows`, started so that the next frame is the
/// round's first tick.
fn started(rows: [&str; 10]) -> Run {
    let mut run = Scenario::new(GameMode::OnePlayer).rows(rows).build();
    run.start();
    run
}

/// [`started`] with the first digger put at (`x`, `y`) facing `facing`.
fn placed(rows: [&str; 10], x: i32, y: i32, facing: Direction) -> Run {
    let mut run = started(rows);
    run.place_digger(FIRST, Px::new(x, y), facing)
        .expect("a reachable position");
    run
}

/// Runs one tick of `script` text and returns the first digger's position.
fn tick(run: &mut Run, entry: &str) -> Px {
    run.set_script(Script::parse(entry).expect("valid script"));
    assert_eq!(run.run_ticks(1), 1);
    run.state().digger(FIRST).position()
}

fn cell(h: u8, v: u8) -> CellPos {
    CellPos::new(h, v).expect("cell inside the grid")
}

/// The emerald note positions among `sounds`, in order.
fn notes(sounds: &[SoundCommand]) -> Vec<u8> {
    sounds
        .iter()
        .filter_map(|s| match s {
            SoundCommand::EmeraldNote(step) => Some(step.number()),
            _ => None,
        })
        .collect()
}

#[test]
fn one_player_starts_at_152_180_facing_right() {
    let mut run = Scenario::new(GameMode::OnePlayer).build();
    run.start();
    let digger = run.state().digger(FIRST);
    assert_eq!(digger.position(), Px::new(152, 180));
    assert_eq!(digger.facing(), Some(Direction::Right));
    assert!(digger.alive());
    assert_eq!(digger.bag_time(), 0, "no push pause");
    assert!(digger.not_firing() && digger.fire_ready(), "fireball ready");
    assert_eq!(digger.recharge_time(), 0);
    assert_eq!(digger.emerald_run(), 0);
    assert_eq!(digger.emerald_window(), 0);
    assert_eq!(digger.eat_multiplier(), 1);
    assert_eq!(digger.located().cell, cell(7, 9));
    assert!(run.state().diggers_visible());
}

#[test]
fn two_diggers_start_at_172_and_132() {
    let mut run = Scenario::new(GameMode::TwoDiggers).build();
    run.start();
    let first = run.state().digger(FIRST);
    assert_eq!(first.position(), Px::new(172, 180));
    assert_eq!(first.facing(), Some(Direction::Right));
    let second = run.state().digger(SECOND);
    assert_eq!(second.position(), Px::new(132, 180));
    assert_eq!(second.facing(), Some(Direction::Left));
    for digger in [first, second] {
        assert!(digger.alive());
        assert_eq!(digger.eat_multiplier(), 1);
        assert_eq!(digger.bag_time(), 0);
    }
}

#[test]
fn turn_requested_between_cells_waits_for_alignment() {
    let mut run = placed(EARTH, 20, 36, Direction::Right);
    let path: Vec<(Px, Option<Direction>)> = (0..4)
        .map(|_| {
            let at = tick(&mut run, "U");
            (at, run.state().digger(FIRST).facing())
        })
        .collect();
    assert_eq!(
        path,
        [
            (Px::new(24, 36), Some(Direction::Right)),
            (Px::new(28, 36), Some(Direction::Right)),
            (Px::new(32, 36), Some(Direction::Right)),
            (Px::new(32, 33), Some(Direction::Up)),
        ]
    );
}

#[test]
fn no_input_stops_between_cells() {
    let mut run = placed(EARTH, 20, 18, Direction::Right);
    assert_eq!(tick(&mut run, "."), Px::new(20, 18));
    let digger = run.state().digger(FIRST);
    assert_eq!(digger.facing(), Some(Direction::Right));
    assert_eq!(digger.moving(), None);
}

#[test]
fn reversal_is_immediate() {
    let mut run = placed(EARTH, 20, 18, Direction::Right);
    assert_eq!(tick(&mut run, "L"), Px::new(16, 18));
    assert_eq!(run.state().digger(FIRST).facing(), Some(Direction::Left));
}

/// Records every `controls` and `movement` call, and replaces the movement
/// direction when asked to.
#[derive(Default)]
struct Recorder {
    controls: Vec<(u32, SlotId)>,
    movements: Vec<(SlotId, Movement)>,
    replace: Option<Direction>,
}

impl Observer for Recorder {
    fn controls(&mut self, tick: u32, slot: SlotId, raw: Controls) -> Controls {
        self.controls.push((tick, slot));
        raw
    }

    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement {
        self.movements.push((slot, sample));
        match self.replace {
            Some(direction) => Movement {
                direction: Some(direction),
                ..sample
            },
            None => sample,
        }
    }
}

#[test]
fn observer_sees_one_movement_per_movable_tick() {
    let mut run = Scenario::new(GameMode::OnePlayer)
        .build_observed(Recorder::default())
        .script(Script::parse("R*5 .*5 UF").expect("valid script"));
    assert_eq!(run.run_ticks(11), 11);
    let obs = run.observer();
    let slot = SlotId::ALL[0];
    assert_eq!(
        obs.controls,
        (1..=11).map(|t| (t, slot)).collect::<Vec<_>>(),
        "one controls call per tick for the one slot in play"
    );
    assert_eq!(obs.movements.len(), 11, "one movement sample per tick");
    let right = Movement {
        direction: Some(Direction::Right),
        fire: false,
    };
    let up_fire = Movement {
        direction: Some(Direction::Up),
        fire: true,
    };
    let samples: Vec<Movement> = obs
        .movements
        .iter()
        .map(|&(s, m)| {
            assert_eq!(s, slot);
            m
        })
        .collect();
    let mut expected = vec![right; 5];
    expected.extend([Movement::default(); 5]);
    expected.push(up_fire);
    assert_eq!(samples, expected);

    // With two diggers both slots are read and sampled on every tick, the
    // first digger's before the second's.
    let mut run = Scenario::new(GameMode::TwoDiggers).build_observed(Recorder::default());
    assert_eq!(run.run_ticks(3), 3);
    let [s0, s1] = SlotId::ALL;
    assert_eq!(
        run.observer().controls,
        [(1, s0), (1, s1), (2, s0), (2, s1), (3, s0), (3, s1)]
    );
    let slots: Vec<SlotId> = run.observer().movements.iter().map(|m| m.0).collect();
    assert_eq!(slots, [s0, s1, s0, s1, s0, s1]);

    // A replaced sample's direction is the one the digger follows.
    let mut run = Scenario::new(GameMode::OnePlayer)
        .rows(EARTH)
        .build_observed(Recorder {
            replace: Some(Direction::Left),
            ..Recorder::default()
        })
        .script(Script::parse("R").expect("valid script"));
    run.run_ticks(1);
    assert_eq!(run.state().digger(FIRST).position(), Px::new(148, 180));
}

#[test]
fn crossing_a_cell_takes_5_or_6_ticks() {
    let mut run = placed(EARTH, 12, 18, Direction::Right);
    let ys: Vec<i32> = (0..6).map(|_| tick(&mut run, "D").y).collect();
    assert_eq!(ys, [21, 24, 27, 30, 33, 36]);
    assert_eq!(run.state().digger(FIRST).position(), Px::new(12, 36));

    let mut run = placed(EARTH, 12, 18, Direction::Right);
    let xs: Vec<i32> = (0..5).map(|_| tick(&mut run, "R").x).collect();
    assert_eq!(xs, [16, 20, 24, 28, 32]);
    assert_eq!(run.state().digger(FIRST).located().cell, cell(1, 0));
}

#[test]
fn stops_at_left_limit_keeping_facing() {
    let mut run = placed(EARTH, 12, 180, Direction::Right);
    let field = *run.state().field();
    assert_eq!(tick(&mut run, "L"), Px::new(12, 180));
    let digger = run.state().digger(FIRST);
    assert_eq!(digger.facing(), Some(Direction::Left));
    assert_eq!(digger.moving(), None);
    assert_eq!(run.state().field(), &field, "nothing dug");
    assert_eq!(run.state().penalty().count_of(PenaltySite::DigStep), 0);

    // The other three limits likewise.
    for (x, y, entry, facing) in [
        (292, 90, "R", Direction::Right),
        (12, 180, "D", Direction::Down),
        (12, 18, "U", Direction::Up),
    ] {
        let mut run = placed(EARTH, x, y, Direction::Left);
        let field = *run.state().field();
        assert_eq!(tick(&mut run, entry), Px::new(x, y));
        assert_eq!(run.state().digger(FIRST).facing(), Some(facing));
        assert_eq!(run.state().field(), &field);
    }
}

#[test]
fn tunnelling_clears_horizontal_slices() {
    let mut run = placed(EARTH, 12, 36, Direction::Right);
    assert!(run.state().field().cell(cell(1, 1)).is_untouched());
    for _ in 0..5 {
        tick(&mut run, "R");
    }
    assert_eq!(run.state().digger(FIRST).position(), Px::new(32, 36));
    let entered = run.state().field().cell(cell(1, 1));
    assert!(entered.has_no_h_slices());
    assert!(!entered.is_untouched());
    assert!(entered.has_all_v_slices());
    // The start cell is not dug by moving out of its corner.
    assert!(run.state().field().cell(cell(0, 1)).is_untouched());
    // The carved tunnel is black just behind the digger.
    let fb = run.state().framebuffer();
    let width = digger_core::video::Framebuffer::WIDTH;
    let real = |x: usize, y: usize| fb.pixels().get(2 * y * width + 2 * x).copied();
    assert_eq!(
        real(30, 44),
        Some(0),
        "tunnel pixel behind the digger is black"
    );
    assert_ne!(real(30, 70), Some(0), "earth below the tunnel");
}

#[test]
fn moving_digger_charges_dig_and_draw() {
    let mut run = placed(EARTH, 12, 36, Direction::Right);
    tick(&mut run, "R");
    let penalty = run.state().penalty();
    assert_eq!(penalty.count_of(PenaltySite::DigStep), 1);
    assert_eq!(penalty.count_of(PenaltySite::DiggerDraw), 1);
    assert_eq!(penalty.value(), 2);

    // Standing still: the draw only.
    tick(&mut run, ".");
    let penalty = run.state().penalty();
    assert_eq!(penalty.count_of(PenaltySite::DigStep), 0);
    assert_eq!(penalty.count_of(PenaltySite::DiggerDraw), 1);
    assert_eq!(penalty.value(), 1);
}

/// Earth with an emerald in cell (3, 1).
const EMERALD_3_1: [&str; 10] = [
    "               ",
    "   C           ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
];

#[test]
fn quirk_q16_emerald_eaten_at_x_64_moving_right() {
    let mut run = placed(EMERALD_3_1, 52, 36, Direction::Right);
    let player = run.state().current_player();
    assert_eq!(run.state().emeralds().count(player), 1);
    let mut seen = Vec::new();
    for _ in 0..3 {
        let at = tick(&mut run, "R");
        let penalty = run.state().penalty();
        seen.push((
            at.x,
            penalty.count_of(PenaltySite::EmeraldRedraw),
            penalty.count_of(PenaltySite::EmeraldEaten),
            run.state().emeralds().count(player),
        ));
    }
    // Redrawn at remainder 8 (x = 60), eaten at remainder 12 (x = 64).
    assert_eq!(seen, [(56, 0, 0, 1), (60, 1, 0, 1), (64, 0, 1, 0)]);
    assert!(!run.state().emeralds().has(player, cell(3, 1)));
    assert_eq!(run.state().scores().visible(ScoreId::ALL[0]), 25);
    let note0 = EmeraldStep::new(0).expect("note 0");
    let sounds = run.take_sounds();
    let tail = sounds.get(sounds.len() - 2..).expect("two cues");
    assert_eq!(
        tail,
        [SoundCommand::EmeraldClick, SoundCommand::EmeraldNote(note0)]
    );
}

#[test]
fn emerald_eaten_moving_up_at_remainder_9() {
    let mut run = placed(EMERALD_3_1, 72, 54, Direction::Up);
    let player = run.state().current_player();
    let mut seen = Vec::new();
    for _ in 0..3 {
        tick(&mut run, "U");
        let state = run.state();
        seen.push((
            state.digger(FIRST).position().y,
            state.penalty().count_of(PenaltySite::EmeraldRedraw),
            state.emeralds().count(player),
        ));
    }
    // y 51, 48, 45 are remainders 15, 12 (redraw) and 9 (eaten).
    assert_eq!(seen, [(51, 0, 1), (48, 1, 1), (45, 0, 0)]);

    // Left: redrawn at remainder 16, eaten at 12.
    let mut run = placed(EMERALD_3_1, 92, 36, Direction::Left);
    tick(&mut run, "L");
    assert_eq!(
        run.state().penalty().count_of(PenaltySite::EmeraldRedraw),
        1
    );
    tick(&mut run, "L");
    assert_eq!(run.state().emeralds().count(player), 0);
    assert_eq!(run.state().digger(FIRST).position(), Px::new(84, 36));

    // Down: the cell below is tested; redrawn at remainder 6, eaten at 9.
    let mut run = placed(EMERALD_3_1, 72, 18, Direction::Down);
    tick(&mut run, "D");
    tick(&mut run, "D");
    assert_eq!(
        run.state().penalty().count_of(PenaltySite::EmeraldRedraw),
        1
    );
    assert_eq!(run.state().emeralds().count(player), 1);
    tick(&mut run, "D");
    assert_eq!(run.state().emeralds().count(player), 0);
    assert_eq!(run.state().digger(FIRST).position(), Px::new(72, 27));
}

#[test]
fn emerald_tick_penalty_is_6() {
    let mut run = placed(EMERALD_3_1, 56, 36, Direction::Right);
    tick(&mut run, "R");
    tick(&mut run, "R");
    let penalty = run.state().penalty();
    assert_eq!(penalty.count_of(PenaltySite::EmeraldEaten), 1);
    assert_eq!(penalty.count_of(PenaltySite::DigStep), 1);
    assert_eq!(penalty.count_of(PenaltySite::DiggerDraw), 1);
    assert_eq!(penalty.count_of(PenaltySite::ScoreCall), 3);
    assert_eq!(penalty.value(), 6);
}

/// Earth with emeralds in cells 1 to 8 of row 1.
const ROW_OF_EIGHT: [&str; 10] = [
    "               ",
    " CCCCCCCC      ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
];

#[test]
fn eight_emerald_run_scores_450_with_notes_0_to_7() {
    let mut run = placed(ROW_OF_EIGHT, 12, 36, Direction::Right);
    run.take_sounds();
    run.set_script(Script::parse("R*38").expect("valid script"));
    assert_eq!(run.run_ticks(38), 38);
    let player = run.state().current_player();
    assert_eq!(run.state().emeralds().count(player), 0);
    assert_eq!(run.state().scores().visible(ScoreId::ALL[0]), 8 * 25 + 250);
    let sounds = run.take_sounds();
    assert_eq!(notes(&sounds), [0, 1, 2, 3, 4, 5, 6, 7]);
    let clicks = sounds
        .iter()
        .filter(|s| **s == SoundCommand::EmeraldClick)
        .count();
    assert_eq!(clicks, 8);
    assert_eq!(run.state().digger(FIRST).emerald_run(), 0, "run reset");
}

/// The note of the fourth emerald when it is eaten `pause` idle ticks after
/// the third, emeralds being 5 ticks apart when walking.
fn fourth_note_after_pause(pause: usize) -> Vec<u8> {
    let mut run = placed(ROW_OF_EIGHT, 12, 36, Direction::Right);
    run.take_sounds();
    // The third emerald is eaten on tick 13.
    let text = format!("R*13 .*{pause} R*5");
    run.set_script(Script::parse(&text).expect("valid script"));
    run.run_ticks(u32::try_from(18 + pause).expect("small"));
    notes(&run.take_sounds())
}

#[test]
fn run_restarts_after_9_idle_ticks() {
    // Eaten 9 ticks after the third: a new run.
    assert_eq!(fourth_note_after_pause(4), [0, 1, 2, 0]);
    // Eaten 8 ticks after: the run continues.
    assert_eq!(fourth_note_after_pause(3), [0, 1, 2, 3]);
}
