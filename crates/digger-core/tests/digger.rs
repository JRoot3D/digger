//! Scenario tests of the digger spec (design D11), one per spec scenario,
//! named after it.

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a test failure"
)]

use digger_core::assets::ImageId;
use digger_core::geom::Px;
use digger_core::testkit::{KillCause, MonsterSetup, Run, Scenario, Script};
use digger_core::video::Framebuffer;
use digger_core::{
    BagId, CellPos, Controls, DeathStage, DiggerId, Direction, EmeraldStep, FireballId,
    FireballStage, GameMode, GauntletSeconds, MonsterSlot, Movement, Observer, PenaltySite,
    ScoreId, SlotId, SoundCommand, Tempo, Tune,
};

const FIRST: DiggerId = DiggerId::ALL[0];
const SECOND: DiggerId = DiggerId::ALL[1];

/// Solid earth everywhere, with one emerald in the bottom-right cell so
/// that the level is not complete before anything happens (game-flow
/// spec, "Level completion test").
const EARTH: [&str; 10] = [
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "              C",
];

/// A one-player run on `rows`, started so that the next frame is the
/// round's first tick.
fn started(rows: [&str; 10]) -> Run {
    let rows = with_spare_emerald(rows);
    let mut run = Scenario::new(GameMode::OnePlayer)
        .rows(rows.each_ref().map(String::as_str))
        .build();
    run.start();
    run
}

/// `rows`, with an emerald added in the top-right cell when the layout has
/// none, so that the level is not complete before anything happens
/// (game-flow spec, "Level completion test").
fn with_spare_emerald(rows: [&str; 10]) -> [String; 10] {
    let mut rows = rows.map(str::to_owned);
    if !rows.iter().any(|row| row.contains('C')) {
        rows[0].replace_range(14..15, "C");
    }
    rows
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
    // The level's only emerald is gone, so the tick also ends the main
    // phase and the music stops after the emerald's cues.
    let sounds = run.take_sounds();
    let tail = sounds.get(sounds.len() - 3..).expect("three cues");
    assert_eq!(
        tail,
        [
            SoundCommand::EmeraldClick,
            SoundCommand::EmeraldNote(note0),
            SoundCommand::MusicOff
        ]
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

// Fireballs (task group 9).

const FIRST_FIREBALL: FireballId = FireballId::ALL[0];

/// The number of fire cues for the first fireball among `sounds`.
fn fire_cues(sounds: &[SoundCommand]) -> usize {
    sounds
        .iter()
        .filter(|s| **s == SoundCommand::Fire(FIRST_FIREBALL))
        .count()
}

#[test]
fn fire_right_spawns_at_160_184() {
    let mut run = Scenario::new(GameMode::OnePlayer).build();
    run.start();
    run.take_sounds();
    run.set_script(Script::parse("F .").expect("valid script"));
    // Fire read on tick 1 is acted on on tick 2.
    run.run_ticks(1);
    assert!(run.state().digger(FIRST).not_firing());
    run.run_ticks(1);
    let digger = run.state().digger(FIRST);
    assert!(!digger.not_firing(), "a fireball is in play");
    assert!(!digger.fire_ready(), "shown without the fire-ready look");
    let fireball = digger.fireball();
    assert_eq!(fireball.position(), Px::new(160, 184));
    assert_eq!(fireball.direction(), Direction::Right);
    assert_eq!(fireball.stage(), FireballStage::Flying);
    let sprites = run.state().sprites();
    let sprite = FIRST_FIREBALL.sprite();
    assert!(sprites.is_enabled(sprite));
    assert_eq!(sprites.position(sprite), Px::new(160, 184));
    // The digger is drawn with a right-facing image without the look.
    let image = sprites.image(FIRST.sprite()).number();
    assert!((4..=6).contains(&image), "image {image}");
    assert_eq!(fire_cues(&run.take_sounds()), 1);
}

#[test]
fn fire_on_tick_21_after_press_on_tick_20() {
    let mut run = Scenario::new(GameMode::OnePlayer).build();
    run.set_script(Script::parse(".*19 F .").expect("valid script"));
    run.run_ticks(20);
    assert_eq!(run.ticks(), 20);
    assert!(run.state().digger(FIRST).not_firing(), "not yet on tick 20");
    assert_eq!(fire_cues(run.sounds()), 0);
    run.run_ticks(1);
    assert!(
        !run.state().digger(FIRST).not_firing(),
        "launched on tick 21"
    );
    assert_eq!(fire_cues(run.sounds()), 1);
}

#[test]
fn no_second_fireball_while_one_exists() {
    // Fire held throughout: one launch, then nothing while the fireball
    // flies, explodes and the digger recharges.
    let mut run = Scenario::new(GameMode::OnePlayer).build();
    run.set_script(Script::parse("F*40").expect("valid script"));
    run.run_ticks(2);
    assert!(!run.state().digger(FIRST).not_firing());
    run.run_ticks(38);
    assert_eq!(fire_cues(run.sounds()), 1, "a single launch in 40 ticks");
    assert!(!run.state().digger(FIRST).fire_ready());
}

/// Row 9 open from column 6 to column 9; the rest earth.
const SHORT_CORRIDOR: [&str; 10] = [
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "      HHHH     ",
];

/// Row 9 open all along.
const LONG_CORRIDOR: [&str; 10] = [
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "               ",
    "HHHHHHHHHHHHHHH",
];

/// Runs `n` ticks of `entry` and returns the first fireball's position and
/// stage after each.
fn flight(run: &mut Run, entry: &str, n: usize) -> Vec<(Px, FireballStage)> {
    (0..n)
        .map(|_| {
            run.set_script(Script::parse(entry).expect("valid script"));
            run.run_ticks(1);
            let fireball = run.state().digger(FIRST).fireball();
            (fireball.position(), fireball.stage())
        })
        .collect()
}

#[test]
fn fireball_advances_8_px_per_tick() {
    let mut run = started(LONG_CORRIDOR);
    let path = flight(&mut run, "F", 5);
    let xs: Vec<i32> = path.iter().skip(1).map(|(at, _)| at.x).collect();
    assert_eq!(xs, [160, 168, 176, 184]);
    assert!(
        path.iter()
            .skip(1)
            .all(|(at, stage)| at.y == 184 && *stage == FireballStage::Flying)
    );
    let penalty = run.state().penalty();
    assert_eq!(penalty.count_of(PenaltySite::FireballFlight), 1);
}

#[test]
fn quirk_q19_shot_into_earth_explodes_at_previous_position() {
    // The corridor ends at x = 211. The shot at x = 208 reads earth at
    // x 212..215 on row 188, touches no sprite, and is drawn back at 200.
    let mut run = started(SHORT_CORRIDOR);
    let path = flight(&mut run, "F", 8);
    let flying: Vec<i32> = path.iter().take(7).skip(1).map(|(at, _)| at.x).collect();
    assert_eq!(flying, [160, 168, 176, 184, 192, 200]);
    assert!(
        path.iter()
            .take(7)
            .all(|(_, stage)| *stage == FireballStage::Flying)
    );
    assert_eq!(
        path.get(7).copied(),
        Some((
            Px::new(200, 184),
            FireballStage::Exploding { next_frame: 1 }
        ))
    );
    assert_eq!(
        run.state().sprites().position(FIRST_FIREBALL.sprite()),
        Px::new(200, 184),
        "redrawn at the previous position"
    );
    // The redraw is a second flight-frame draw, not a second penalty.
    let penalty = run.state().penalty();
    assert_eq!(penalty.count_of(PenaltySite::FireballFlight), 1);
}

#[test]
fn quirk_q19_probe_ignored_while_touching_a_sprite() {
    // Fired up from (152,180) into earth: at y = 173 the probe column
    // x = 160 shows earth, but the fireball still overlaps its own digger
    // (row 180), so it flies on. At y = 166 it touches nothing and is drawn
    // back at 173.
    let rows = [
        "               ",
        "               ",
        "               ",
        "               ",
        "               ",
        "               ",
        "               ",
        "               ",
        "               ",
        "       S       ",
    ];
    let mut run = started(rows);
    let fb = run.state().framebuffer();
    assert!(
        (173..180).any(|y| !fb.probe(Px::new(160, y)).is_empty()),
        "earth above the digger"
    );
    run.place_digger(FIRST, Px::new(152, 180), Direction::Up)
        .expect("a reachable position");
    let path = flight(&mut run, "F", 4);
    assert_eq!(
        path.get(1..).unwrap_or_default(),
        [
            (Px::new(156, 180), FireballStage::Flying),
            (Px::new(156, 173), FireballStage::Flying),
            (
                Px::new(156, 173),
                FireballStage::Exploding { next_frame: 1 }
            ),
        ]
    );
}

/// A two-digger run on `rows` with digger 1 at (`x1`, 180) facing right
/// and digger 2 at (`x2`, 180) facing left.
fn facing_pair(rows: [&str; 10], x1: i32, x2: i32) -> Run {
    let rows = with_spare_emerald(rows);
    let mut run = Scenario::new(GameMode::TwoDiggers)
        .rows(rows.each_ref().map(String::as_str))
        .build();
    run.start();
    run.place_digger(FIRST, Px::new(x1, 180), Direction::Right)
        .expect("a reachable position");
    run.place_digger(SECOND, Px::new(x2, 180), Direction::Left)
        .expect("a reachable position");
    run
}

#[test]
fn fireball_kills_nobbin_for_250() {
    let mut run = started(LONG_CORRIDOR);
    let slot = MonsterSlot::ALL[2];
    place_still_monster(&mut run, slot, Px::new(200, 180));
    let path = flight(&mut run, "F", 7);
    // Launched at 160, the shot first overlaps the nobbin at x = 200.
    assert_eq!(
        path.get(5..).unwrap_or_default(),
        [
            (Px::new(192, 184), FireballStage::Flying),
            (
                Px::new(200, 184),
                FireballStage::Exploding { next_frame: 1 }
            ),
        ]
    );
    let monster = run.state().monster(slot);
    assert!(!monster.occupied() && !monster.alive(), "removed at once");
    assert!(!run.state().sprites().is_enabled(slot.sprite()));
    assert_eq!(run.state().scores().visible(ScoreId::ALL[0]), 250);
}

#[test]
fn fireball_explodes_on_bag_without_harming_it() {
    let mut run = started(LONG_CORRIDOR);
    let bag = BagId::ALL[0];
    run.place_bag(bag, Px::new(200, 180))
        .expect("inside the playfield");
    let path = flight(&mut run, "F", 7);
    assert_eq!(
        path.last().copied(),
        Some((
            Px::new(200, 184),
            FireballStage::Exploding { next_frame: 1 }
        ))
    );
    let state = run.state();
    assert!(state.bag(bag).exists());
    assert_eq!(state.bag(bag).position(), Px::new(200, 180));
    assert!(state.sprites().is_enabled(bag.sprite()));
    assert_eq!(state.sprites().position(bag.sprite()), Px::new(200, 180));
    assert_eq!(state.scores().visible(ScoreId::ALL[0]), 0);
}

#[test]
fn fireball_up_explodes_at_y_11() {
    let mut rows = EARTH;
    rows[0] = "HHHHHHHHHHHHHHH";
    let mut run = placed(rows, 152, 18, Direction::Up);
    let path = flight(&mut run, "F", 3);
    assert_eq!(
        path.get(1..).unwrap_or_default(),
        [
            (Px::new(156, 18), FireballStage::Flying),
            (Px::new(156, 11), FireballStage::Exploding { next_frame: 1 }),
        ]
    );
}

#[test]
fn two_fireballs_explode_each_other() {
    let mut run = facing_pair(LONG_CORRIDOR, 52, 252);
    run.set_script(Script::parse("F/F*14").expect("valid script"));
    // Launched at 60 and 252 on tick 2; on tick 14 the first fireball
    // moves to x = 156, just short of the second at 164, which then moves
    // onto it, and both start exploding.
    run.run_ticks(13);
    let fireball = |run: &Run, unit: DiggerId| {
        let fireball = run.state().digger(unit).fireball();
        (fireball.position().x, fireball.stage())
    };
    assert_eq!(fireball(&run, FIRST), (148, FireballStage::Flying));
    assert_eq!(fireball(&run, SECOND), (164, FireballStage::Flying));
    run.run_ticks(1);
    let exploding = FireballStage::Exploding { next_frame: 1 };
    assert_eq!(fireball(&run, FIRST), (156, exploding));
    assert_eq!(fireball(&run, SECOND), (156, exploding));
    assert!(run.state().digger(FIRST).alive() && run.state().digger(SECOND).alive());
}

#[test]
fn fireball_kills_other_digger() {
    // Friendly fire: digger 1's shot, launched at x = 140 on tick 2,
    // reaches digger 2 at x = 212 on tick 11.
    let mut run = facing_pair(LONG_CORRIDOR, 132, 212);
    run.set_script(Script::parse("F*11").expect("valid script"));
    run.run_ticks(10);
    assert!(run.state().digger(SECOND).alive());
    run.run_ticks(1);
    let victim = run.state().digger(SECOND);
    assert!(!victim.alive());
    // Killed as by a monster during digger 1's update; digger 2's own
    // update on the same tick already spends the undrawn tick.
    assert_eq!(victim.death_stage(), Some(DeathStage::Arc));
    assert_eq!(victim.death_frame(), 0);
    let fireball = run.state().digger(FIRST).fireball();
    assert_eq!(fireball.stage(), FireballStage::Exploding { next_frame: 1 });
    assert!(run.state().digger(FIRST).alive());
}

#[test]
fn explosion_lasts_three_frames_then_removed() {
    // The shot starts exploding at x = 200 on tick E = 8.
    let mut run = started(SHORT_CORRIDOR);
    run.set_script(Script::parse("F*8").expect("valid script"));
    run.run_ticks(8);
    let exploding = |frame| FireballStage::Exploding { next_frame: frame };
    assert_eq!(run.state().digger(FIRST).fireball().stage(), exploding(1));
    run.take_sounds();
    let sprite = FIRST_FIREBALL.sprite();
    let mut seen = Vec::new();
    for _ in 0..4 {
        run.set_script(Script::parse(".").expect("valid script"));
        run.run_ticks(1);
        let state = run.state();
        let shown = state
            .sprites()
            .is_enabled(sprite)
            .then(|| state.sprites().image(sprite));
        let frames = state.penalty().count_of(PenaltySite::ExplosionFrame);
        seen.push((shown, frames, run.take_sounds()));
    }
    assert_eq!(
        seen,
        [
            (
                Some(ImageId::EXPLOSION_1),
                1,
                vec![SoundCommand::Explode(FIRST_FIREBALL)]
            ),
            (Some(ImageId::EXPLOSION_2), 1, vec![]),
            (Some(ImageId::EXPLOSION_3), 1, vec![]),
            (None, 0, vec![SoundCommand::FireStop(FIRST_FIREBALL)]),
        ]
    );
    let digger = run.state().digger(FIRST);
    assert!(digger.not_firing(), "gone on E+4");
    assert_eq!(digger.fireball().position(), Px::new(200, 184));
    assert_eq!(digger.recharge_time(), 63, "recharge begins next tick");
}

#[test]
fn level_1_next_shot_64_ticks_after_removal() {
    // Fire held throughout: the first fireball is removed on tick R = 12.
    let mut run = started(SHORT_CORRIDOR);
    run.set_script(Script::parse("F*100").expect("valid script"));
    run.run_ticks(11);
    assert!(!run.state().digger(FIRST).not_firing());
    run.run_ticks(1);
    assert!(run.state().digger(FIRST).not_firing(), "removed on tick R");
    run.take_sounds();
    run.run_ticks(62);
    let digger = run.state().digger(FIRST);
    assert!(!digger.fire_ready() && digger.recharge_time() == 1);
    run.run_ticks(1);
    let digger = run.state().digger(FIRST);
    assert!(digger.fire_ready(), "ready on tick R+63");
    assert!(digger.not_firing(), "not yet fired on R+63");
    assert_eq!(fire_cues(run.sounds()), 0);
    run.run_ticks(1);
    assert!(!run.state().digger(FIRST).not_firing(), "fired on R+64");
    assert_eq!(fire_cues(run.sounds()), 1);
}

/// Runs `n` ticks with no input.
fn idle(run: &mut Run, n: u32) {
    run.set_script(Script::default());
    assert_eq!(run.run_ticks(n), n);
}

/// The image number and position of `unit`'s sprite, or `None` while it is
/// not shown.
fn digger_sprite(run: &Run, unit: DiggerId) -> Option<(u8, Px)> {
    let sprites = run.state().sprites();
    let sprite = unit.sprite();
    sprites
        .is_enabled(sprite)
        .then(|| (sprites.image(sprite).number(), sprites.position(sprite)))
}

/// Whether `sounds` holds `command`.
fn has(sounds: &[SoundCommand], command: SoundCommand) -> bool {
    sounds.contains(&command)
}

/// A one-player run on solid earth whose digger is killed by a monster at
/// the end of tick T = 1; returns the run and T.
fn killed_alone(mode: GameMode) -> (Run, u32) {
    let mut run = Scenario::new(mode).rows(EARTH).build();
    run.start();
    idle(&mut run, 1);
    run.kill_digger(FIRST, KillCause::Monster)
        .expect("the digger is in play");
    (run, 1)
}

#[test]
fn death_arc_heights_177_175_174_174_175_177_180() {
    let (mut run, t) = killed_alone(GameMode::OnePlayer);
    run.take_sounds();
    // T+1: nothing is drawn; the sprite still shows the walking digger.
    idle(&mut run, 1);
    let (image, at) = digger_sprite(&run, FIRST).expect("shown");
    assert_ne!(image, ImageId::DIGGER_DEAD.number());
    assert_eq!(at, Px::new(152, 180));
    assert_eq!(run.state().penalty().count_of(PenaltySite::DeathArc), 0);
    assert_eq!(run.take_sounds(), []);
    // T+2 to T+8: the dead digger in its arc.
    let mut heights = Vec::new();
    let mut cues = Vec::new();
    for _ in 0..7 {
        idle(&mut run, 1);
        let (image, at) = digger_sprite(&run, FIRST).expect("shown");
        assert_eq!(image, ImageId::DIGGER_DEAD.number());
        assert_eq!(at.x, 152);
        heights.push(at.y);
        assert_eq!(run.state().penalty().count_of(PenaltySite::DeathArc), 1);
        let sounds = run.take_sounds();
        cues.push((
            has(&sounds, SoundCommand::DiggerDeath),
            has(&sounds, SoundCommand::MusicOff),
        ));
    }
    assert_eq!(run.ticks(), t + 8);
    assert_eq!(heights, [177, 175, 174, 174, 175, 177, 180]);
    let none = (false, false);
    assert_eq!(
        cues,
        [(true, false), none, none, none, none, none, (false, true)],
        "death cue on T+2, music off on T+8"
    );
    let digger = run.state().digger(FIRST);
    assert_eq!(digger.death_stage(), Some(DeathStage::Grave));
    assert_eq!(digger.position(), Px::new(152, 180));
}

#[test]
fn tombstone_frames_at_g_g3_g6_g9_g12() {
    let (mut run, t) = killed_alone(GameMode::OnePlayer);
    // The 5-tick wait ends on T+13; G = T+14.
    idle(&mut run, 13);
    run.take_sounds();
    let g = t + 14;
    let mut frames = Vec::new();
    for tick in g..=g + 13 {
        idle(&mut run, 1);
        assert_eq!(run.ticks(), tick);
        let (image, at) = digger_sprite(&run, FIRST).expect("shown");
        assert_eq!(at, Px::new(152, 180));
        if run.state().penalty().count_of(PenaltySite::TombstoneFrame) == 1 {
            frames.push((tick - g, image));
        }
    }
    assert_eq!(frames, [(0, 26), (3, 27), (6, 28), (9, 29), (12, 30)]);
    let dirges: Vec<_> = run
        .sounds()
        .iter()
        .filter(|s| matches!(s, SoundCommand::Music { .. }))
        .collect();
    assert_eq!(
        dirges,
        [&SoundCommand::Music {
            tune: Tune::Dirge,
            tempo: Tempo::Standard,
        }]
    );
    assert_eq!(
        run.state().digger(FIRST).death_stage(),
        Some(DeathStage::Tombstone)
    );

    // With two diggers the dirge is at the quick tempo.
    let mut run = Scenario::new(GameMode::TwoDiggers).rows(EARTH).build();
    run.start();
    idle(&mut run, 1);
    run.kill_digger(SECOND, KillCause::Fireball)
        .expect("the digger is in play");
    idle(&mut run, 13);
    run.take_sounds();
    idle(&mut run, 1);
    assert_eq!(
        digger_sprite(&run, SECOND).map(|(image, _)| image),
        Some(26 + 31)
    );
    assert!(has(
        run.sounds(),
        SoundCommand::Music {
            tune: Tune::Dirge,
            tempo: Tempo::Quick,
        }
    ));
}

#[test]
fn quirk_q23_one_digger_round_ends_at_g_plus_89() {
    let (mut run, t) = killed_alone(GameMode::OnePlayer);
    let g = t + 14;
    // The tombstone has rested its 60 ticks by G+73, but the death waits
    // for the dirge.
    idle(&mut run, g + 73 - t);
    let digger = run.state().digger(FIRST);
    assert_eq!(digger.death_stage(), Some(DeathStage::Tombstone));
    assert_eq!(digger.death_timer(), 0);
    assert!(!digger.dead());
    idle(&mut run, 88 - 73);
    assert_eq!(run.ticks(), g + 88);
    assert_eq!(run.state().round(), 0);
    assert_eq!(run.state().lives(FIRST), 3);
    assert!(run.state().diggers_visible());
    idle(&mut run, 1);
    assert_eq!(run.ticks(), g + 89);
    assert!(
        !run.state().diggers_visible(),
        "the main phase ended on tick G+89"
    );
    // A quiet wind-down of one tick ends the round.
    idle(&mut run, 1);
    assert_eq!(
        run.state().round(),
        1,
        "the round ended after the wind-down"
    );
    // The next round has begun at the start cell.
    let digger = run.state().digger(FIRST);
    assert!(digger.alive());
    assert_eq!(digger.position(), Px::new(152, 180));
}

/// The top line (score and lives) of `run`'s picture.
fn status_line(run: &Run) -> Vec<u8> {
    let rows = 14 * Framebuffer::WIDTH * 2;
    run.state()
        .framebuffer()
        .pixels()
        .get(..rows)
        .expect("the picture has a top line")
        .to_vec()
}

#[test]
fn one_player_death_leaves_2_lives() {
    let (mut run, _) = killed_alone(GameMode::OnePlayer);
    assert!(run.run_until(|s| s.round() == 1, 200));
    assert_eq!(run.state().lives(FIRST), 2);
    assert!(run.state().digger(FIRST).alive());
    // The status line shows one spare life, as a round started with 2 lives
    // does.
    let mut fresh = Scenario::new(GameMode::OnePlayer).rows(EARTH).build();
    fresh.set_lives(FIRST, 2).expect("in play");
    fresh.start();
    assert_eq!(status_line(&run), status_line(&fresh));
}

#[test]
fn gauntlet_death_keeps_lives() {
    let mode = GameMode::Gauntlet {
        time: GauntletSeconds::DEFAULT,
    };
    let (mut run, _) = killed_alone(mode);
    assert!(run.run_until(|s| s.round() == 1, 200));
    assert_eq!(run.state().lives(FIRST), 3);
    let digger = run.state().digger(FIRST);
    assert!(digger.alive(), "play resumes with a new round");
    assert_eq!(digger.position(), Px::new(152, 180));
    assert!(run.summary().is_none());
}

/// A two-digger run on solid earth whose second digger, with `lives`
/// lives, is killed by a monster at the end of tick T = 1; returns the run
/// and the tick R = G + 73 its death is resolved on.
fn partner_killed(lives: u32) -> (Run, u32) {
    let mut run = Scenario::new(GameMode::TwoDiggers).rows(EARTH).build();
    run.set_lives(SECOND, lives).expect("in play");
    run.start();
    idle(&mut run, 1);
    run.kill_digger(SECOND, KillCause::Monster)
        .expect("the digger is in play");
    (run, 1 + 14 + 73)
}

#[test]
fn respawn_at_132_180_with_2_lives_and_blinking() {
    let (mut run, r) = partner_killed(3);
    idle(&mut run, r - 2);
    assert!(!run.state().digger(SECOND).alive());
    run.take_sounds();
    idle(&mut run, 1);
    assert_eq!(run.ticks(), r);
    let state = run.state();
    let digger = state.digger(SECOND);
    assert_eq!(state.lives(SECOND), 2);
    assert!(digger.alive() && !digger.dead());
    assert_eq!(digger.position(), Px::new(132, 180));
    assert_eq!(digger.facing(), Some(Direction::Left));
    assert_eq!(digger.death_stage(), Some(DeathStage::Bag));
    assert!(digger.invincible());
    assert_eq!(digger.invincibility(), 50);
    assert_eq!(state.round(), 0, "the round goes on");
    let sprite = digger_sprite(&run, SECOND).expect("shown");
    assert_eq!(sprite.1, Px::new(132, 180));
    // The music in effect is requested again.
    assert!(has(
        run.sounds(),
        SoundCommand::Music {
            tune: Tune::Main,
            tempo: Tempo::Standard,
        }
    ));
    // It blinks: hidden on the draws where the count left modulo 10 is
    // below 5.
    let mut shown = Vec::new();
    for _ in 0..10 {
        idle(&mut run, 1);
        shown.push(digger_sprite(&run, SECOND).is_some());
    }
    assert_eq!(
        shown,
        [
            true, true, true, true, true, false, false, false, false, false
        ]
    );
}

#[test]
fn invincible_for_50_ticks_when_unblocked() {
    let (mut run, r) = partner_killed(3);
    idle(&mut run, r - 1);
    assert!(run.state().digger(SECOND).invincible());
    idle(&mut run, 49);
    let digger = run.state().digger(SECOND);
    assert!(digger.invincible(), "still invincible on R+49");
    assert_eq!(digger.invincibility(), 1);
    // A kill is ignored while it lasts.
    run.kill_digger(SECOND, KillCause::Monster)
        .expect("in play");
    assert!(run.state().digger(SECOND).alive());
    idle(&mut run, 1);
    let digger = run.state().digger(SECOND);
    assert!(!digger.invincible(), "over on R+50");
    assert_eq!(digger.invincibility(), 0);
    assert!(digger_sprite(&run, SECOND).is_some());
}

#[test]
fn last_life_lost_removes_tombstone_next_tick() {
    let (mut run, r) = partner_killed(1);
    idle(&mut run, r - 1);
    let state = run.state();
    let digger = state.digger(SECOND);
    assert_eq!(state.lives(SECOND), 0);
    assert!(!digger.alive() && digger.dead());
    assert_eq!(digger.death_stage(), Some(DeathStage::Tombstone));
    assert_eq!(
        digger_sprite(&run, SECOND),
        Some((30 + 31, Px::new(132, 180))),
        "the tombstone stays on tick R"
    );
    idle(&mut run, 1);
    assert_eq!(digger_sprite(&run, SECOND), None, "removed on R+1");
    let state = run.state();
    assert_eq!(state.digger(SECOND).death_stage(), Some(DeathStage::Done));
    assert_eq!(state.round(), 0, "digger 1 plays on");
    assert!(state.digger(FIRST).alive());
    idle(&mut run, 20);
    assert_eq!(digger_sprite(&run, SECOND), None);
    assert_eq!(run.state().lives(SECOND), 0);
}

#[test]
fn dying_digger_takes_no_movement_sample() {
    let mut run = Scenario::new(GameMode::TwoDiggers)
        .rows(EARTH)
        .build_observed(Recorder::default());
    run.start();
    assert_eq!(run.run_ticks(1), 1);
    run.kill_digger(FIRST, KillCause::Monster)
        .expect("the digger is in play");
    run.observer_mut().movements.clear();
    // The whole death sequence, until the dead digger respawns, with
    // controls held for both slots: only the living second digger is
    // sampled.
    run.set_script(Script::parse("RF/LF*120").expect("valid script"));
    let mut ticks = 0;
    while !run.state().digger(FIRST).alive() {
        assert_eq!(run.run_ticks(1), 1);
        ticks += 1;
        assert!(ticks < 200, "no respawn");
    }
    let slots: Vec<SlotId> = run.observer().movements.iter().map(|m| m.0).collect();
    assert!(ticks > 80, "a full death sequence ({ticks} ticks)");
    assert_eq!(slots, vec![SlotId::ALL[1]; ticks]);
}
