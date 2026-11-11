//! Scenario tests of the digger spec's bonus requirements (design D11):
//! the bonus's appearance, entering and leaving bonus mode, its signalling
//! and eating monsters by moving into them, one per spec scenario.

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a test failure"
)]

use digger_core::geom::Px;
use digger_core::sprite::SpriteId;
use digger_core::testkit::{KillCause, MonsterSetup, Run, Scenario, Script};
use digger_core::video::Intensity;
use digger_core::{DiggerId, Direction, GameMode, MonsterSlot, ScoreId, SoundCommand, Tempo, Tune};

const FIRST: DiggerId = DiggerId::ALL[0];
const SCORE: ScoreId = ScoreId::ALL[0];

/// Solid earth everywhere.
const EARTH: [&str; 10] = ["               "; 10];

/// Where the bonus appears.
const BONUS_AT: Px = Px::new(292, 18);

/// The main music at standard tempo.
const MAIN: SoundCommand = SoundCommand::Music {
    tune: Tune::Main,
    tempo: Tempo::Standard,
};

/// The bonus music at standard tempo.
const BONUS_MUSIC: SoundCommand = SoundCommand::Music {
    tune: Tune::Bonus,
    tempo: Tempo::Standard,
};

fn slot(n: u8) -> MonsterSlot {
    MonsterSlot::new(n).expect("a monster slot")
}

/// A one-player level-1 run on solid earth, started so that the next
/// frame is the round's first tick.
fn started() -> Run {
    let mut run = Scenario::new(GameMode::OnePlayer).rows(EARTH).build();
    run.start();
    run
}

/// Runs one tick of `entry` and returns the sound commands it produced.
fn tick(run: &mut Run, entry: &str) -> Vec<SoundCommand> {
    run.set_script(Script::parse(entry).expect("valid script"));
    run.take_sounds();
    assert_eq!(run.run_ticks(1), 1);
    run.take_sounds()
}

/// Runs one tick without input and returns its sound commands.
fn idle(run: &mut Run) -> Vec<SoundCommand> {
    tick(run, ".")
}

fn count(sounds: &[SoundCommand], wanted: SoundCommand) -> usize {
    sounds.iter().filter(|&&s| s == wanted).count()
}

fn intensity(run: &Run) -> Intensity {
    run.state().framebuffer().intensity()
}

/// Puts the digger beside the shown bonus, so that its draw on the next
/// tick overlaps it, and runs that tick: the pickup tick P. Returns P and
/// the tick's sounds.
fn pick_up_bonus(run: &mut Run) -> (u32, Vec<SoundCommand>) {
    run.show_bonus().expect("playing");
    run.place_digger(FIRST, Px::new(280, 18), Direction::Right)
        .expect("reachable");
    let before = run.state().scores().visible(SCORE);
    let sounds = idle(run);
    assert!(run.state().bonus_mode(), "the pickup starts bonus mode");
    assert_eq!(run.state().scores().visible(SCORE), before + 1000);
    (run.ticks(), sounds)
}

/// A nobbin that stays where it is: no direction, held in its standstill.
fn still() -> MonsterSetup {
    MonsterSetup {
        direction: None,
        standstill: u8::MAX,
        ..MonsterSetup::default()
    }
}

/// Runs a level-1 round on the built-in layout, removing every monster on
/// the tick after it spawns (outside bonus mode, so the round's total
/// stays 6), until all six have spawned; returns the tick of the last
/// spawn.
fn spawn_all_six(run: &mut Run) -> u32 {
    let mut last = 0;
    while run.state().monsters().spawned() < 6 {
        let before = run.state().monsters().spawned();
        idle(run);
        if run.state().monsters().spawned() > before {
            last = run.ticks();
        }
        for n in 0..6 {
            if run.state().monster(slot(n)).occupied() {
                run.remove_monster(slot(n)).expect("playing");
            }
        }
        assert!(run.ticks() < 1000, "six spawns within 1000 ticks");
    }
    last
}

// --- 13.1 Appearance ------------------------------------------------------

#[test]
fn cherry_appears_44_ticks_after_last_level_1_spawn() {
    let mut run = Scenario::new(GameMode::OnePlayer).build();
    run.start();
    let last = spawn_all_six(&mut run);
    assert_eq!(last, 231, "spawns 43 + 1 ticks apart from tick 11");
    assert!(run.state().monsters().bonus_pending());
    while run.ticks() < last + 43 {
        idle(&mut run);
        assert!(!run.state().bonus_visible(), "tick {}", run.ticks());
    }
    idle(&mut run);
    assert_eq!(run.ticks(), last + 44);
    let state = run.state();
    assert!(state.digger(FIRST).alive());
    assert!(state.bonus_visible());
    assert!(!state.monsters().bonus_pending(), "once per round");
    assert!(state.sprites().is_enabled(SpriteId::BONUS));
    assert_eq!(state.sprites().position(SpriteId::BONUS), BONUS_AT);
}

#[test]
fn bonus_removed_at_round_end() {
    let mut run = started();
    run.show_bonus().expect("playing");
    assert!(run.state().sprites().is_enabled(SpriteId::BONUS));
    run.kill_digger(FIRST, KillCause::Monster).expect("playing");
    assert!(run.run_until(|state| state.round() == 1, 10_000));
    let state = run.state();
    assert!(!state.bonus_visible());
    assert!(!state.sprites().is_enabled(SpriteId::BONUS));
    assert!(
        state.monsters().bonus_pending(),
        "the next round's bonus is due again"
    );
}

// --- 13.2 Pickup, duration and ending ------------------------------------

#[test]
fn bonus_pickup_scores_1000_and_removes_bonus() {
    let mut run = started();
    let (_, _) = pick_up_bonus(&mut run);
    let state = run.state();
    assert!(!state.bonus_visible());
    assert!(!state.sprites().is_enabled(SpriteId::BONUS));
    assert_eq!(state.digger(FIRST).eat_multiplier(), 1);
    // The countdown starts on the pickup tick itself.
    assert_eq!(state.bonus_time_left(), 229);
    assert_eq!(state.bonus_start_time_left(), 19);
}

#[test]
fn bonus_mode_lasts_ticks_p_to_p_plus_229_on_level_1() {
    let mut run = started();
    // No monster spawns beside the digger when bonus mode ends.
    run.set_spawn_timer(5000).expect("playing");
    let (p, _) = pick_up_bonus(&mut run);
    while run.ticks() < p + 229 {
        idle(&mut run);
        assert!(run.state().bonus_mode(), "tick {}", run.ticks());
    }
    assert_eq!(run.state().bonus_time_left(), 0);
    let sounds = idle(&mut run);
    assert_eq!(run.ticks(), p + 230);
    assert!(!run.state().bonus_mode(), "ended on tick P+230");
    assert!(run.state().digger(FIRST).alive());
    assert_eq!(intensity(&run), Intensity::Normal);
    let pulse_stop = sounds
        .iter()
        .position(|&s| s == SoundCommand::BonusPulseStop);
    let main = sounds.iter().position(|&s| s == MAIN);
    assert!(
        pulse_stop.is_some() && pulse_stop < main,
        "pulse stopped, then the main music: {sounds:?}"
    );
}

#[test]
fn bonus_mode_ends_when_all_diggers_dead() {
    let mut run = started();
    let (p, _) = pick_up_bonus(&mut run);
    idle(&mut run);
    assert!(run.state().bonus_mode());
    run.kill_digger(FIRST, KillCause::Monster).expect("playing");
    let sounds = idle(&mut run);
    assert_eq!(run.ticks(), p + 2);
    assert!(!run.state().bonus_mode(), "ended at once");
    assert_eq!(intensity(&run), Intensity::Normal);
    assert_eq!(count(&sounds, SoundCommand::BonusPulseStop), 1);
    assert_eq!(count(&sounds, MAIN), 1);
}

#[test]
fn bonus_mode_length_follows_level() {
    // Level 10's countdown is 50: bonus mode runs P through P+49.
    let mut run = Scenario::new(GameMode::OnePlayer)
        .rows(EARTH)
        .start_level(10)
        .build();
    run.start();
    let (p, _) = pick_up_bonus(&mut run);
    assert_eq!(run.state().bonus_time_left(), 49);
    while run.ticks() < p + 49 {
        idle(&mut run);
        assert!(run.state().bonus_mode());
    }
    idle(&mut run);
    assert!(!run.state().bonus_mode());
}

// --- 13.3 Signalling -----------------------------------------------------

#[test]
fn pulse_stops_and_bonus_music_starts_on_tick_p_plus_19() {
    let mut run = started();
    let (p, sounds) = pick_up_bonus(&mut run);
    assert_eq!(count(&sounds, SoundCommand::BonusPulseStart), 1);
    // 229 left after the pickup tick: odd, so normal intensity.
    assert_eq!(intensity(&run), Intensity::Normal);
    let mut expected = Intensity::High;
    while run.ticks() < p + 18 {
        let sounds = idle(&mut run);
        assert_eq!(
            sounds,
            [SoundCommand::BonusPulseStart],
            "tick {}",
            run.ticks()
        );
        assert_eq!(intensity(&run), expected, "tick {}", run.ticks());
        expected = match expected {
            Intensity::High => Intensity::Normal,
            Intensity::Normal => Intensity::High,
        };
    }
    let sounds = idle(&mut run);
    assert_eq!(run.ticks(), p + 19);
    assert_eq!(
        sounds,
        [
            SoundCommand::BonusPulseStart,
            BONUS_MUSIC,
            SoundCommand::BonusPulseStop
        ]
    );
    assert_eq!(intensity(&run), Intensity::High);
    // Then the picture is held at high intensity, silently.
    let sounds = idle(&mut run);
    assert!(sounds.is_empty(), "{sounds:?}");
    assert_eq!(intensity(&run), Intensity::High);
}

#[test]
fn intensity_alternates_in_final_20_ticks() {
    let mut run = started();
    let (p, _) = pick_up_bonus(&mut run);
    while run.ticks() < p + 209 {
        let sounds = idle(&mut run);
        if run.ticks() > p + 19 {
            assert!(sounds.is_empty(), "tick {}: {sounds:?}", run.ticks());
            assert_eq!(intensity(&run), Intensity::High);
        }
    }
    let mut seen = Vec::new();
    while run.ticks() < p + 229 {
        let sounds = idle(&mut run);
        assert_eq!(sounds, [SoundCommand::BonusPulseStart]);
        seen.push((run.state().bonus_time_left(), intensity(&run)));
    }
    let expected: Vec<(i16, Intensity)> = (0..20)
        .rev()
        .map(|left| {
            let intensity = if left % 2 == 1 {
                Intensity::Normal
            } else {
                Intensity::High
            };
            (left, intensity)
        })
        .collect();
    assert_eq!(seen, expected);
    // The opening countdown kept going below 0 during the final flash.
    assert_eq!(run.state().bonus_start_time_left(), -20);
    idle(&mut run);
    assert_eq!(intensity(&run), Intensity::Normal);
}

#[test]
fn respawn_in_bonus_mode_requests_bonus_music() {
    let mut run = Scenario::new(GameMode::TwoDiggers).rows(EARTH).build();
    run.start();
    run.set_spawn_timer(5000).expect("playing");
    let (_, _) = pick_up_bonus(&mut run);
    run.kill_digger(DiggerId::ALL[1], KillCause::Monster)
        .expect("playing");
    let mut sounds = Vec::new();
    while !run.state().digger(DiggerId::ALL[1]).alive() {
        sounds.extend(idle(&mut run));
        assert!(run.state().bonus_mode(), "the partner stays alive");
        assert!(run.ticks() < 200, "respawn within 200 ticks");
    }
    assert_eq!(sounds.last(), Some(&BONUS_MUSIC), "{sounds:?}");
}

// --- 13.4 Eating ------------------------------------------------------------

#[test]
fn two_monsters_eaten_in_one_move_score_200_then_400() {
    let mut run = started();
    run.place_digger(FIRST, Px::new(100, 90), Direction::Right)
        .expect("reachable");
    run.set_bonus_mode(true).expect("playing");
    // Out of reach of the digger's draw where it stands, overlapped after
    // one step right.
    run.place_monster_as(slot(1), Px::new(116, 90), still())
        .expect("inside");
    run.place_monster_as(slot(3), Px::new(118, 90), still())
        .expect("inside");
    let sounds = idle(&mut run);
    assert_eq!(run.state().digger(FIRST).position(), Px::new(100, 90));
    assert_eq!(count(&sounds, SoundCommand::EatMonster), 0);

    let sounds = tick(&mut run, "R");
    let state = run.state();
    assert_eq!(state.digger(FIRST).position(), Px::new(104, 90));
    assert!(!state.monster(slot(1)).occupied());
    assert!(!state.monster(slot(3)).occupied());
    assert_eq!(state.scores().visible(SCORE), 200 + 400);
    assert_eq!(state.digger(FIRST).eat_multiplier(), 4);
    assert_eq!(count(&sounds, SoundCommand::EatMonster), 2);
    assert!(state.digger(FIRST).alive());
    // Quirk Q31: both come back.
    assert_eq!(state.monsters().total(), 8);
}

#[test]
fn digger_moving_into_monster_outside_bonus_mode_eats_nothing() {
    let mut run = started();
    run.place_digger(FIRST, Px::new(100, 90), Direction::Right)
        .expect("reachable");
    run.set_spawn_timer(5000).expect("playing");
    run.place_monster_as(slot(0), Px::new(116, 90), still())
        .expect("inside");
    let sounds = tick(&mut run, "R");
    assert!(run.state().monster(slot(0)).occupied());
    assert_eq!(count(&sounds, SoundCommand::EatMonster), 0);
    assert_eq!(run.state().scores().visible(SCORE), 0);
}

#[test]
fn eaten_monster_returns_after_bonus_mode() {
    let mut run = Scenario::new(GameMode::OnePlayer).build();
    run.start();
    spawn_all_six(&mut run);
    assert_eq!(run.state().monsters().total(), 6);
    run.place_digger(FIRST, Px::new(152, 180), Direction::Right)
        .expect("reachable");
    run.set_bonus_mode(true).expect("playing");
    run.place_monster_as(slot(0), Px::new(168, 180), still())
        .expect("inside");
    tick(&mut run, "R");
    assert!(!run.state().monster(slot(0)).occupied(), "eaten");
    assert_eq!(run.state().monsters().total(), 7);
    assert_eq!(run.state().monsters().spawned(), 6);
    while run.state().bonus_mode() {
        assert_eq!(run.state().monsters().spawned(), 6, "none in bonus mode");
        idle(&mut run);
        assert!(run.ticks() < 1000);
    }
    // The tick that ended bonus mode spawned the returning monster.
    assert_eq!(run.state().monsters().spawned(), 7);
    assert!(run.state().monster(slot(0)).occupied());
}

#[test]
fn multiplier_resets_in_second_bonus_mode() {
    let mut run = started();
    run.set_spawn_timer(5000).expect("playing");
    run.place_digger(FIRST, Px::new(100, 90), Direction::Right)
        .expect("reachable");
    run.set_bonus_mode(true).expect("playing");
    run.place_monster_as(slot(0), Px::new(116, 90), still())
        .expect("inside");
    tick(&mut run, "R");
    assert_eq!(run.state().scores().visible(SCORE), 200);
    assert_eq!(run.state().digger(FIRST).eat_multiplier(), 2);
    while run.state().bonus_mode() {
        idle(&mut run);
        assert!(run.ticks() < 1000);
    }
    assert_eq!(
        run.state().digger(FIRST).eat_multiplier(),
        2,
        "not reset when bonus mode ends"
    );

    pick_up_bonus(&mut run);
    assert_eq!(run.state().digger(FIRST).eat_multiplier(), 1);
    run.place_digger(FIRST, Px::new(200, 90), Direction::Right)
        .expect("reachable");
    run.place_monster_as(slot(0), Px::new(216, 90), still())
        .expect("inside");
    let before = run.state().scores().visible(SCORE);
    tick(&mut run, "R");
    assert!(!run.state().monster(slot(0)).occupied());
    assert_eq!(run.state().scores().visible(SCORE), before + 200);
}
