//! Tick-trace tests of the twelve sound effects (task group 4; design D12).
//!
//! Every test drives a [`ManualDriver`] tick by tick and checks the divisor
//! each tick offers against the numbers of the sound spec. Ticks are
//! counted from 1, the first being the tick on which the start command
//! takes effect.

use digger_sound::{
    Divisor, EmeraldStep, FireballId, ManualDriver, NoiseRng, SampleRate, SoundCommand,
    SoundConfig, SoundSink, TuneTables, Voice,
};

/// A driver for a new engine with the default configuration.
#[expect(
    clippy::expect_used,
    reason = "the embedded tables are tested to decode"
)]
fn new_driver() -> ManualDriver {
    let tables = TuneTables::embedded().expect("embedded tables decode");
    ManualDriver::new(SoundConfig::default(), tables)
}

/// Fireball `n`.
#[expect(clippy::expect_used, reason = "only 0 and 1 are passed")]
fn fireball(n: u8) -> FireballId {
    FireballId::new(n).expect("fireball 0 or 1")
}

/// Emerald step `n`.
#[expect(clippy::expect_used, reason = "only 0..=7 are passed")]
fn step(n: u8) -> EmeraldStep {
    EmeraldStep::new(n).expect("step 0..=7")
}

/// The divisors offered on the next `n` ticks.
fn offers(driver: &mut ManualDriver, n: usize) -> Vec<Option<u16>> {
    driver
        .advance_ticks(n)
        .iter()
        .map(|trace| trace.effect_offer.map(Divisor::get))
        .collect()
}

/// The divisors offered on `n` ticks after `command` is sent to a new
/// engine.
fn run(command: SoundCommand, n: usize) -> Vec<Option<u16>> {
    let mut driver = new_driver();
    driver.send(command);
    offers(&mut driver, n)
}

/// The divisors offered on `n` ticks of a new engine, sending each command
/// before the tick with the given 0-based index.
fn script(commands: &[(usize, SoundCommand)], n: usize) -> Vec<Option<u16>> {
    let mut driver = new_driver();
    (0..n)
        .flat_map(|tick| {
            for &(at, command) in commands {
                if at == tick {
                    driver.send(command);
                }
            }
            offers(&mut driver, 1)
        })
        .collect()
}

/// The offers of `n` ticks with `on` at the given 1-based ticks and
/// nothing elsewhere.
fn only(n: usize, on: &[(usize, u16)]) -> Vec<Option<u16>> {
    (1..=n)
        .map(|tick| {
            on.iter()
                .find(|&&(at, _)| at == tick)
                .map(|&(_, divisor)| divisor)
        })
        .collect()
}

/// The 1-based ticks on which something is offered.
fn offering_ticks(offers: &[Option<u16>]) -> Vec<usize> {
    offers
        .iter()
        .enumerate()
        .filter_map(|(i, offer)| offer.map(|_| i + 1))
        .collect()
}

// ---- 4.1 Fall, wobble and break ----------------------------------------

#[test]
fn fall_first_three_tones() {
    assert_eq!(
        run(SoundCommand::FallStart, 12),
        only(12, &[(3, 1000), (7, 1050), (11, 1100)])
    );
}

#[test]
fn fall_stop_ends_the_effect() {
    let mut driver = new_driver();
    driver.send(SoundCommand::FallStart);
    assert_eq!(offers(&mut driver, 3), only(3, &[(3, 1000)]));
    driver.send(SoundCommand::FallStop);
    assert_eq!(offers(&mut driver, 40), only(40, &[]));
}

#[test]
fn fall_restart_begins_same_cycle() {
    // A restart in the middle of any cycle, with or without a stop in
    // between, begins the first cycle again at divisor 1000 (fixed quirk
    // SF6).
    for ticks_before in 1..=9 {
        for with_stop in [false, true] {
            let mut driver = new_driver();
            driver.send(SoundCommand::FallStart);
            offers(&mut driver, ticks_before);
            if with_stop {
                driver.send(SoundCommand::FallStop);
                offers(&mut driver, 1);
            }
            driver.send(SoundCommand::FallStart);
            assert_eq!(
                offers(&mut driver, 8),
                only(8, &[(3, 1000), (7, 1050)]),
                "restart after {ticks_before} ticks, stop: {with_stop}"
            );
        }
    }
}

#[test]
fn wobble_full_cycle() {
    let expected = only(64, &[(16, 2500), (32, 3000), (48, 2500), (64, 2000)]);
    assert_eq!(run(SoundCommand::WobbleStart, 64), expected);
    // The cycle repeats.
    let mut twice = expected.clone();
    twice.extend(expected);
    assert_eq!(run(SoundCommand::WobbleStart, 128), twice);
}

#[test]
fn wobble_restart_resets_cycle() {
    let mut driver = new_driver();
    driver.send(SoundCommand::WobbleStart);
    assert_eq!(offers(&mut driver, 20), only(20, &[(16, 2500)]));
    driver.send(SoundCommand::WobbleStop);
    driver.send(SoundCommand::WobbleStart);
    assert_eq!(offers(&mut driver, 16), only(16, &[(16, 2500)]));
}

#[test]
fn wobble_start_while_wobbling_keeps_position() {
    // The simulation repeats wobble-start every other frame while a bag
    // wobbles; only wobble-stop resets the cycle.
    let repeated: Vec<(usize, SoundCommand)> = (0..64)
        .step_by(11)
        .map(|tick| (tick, SoundCommand::WobbleStart))
        .collect();
    assert_eq!(
        script(&repeated, 64),
        only(64, &[(16, 2500), (32, 3000), (48, 2500), (64, 2000)])
    );
}

#[test]
fn break_sounds_15000_for_three_ticks() {
    assert_eq!(
        run(SoundCommand::Break, 10),
        only(10, &[(1, 15_000), (2, 15_000), (3, 15_000)])
    );
    // A break during a break starts its three ticks again.
    assert_eq!(
        script(&[(0, SoundCommand::Break), (2, SoundCommand::Break)], 8),
        only(
            8,
            &[
                (1, 15_000),
                (2, 15_000),
                (3, 15_000),
                (4, 15_000),
                (5, 15_000)
            ]
        )
    );
}

// ---- 4.2 Fireball flight ------------------------------------------------

#[test]
fn fire_first_six_tones() {
    assert_eq!(
        run(SoundCommand::Fire(fireball(0)), 12),
        only(
            12,
            &[(2, 510), (4, 572), (6, 564), (8, 554), (10, 570), (12, 571)]
        )
    );
}

#[test]
fn fire_restart_keeps_cadence() {
    // Restarted 6 ticks after the first start (spec scenario): the base
    // returns to 500 and the next sounding base, 509, is on tick 8, the
    // next even tick of the original rhythm.
    let mut driver = new_driver();
    driver.send(SoundCommand::Fire(fireball(0)));
    offers(&mut driver, 6);
    driver.send(SoundCommand::Fire(fireball(0)));
    let after = offers(&mut driver, 2);
    let mut noise = NoiseRng::new();
    noise.draw(509 >> 3);
    noise.draw(518 >> 3);
    noise.draw(527 >> 3);
    let jitter = u16::try_from(noise.draw(509 >> 3)).ok();
    assert_eq!(after, [None, jitter.map(|j| 509 + j)]);

    // Restarted after an odd tick: the rhythm is kept, so the flight sounds
    // on the very next tick, where a fresh start would be silent (kept
    // quirk SQ4).
    let mut driver = new_driver();
    driver.send(SoundCommand::Fire(fireball(0)));
    offers(&mut driver, 5);
    driver.send(SoundCommand::Fire(fireball(0)));
    let after = offers(&mut driver, 4);
    assert_eq!(offering_ticks(&after), [1, 3]);
    let first = after.first().copied().flatten().unwrap_or(0);
    assert!((509..509 + 63).contains(&first), "{first}");

    // A fire-stop resets the rhythm as well.
    let mut driver = new_driver();
    driver.send(SoundCommand::Fire(fireball(0)));
    offers(&mut driver, 5);
    driver.send(SoundCommand::FireStop(fireball(0)));
    driver.send(SoundCommand::Fire(fireball(0)));
    assert_eq!(offering_ticks(&offers(&mut driver, 4)), [2, 4]);
}

#[test]
fn fire_ends_after_base_exceeds_30000() {
    let all = run(SoundCommand::Fire(fireball(1)), 600);
    let ticks = offering_ticks(&all);
    // 231 growth steps, on ticks 2, 4, ..., 462; the last one takes the
    // base from 29,954 to 30,345 and still sounds.
    assert_eq!(ticks.len(), 231);
    assert_eq!(ticks, (2..=462).step_by(2).collect::<Vec<_>>());
    let last = all.get(461).copied().flatten().unwrap_or(0);
    assert!((30_345..30_345 + (30_345 >> 3)).contains(&last), "{last}");
}

#[test]
fn fire_stop_ends_the_flight() {
    let mut driver = new_driver();
    driver.send(SoundCommand::Fire(fireball(0)));
    offers(&mut driver, 3);
    driver.send(SoundCommand::FireStop(fireball(0)));
    assert_eq!(offers(&mut driver, 20), only(20, &[]));
}

#[test]
fn two_fireballs_alternate() {
    // Fireball 1 starts 40 ticks after fireball 0, so both grow on the
    // same even ticks but fireball 0's base is 20 steps ahead: its tones
    // (base plus less than an eighth) never overlap fireball 1's.
    let all = script(
        &[
            (0, SoundCommand::Fire(fireball(0))),
            (40, SoundCommand::Fire(fireball(1))),
        ],
        80,
    );
    // The base after `steps` growth steps; a tone is its base plus less
    // than an eighth of it.
    let base = |steps: usize| (0..steps).fold(500_u16, |b, _| b + b / 55);
    let within = |divisor: u16, base: u16| (base..base + base / 8).contains(&divisor);
    let heard: Vec<u8> = (42..=80)
        .step_by(2)
        .map(|tick| {
            let divisor = all.get(tick - 1).copied().flatten().unwrap_or(0);
            if within(divisor, base(tick / 2)) {
                0
            } else {
                assert!(
                    within(divisor, base(tick / 2 - 20)),
                    "tick {tick}: {divisor}"
                );
                1
            }
        })
        .collect();
    assert_eq!(heard.len(), 20);
    for pair in heard.windows(2) {
        assert_ne!(pair.first(), pair.get(1), "{heard:?}");
    }
    // No odd tick sounds.
    assert!(offering_ticks(&all).iter().all(|t| t % 2 == 0));
}

// ---- 4.3 Explosion ------------------------------------------------------

/// The explosion's chirp.
const CHIRP: [u16; 10] = [1313, 1149, 1006, 881, 771, 675, 591, 518, 454, 398];

#[test]
fn explode_sequence() {
    let on: Vec<(usize, u16)> = CHIRP
        .iter()
        .copied()
        .enumerate()
        .map(|(i, d)| (i + 1, d))
        .collect();
    for n in 0..2 {
        assert_eq!(run(SoundCommand::Explode(fireball(n)), 20), only(20, &on));
    }
}

#[test]
fn explode_ends_flight() {
    let mut driver = new_driver();
    driver.send(SoundCommand::Fire(fireball(1)));
    offers(&mut driver, 3);
    driver.send(SoundCommand::Explode(fireball(1)));
    let on: Vec<(usize, u16)> = CHIRP
        .iter()
        .copied()
        .enumerate()
        .map(|(i, d)| (i + 1, d))
        .collect();
    // The flight, which would outrank the explosion on even ticks, offers
    // nothing from the explode tick on.
    assert_eq!(offers(&mut driver, 30), only(30, &on));

    // An explosion of the other fireball leaves the flight running.
    let mut driver = new_driver();
    driver.send(SoundCommand::Fire(fireball(1)));
    driver.send(SoundCommand::Explode(fireball(0)));
    let all = offers(&mut driver, 30);
    assert!(all.iter().skip(10).step_by(2).all(Option::is_none));
    assert!(all.iter().skip(11).step_by(2).all(Option::is_some));
}

#[test]
fn two_explosions_alternate() {
    // Fireball 1 explodes 3 ticks after fireball 0; on ticks 4 to 10 both
    // offer, and the one heard alternates.
    let all = script(
        &[
            (0, SoundCommand::Explode(fireball(0))),
            (3, SoundCommand::Explode(fireball(1))),
        ],
        16,
    );
    let heard: Vec<u8> = (4..=10)
        .map(|tick| {
            let divisor = all.get(tick - 1).copied().flatten();
            if divisor == CHIRP.get(tick - 1).copied() {
                0
            } else {
                assert_eq!(divisor, CHIRP.get(tick - 4).copied(), "tick {tick}");
                1
            }
        })
        .collect();
    for pair in heard.windows(2) {
        assert_ne!(pair.first(), pair.get(1), "{heard:?}");
    }
    // Before and after the overlap only one explosion sounds.
    assert_eq!(all.get(..3), Some(&CHIRP.map(Some)[..3]));
    assert_eq!(all.get(10..13), Some(&CHIRP.map(Some)[7..]));
    assert_eq!(all.get(13..), Some(&[None; 3][..]));
}

// ---- 4.4 Bonus siren, emerald click and emerald scale -------------------

/// The offers of the bonus siren's first 21 ticks.
fn bonus_cycle() -> Vec<Option<u16>> {
    let mut on: Vec<(usize, u16)> = Vec::new();
    on.extend((1..=5).map(|t| (t, 1230)));
    on.extend((8..=13).map(|t| (t, 1513)));
    on.extend((16..=21).map(|t| (t, 1230)));
    only(21, &on)
}

#[test]
fn bonus_first_cycle() {
    assert_eq!(run(SoundCommand::BonusPulseStart, 21), bonus_cycle());
}

#[test]
fn bonus_stop_resets_position() {
    let mut driver = new_driver();
    driver.send(SoundCommand::BonusPulseStart);
    offers(&mut driver, 10);
    driver.send(SoundCommand::BonusPulseStop);
    assert_eq!(offers(&mut driver, 3), only(3, &[]));
    driver.send(SoundCommand::BonusPulseStart);
    assert_eq!(offers(&mut driver, 21), bonus_cycle());
}

#[test]
fn bonus_start_while_sounding_keeps_position() {
    // The simulation repeats bonus-start on every frame of the flashing.
    let repeated: Vec<(usize, SoundCommand)> = (0..21)
        .step_by(6)
        .map(|tick| (tick, SoundCommand::BonusPulseStart))
        .collect();
    assert_eq!(script(&repeated, 21), bonus_cycle());
}

#[test]
fn click_is_one_tick_of_1000() {
    assert_eq!(run(SoundCommand::EmeraldClick, 5), only(5, &[(1, 1000)]));
}

/// The 1-based ticks on which an emerald note sounds.
const BURST_TICKS: [usize; 14] = [1, 2, 9, 10, 17, 18, 25, 26, 33, 34, 41, 42, 49, 50];

#[test]
fn emerald_burst_pattern() {
    let on: Vec<(usize, u16)> = BURST_TICKS.iter().map(|&t| (t, 2280)).collect();
    assert_eq!(run(SoundCommand::EmeraldNote(step(0)), 70), only(70, &on));
}

#[test]
fn emerald_scale_divisors() {
    let divisors = [2280, 2032, 1810, 1708, 1522, 1356, 1208, 1140];
    let hertz = [523, 587, 659, 698, 783, 879, 987, 1046];
    for (n, (divisor, hz)) in (0..8).zip(divisors.into_iter().zip(hertz)) {
        let on: Vec<(usize, u16)> = BURST_TICKS.iter().map(|&t| (t, divisor)).collect();
        assert_eq!(
            run(SoundCommand::EmeraldNote(step(n)), 60),
            only(60, &on),
            "step {n}"
        );
        assert_eq!(Divisor(divisor).hertz().get(), hz, "step {n}");
    }
}

#[test]
fn new_emerald_replaces_running_one() {
    let mut driver = new_driver();
    driver.send(SoundCommand::EmeraldNote(step(0)));
    assert_eq!(
        offers(&mut driver, 9),
        only(9, &[(1, 2280), (2, 2280), (9, 2280)])
    );
    driver.send(SoundCommand::EmeraldNote(step(3)));
    let on: Vec<(usize, u16)> = BURST_TICKS.iter().map(|&t| (t, 1708)).collect();
    assert_eq!(offers(&mut driver, 60), only(60, &on));
}

// ---- 4.5 Gold and eat-monster -------------------------------------------

/// The gold effect's 31 divisors.
const GOLD: [u16; 31] = [
    4000, 531, 3516, 599, 3091, 675, 2717, 761, 2389, 858, 2100, 967, 1846, 1091, 1623, 1231, 1427,
    1388, 1255, 1566, 1104, 1766, 971, 1993, 855, 2249, 752, 2538, 661, 2864, 582,
];

#[test]
fn gold_sequence() {
    let all = run(SoundCommand::Gold, 31);
    assert_eq!(all, GOLD.map(Some));
    // The spec's anchors.
    assert_eq!(
        all.get(..8),
        Some(&[4000, 531, 3516, 599, 3091, 675, 2717, 761].map(Some)[..])
    );
    assert_eq!(all.get(30), Some(&Some(582)));
}

#[test]
fn gold_lasts_31_ticks() {
    // Gold still sounds on the tick on which it ends (kept quirk SQ5).
    let all = run(SoundCommand::Gold, 60);
    assert_eq!(offering_ticks(&all), (1..=31).collect::<Vec<_>>());
}

/// One eat-monster chirp: the ten divisors on ticks 2, 4, ..., 20.
const EAT: [u16; 10] = [1758, 1649, 1360, 1275, 1052, 987, 815, 765, 632, 593];

/// The offers of one chirp starting at 1-based tick `first`.
fn eat_chirp(first: usize) -> Vec<(usize, u16)> {
    EAT.iter()
        .enumerate()
        .map(|(i, &d)| (first + 1 + 2 * i, d))
        .collect()
}

#[test]
fn eat_monster_chirp() {
    let all = run(SoundCommand::EatMonster, 21);
    assert_eq!(all, only(21, &eat_chirp(1)));
    assert!(all.iter().step_by(2).all(Option::is_none));
}

#[test]
fn eat_monster_three_chirps_63_ticks() {
    let mut on = eat_chirp(1);
    on.extend(eat_chirp(22));
    on.extend(eat_chirp(43));
    let all = run(SoundCommand::EatMonster, 100);
    assert_eq!(all, only(100, &on));
    // The same ten divisors from tick 23 and from tick 44; the last tone
    // is on tick 62, tick 63 is the last (silent) tick of the effect.
    assert_eq!(on.get(10), Some(&(23, 1758)));
    assert_eq!(on.get(20), Some(&(44, 1758)));
    assert_eq!(on.last(), Some(&(62, 593)));

    // The effect is over after tick 63: a second eat-monster sent then
    // starts afresh rather than continuing a fourth chirp.
    let restarted = script(
        &[
            (0, SoundCommand::EatMonster),
            (63, SoundCommand::EatMonster),
        ],
        84,
    );
    on.extend(eat_chirp(64));
    assert_eq!(restarted, only(84, &on));
}

// ---- 4.6 Death and extra life -------------------------------------------

/// The death effect's divisor on 1-based tick `tick`.
fn death_divisor(tick: u16) -> u16 {
    if tick <= 10 {
        20_000 - 1000 * tick
    } else {
        10_000 + 500 * (tick - 10)
    }
}

#[test]
fn death_divisor_sequence() {
    let all = run(SoundCommand::DiggerDeath, 51);
    let expected: Vec<Option<u16>> = (1..=51).map(|t| Some(death_divisor(t))).collect();
    assert_eq!(all, expected);
    assert_eq!(all.first(), Some(&Some(19_000)));
    assert_eq!(all.get(9), Some(&Some(10_000)));
    assert_eq!(all.get(10), Some(&Some(10_500)));
    assert_eq!(all.last(), Some(&Some(30_500)));
}

#[test]
fn death_lasts_51_ticks() {
    // Death offers on every tick up to and including the one on which its
    // divisor exceeds 30000 (kept quirk SQ5), and is over on tick 52.
    let all = run(SoundCommand::DiggerDeath, 80);
    assert_eq!(offering_ticks(&all), (1..=51).collect::<Vec<_>>());
}

#[test]
fn death_is_audible_on_ticks_4_to_22() {
    // The divisor gate silences 16384 and above, so death displaces music
    // on all 51 ticks but is heard on ticks 4 to 22 only (kept quirk SQ1).
    let mut driver = new_driver();
    driver.send(SoundCommand::DiggerDeath);
    let rate = SampleRate::DEFAULT;
    let audible: Vec<usize> = driver
        .advance_ticks(51)
        .iter()
        .enumerate()
        .filter(|(_, trace)| {
            assert!(matches!(trace.voice, Voice::Effect(_)));
            trace.voice.tone(rate).is_some()
        })
        .map(|(i, _)| i + 1)
        .collect();
    assert_eq!(audible, (4..=22).collect::<Vec<_>>());
    // 16000 is 74 Hz and 10000 is 119 Hz.
    assert_eq!(Divisor(16_000).hertz().get(), 74);
    assert_eq!(Divisor(10_000).hertz().get(), 119);
}

#[test]
fn extra_life_start() {
    let all = run(SoundCommand::ExtraLife, 8);
    assert_eq!(all, only(8, &[(2, 980), (3, 976), (4, 972), (8, 956)]));
}

#[test]
fn extra_life_end() {
    let all = run(SoundCommand::ExtraLife, 110);
    assert_eq!(
        all.get(91..),
        Some(&only(19, &[(1, 620), (2, 616), (3, 612)])[..])
    );
    // 3 ticks of tone and 3 of nothing throughout: 16 tones of 3 ticks.
    let ticks = offering_ticks(&all);
    assert_eq!(ticks.len(), 48);
    assert_eq!(ticks.last(), Some(&94));
}

// ---- 4.7 Priority and stop-all ------------------------------------------

/// One way to start each effect, in the spec's priority order, highest
/// first. The two fireball effects use different fireballs so that the
/// explosion does not end the flight.
fn effects_by_priority() -> [(&'static str, SoundCommand); 12] {
    [
        ("bonus siren", SoundCommand::BonusPulseStart),
        ("extra life", SoundCommand::ExtraLife),
        ("bag fall", SoundCommand::FallStart),
        ("eat monster", SoundCommand::EatMonster),
        ("fireball flight", SoundCommand::Fire(fireball(0))),
        ("explosion", SoundCommand::Explode(fireball(1))),
        ("emerald click", SoundCommand::EmeraldClick),
        ("gold", SoundCommand::Gold),
        ("bag break", SoundCommand::Break),
        ("death", SoundCommand::DiggerDeath),
        ("wobble", SoundCommand::WobbleStart),
        ("emerald scale", SoundCommand::EmeraldNote(step(0))),
    ]
}

#[test]
fn effect_priority_pairs() {
    const TICKS: usize = 140;
    let effects = effects_by_priority();
    for pair in effects.windows(2) {
        let [(high_name, high), (low_name, low)] = pair else {
            unreachable!("windows of two")
        };
        let mut both_offer = false;
        let mut high_silent = false;
        // The lower effect is started 0 to 63 ticks after the higher one,
        // so that every pair meets on some tick.
        for delay in 0..64 {
            let high_alone = script(&[(0, *high)], TICKS);
            let low_alone = script(&[(delay, *low)], TICKS);
            let together = script(&[(0, *high), (delay, *low)], TICKS);
            for (i, ((h, l), t)) in high_alone.iter().zip(&low_alone).zip(&together).enumerate() {
                // The higher offer wins; an absent higher offer lets the
                // lower one through.
                assert_eq!(
                    *t,
                    h.or(*l),
                    "{high_name} over {low_name}, delay {delay}, tick {}",
                    i + 1
                );
                both_offer |= h.is_some() && l.is_some() && h != l;
                high_silent |= h.is_none() && l.is_some();
            }
        }
        assert!(both_offer, "{high_name} and {low_name} never met");
        assert!(high_silent, "{low_name} never heard under {high_name}");
    }
}

#[test]
fn spec_priority_scenarios() {
    // Gold over break.
    let all = script(&[(0, SoundCommand::Gold), (0, SoundCommand::Break)], 4);
    assert_eq!(all, [Some(4000), Some(531), Some(3516), Some(599)]);
    // A silent tick of the fall lets gold's 531 through (tick 2).
    let all = script(&[(0, SoundCommand::FallStart), (0, SoundCommand::Gold)], 3);
    assert_eq!(all, [Some(4000), Some(531), Some(1000)]);
}

#[test]
fn stop_all_ends_every_effect() {
    let mut driver = new_driver();
    for (_, command) in effects_by_priority() {
        driver.send(command);
    }
    driver.send(SoundCommand::Fire(fireball(1)));
    driver.send(SoundCommand::Explode(fireball(0)));
    assert!(offers(&mut driver, 5).iter().all(Option::is_some));
    driver.send(SoundCommand::StopAll);
    let traces = driver.advance_ticks(200);
    assert!(
        traces
            .iter()
            .all(|t| t.effect_offer.is_none() && t.voice == Voice::Silent)
    );
    // Effects started after the stop-all play from their beginning.
    driver.send(SoundCommand::Gold);
    driver.send(SoundCommand::FallStart);
    assert_eq!(offers(&mut driver, 3), [Some(4000), Some(531), Some(1000)]);
}
