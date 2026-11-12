//! The DRF and eDRF recorders (task group 8 of
//! `add-recording-and-conformance`; game-recording spec, "Writing a DRF
//! recording", "Writing an eDRF recording", "Re-recording and conversion
//! between formats"; replay-verification spec, "Checkpoint cadence";
//! quirks P12, P14–P16).
//!
//! Most tests drive a recorder's hooks by hand, so that each one exercises
//! exactly one rule; the state handed to the hooks is that of a fresh
//! game, which only feeds the hashes.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]

#[path = "common/inputs.rs"]
mod inputs;

use digger_core::recording::{DrfCursor, DrfStep, Recording, RecordingKind, parse};
use digger_core::replay::{
    DrfRecorder, EdrfRecorder, Outcome, Replay, ReplayOptions, ReplayStep, verify,
};
use digger_core::{
    Controls, DiggerId, Direction, FrameInput, Game, GameConfig, GameMode, GauntletSeconds,
    LevelNumber, Movement, NoopObserver, Observer, PlayerId, SlotId, StartLevel, StateView,
};

use inputs::InputGen;

const S0: SlotId = SlotId::ALL[0];
const S1: SlotId = SlotId::ALL[1];
const P0: PlayerId = PlayerId::ALL[0];

/// The most frames a test game may take.
const MAX_FRAMES: usize = 20_000;

fn mode_config(mode: GameMode) -> GameConfig {
    GameConfig {
        mode,
        ..GameConfig::default()
    }
}

fn gauntlet(seconds: u32) -> GauntletSeconds {
    GauntletSeconds::new(seconds)
}

fn level(n: u16) -> LevelNumber {
    LevelNumber::new(n).expect("a level number")
}

fn record(direction: Option<Direction>, fire: bool) -> Movement {
    Movement { direction, fire }
}

const fn right() -> Controls {
    Controls {
        right: true,
        ..Controls::NONE
    }
}

const fn left() -> Controls {
    Controls {
        left: true,
        ..Controls::NONE
    }
}

/// The DRF body: everything after the 84 header and map lines.
fn drf_body(text: &str) -> String {
    text.split_inclusive('\n').skip(84).collect()
}

/// The lines of an eDRF after its four header lines.
fn edrf_records(text: &str) -> Vec<&str> {
    text.lines().skip(4).collect()
}

/// Runs `ticks` ticks of hooks on `recorder`: each slot is given the
/// controls `controls(tick, slot)` and every slot in `sampled` takes a
/// movement sample.
fn run_ticks(
    recorder: &mut dyn Observer,
    state: StateView<'_>,
    ticks: std::ops::RangeInclusive<u32>,
    controls: impl Fn(u32, SlotId) -> Controls,
    sampled: &[SlotId],
) {
    for tick in ticks {
        for slot in SlotId::ALL {
            let raw = controls(tick, slot);
            recorder.controls(tick, slot, raw);
            if sampled.contains(&slot) {
                recorder.movement(slot, record(None, false));
            }
        }
        recorder.tick_end(tick, state);
    }
}

#[test]
fn drf_writer_mode_lines() {
    let start5 = GameConfig {
        start_level: StartLevel::new(5).expect("a start level"),
        ..GameConfig::default()
    };
    let cases = [
        (mode_config(GameMode::OnePlayer), "1"),
        (mode_config(GameMode::TwoPlayers), "2"),
        (mode_config(GameMode::TwoDiggers), "M2"),
        (
            mode_config(GameMode::Gauntlet {
                time: gauntlet(120),
            }),
            "G120",
        ),
        (
            mode_config(GameMode::GauntletTwoDiggers { time: gauntlet(90) }),
            "M2G90",
        ),
        (start5, "1I5"),
    ];
    for (config, mode_line) in cases {
        let text = DrfRecorder::new(&config).text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            &lines[..4],
            &["DRF", "RS DIG 20261111", mode_line, "20000"],
            "{mode_line}"
        );
        assert_eq!(
            lines.len(),
            84,
            "{mode_line}: 4 header lines and 80 map rows"
        );
        for row in &lines[4..] {
            assert_eq!(row.len(), 15, "{mode_line}: map row {row:?}");
        }
        assert!(text.ends_with('\n') && !text.contains('\r'), "LF line ends");
        // The header reads back as the configuration it was written from.
        let recording = parse(text.as_bytes()).expect("a valid DRF");
        let header = recording.header();
        assert_eq!(header.mode(), config.mode, "{mode_line}");
        assert_eq!(header.start_level(), config.start_level, "{mode_line}");
        assert_eq!(header.bonus_score(), 20000);
        let Recording::Drf(drf) = &recording else {
            panic!("a DRF")
        };
        assert_eq!(drf.maps(), config.levels.layouts(), "{mode_line}");

        // The eDRF header carries the same three lines after its first.
        let mut edrf = EdrfRecorder::new(&config);
        let text = edrf.take_pending();
        assert_eq!(
            text,
            format!("eDRF 2\nRS DIG 20261111\n{mode_line}\n20000\n")
        );
        assert_eq!(edrf.take_pending(), "", "drained");
    }
}

#[test]
fn drf_writer_run_cap_and_wrap() {
    let game = Game::new(GameConfig::default(), 0);
    let state = game.state();

    // 1 × stop, 1000 × right, 2 × left with fire: `sr999rL2`.
    let mut recorder = DrfRecorder::new(&GameConfig::default());
    recorder.round_seed(0, 0x0001_E240);
    recorder.movement(S0, record(None, false));
    for _ in 0..1000 {
        recorder.movement(S0, record(Some(Direction::Right), false));
    }
    for _ in 0..2 {
        recorder.movement(S0, record(Some(Direction::Left), true));
    }
    recorder.round_end(state);
    assert_eq!(drf_body(&recorder.text()), "0001E240\nsr999rL2\nEOL\n");

    // Each run adds 1 plus its count's digits to the column counter; a
    // line break is written once the counter reaches 60.
    let mut recorder = DrfRecorder::new(&GameConfig::default());
    recorder.round_seed(0, -1);
    for i in 0..61 {
        let direction = if i % 2 == 0 {
            Direction::Right
        } else {
            Direction::Up
        };
        recorder.movement(S0, record(Some(direction), false));
    }
    recorder.round_end(state);
    let sixty = "ru".repeat(30);
    assert_eq!(
        drf_body(&recorder.text()),
        format!("FFFFFFFF\n{sixty}\nr\nEOL\n")
    );

    // Runs of 10 count 3 columns: the 20th reaches 60. A line that ends
    // exactly on the wrap gets no second line break before `EOL`.
    let mut recorder = DrfRecorder::new(&GameConfig::default());
    recorder.round_seed(0, 0x00AB_CDEF);
    for i in 0..20 {
        let direction = if i % 2 == 0 {
            Direction::Down
        } else {
            Direction::Left
        };
        for _ in 0..10 {
            recorder.movement(S0, record(Some(direction), false));
        }
    }
    recorder.round_end(state);
    let line = "d10l10".repeat(10);
    assert_eq!(
        drf_body(&recorder.text()),
        format!("00ABCDEF\n{line}\nEOL\n")
    );

    // A run of exactly 999 is written as one, the 1000th starts a new one.
    let mut recorder = DrfRecorder::new(&GameConfig::default());
    recorder.round_seed(0, 0);
    for _ in 0..1998 {
        recorder.movement(S0, record(None, true));
    }
    recorder.round_end(state);
    assert_eq!(drf_body(&recorder.text()), "00000000\nS999S999\nEOL\n");
}

/// Plays a live game under `config` with generated controls (fire only
/// on ticks with a movement sample, quirk Q15), quitting after
/// `quit_after` ticks if given; returns the game.
fn live_game(
    config: &GameConfig,
    seed: u64,
    quit_after: Option<u32>,
    obs: &mut dyn Observer,
) -> Box<Game> {
    let mut game = Game::new(config.clone(), seed);
    let mut generator = InputGen::new(seed);
    for _ in 0..MAX_FRAMES {
        let state = game.state();
        let mut input = generator.controls();
        for (slot, controls) in input.slots.iter_mut().enumerate() {
            controls.fire &= will_sample(state, slot);
        }
        input.quit = quit_after.is_some_and(|ticks| state.ticks() >= ticks);
        if game.step_observed(&input, obs).finished.is_some() {
            return game;
        }
    }
    panic!("the game did not finish in {MAX_FRAMES} frames");
}

/// Whether the digger of control slot `slot` takes a movement sample on
/// the next tick.
fn will_sample(state: StateView<'_>, slot: usize) -> bool {
    let index = if state.mode().alternating() {
        usize::from(state.current_player().number())
    } else {
        slot
    };
    let digger = state.digger(DiggerId::ALL[index]);
    state.diggers_visible() && digger.alive() && digger.bag_time() == 0
}

#[test]
fn drf_writer_quit_appends_eog() {
    let config = GameConfig::default();
    let mut recorder = DrfRecorder::new(&config);
    let game = live_game(&config, 7, Some(300), &mut recorder);
    assert_eq!(game.state().ticks(), 300);
    let text = recorder.text();
    assert!(text.ends_with("\nEOL\nEOG\n"), "{}", drf_body(&text));
    assert_eq!(text.matches("EOG").count(), 1);

    // A game that ends by itself has no `EOG`.
    let mut recorder = DrfRecorder::new(&config);
    let _ = live_game(&config, 7, None, &mut recorder);
    let text = recorder.text();
    assert!(text.ends_with("\nEOL\n"), "{}", drf_body(&text));
    assert!(!text.contains("EOG"));

    // The natural end replays to the same end.
    let report = verify(text.as_bytes(), ReplayOptions::default()).expect("a valid DRF");
    assert_eq!(report.outcome, Outcome::Completed);
    let summary = game_summary(&config, 7);
    assert_eq!(report.ticks, summary.ticks);
    assert_eq!(report.scores[0], summary.scores[0].total);
}

/// The summary of the live game [`live_game`] plays to its end.
fn game_summary(config: &GameConfig, seed: u64) -> digger_core::GameSummary {
    let game = live_game(config, seed, None, &mut NoopObserver);
    game.summary().expect("finished")
}

#[test]
fn drf_writer_exceeds_256k() {
    // A round of 300 000 single-letter runs: well over upstream's buffer.
    const RECORDS: usize = 300_000;
    let game = Game::new(GameConfig::default(), 0);
    let mut recorder = DrfRecorder::new(&GameConfig::default());
    recorder.round_seed(0, 0x1234_5678);
    let records: Vec<Movement> = (0..RECORDS)
        .map(|i| {
            let direction = [Direction::Right, Direction::Left, Direction::Up][i % 3];
            record(Some(direction), i % 7 == 0)
        })
        .collect();
    for &movement in &records {
        recorder.movement(S0, movement);
    }
    recorder.round_end(game.state());
    recorder.add_initials("abc");
    let text = recorder.text();
    assert!(text.len() > 262_144, "{} bytes", text.len());
    assert!(text.ends_with("EOL\n*abc\n"));

    // Every record reads back, in order, so a playback is given exactly
    // what the game produced. (No real game reaches this size in a test's
    // time: one control read writes at most one byte, and three lives or
    // the longest gauntlet last far fewer reads.)
    let Recording::Drf(drf) = parse(text.as_bytes()).expect("a valid DRF") else {
        panic!("a DRF")
    };
    let mut cursor = DrfCursor::new();
    assert_eq!(cursor.next_seed(&drf), Ok(0x1234_5678));
    for (i, &expected) in records.iter().enumerate() {
        let Ok(DrfStep::Record(read)) = cursor.next_record(&drf) else {
            panic!("record {i} missing")
        };
        assert_eq!(Movement::from(read), expected, "record {i}");
    }
    assert_eq!(cursor.end_round(&drf), Ok(()));
    assert!(!cursor.round_follows(&drf), "the initials are a trailer");
}

#[test]
fn edrf_chunk_forms() {
    let game = Game::new(GameConfig::default(), 0);
    let state = game.state();

    // One digger: slot 1 is never read and agrees with anything, so the
    // chunk has no slot list.
    let mut recorder = EdrfRecorder::new(&mode_config(GameMode::OnePlayer));
    let _ = recorder.take_pending();
    run_ticks(&mut recorder, state, 1..=32, |_, _| Controls::NONE, &[S0]);
    run_ticks(
        &mut recorder,
        state,
        33..=64,
        |t, slot| {
            if slot == S0 && t > 40 {
                right()
            } else {
                Controls::NONE
            }
        },
        &[S0],
    );
    assert_eq!(recorder.take_pending(), "I @32\nI @8D24\n");

    // Two diggers: slot 0 right, slot 1 nothing: only slot 0 is listed.
    let mut recorder = EdrfRecorder::new(&mode_config(GameMode::TwoDiggers));
    let _ = recorder.take_pending();
    run_ticks(
        &mut recorder,
        state,
        1..=32,
        |_, slot| if slot == S0 { right() } else { Controls::NONE },
        &[S0, S1],
    );
    assert_eq!(recorder.take_pending(), "I0 D32\n");

    // Both slots active and different: both listed.
    run_ticks(
        &mut recorder,
        state,
        33..=64,
        |_, slot| if slot == S0 { right() } else { left() },
        &[S0, S1],
    );
    assert_eq!(recorder.take_pending(), "I01 D32 H32\n");

    // Only slot 1 active.
    run_ticks(
        &mut recorder,
        state,
        65..=96,
        |t, slot| {
            if slot == S1 && t <= 70 {
                left()
            } else {
                Controls::NONE
            }
        },
        &[S0, S1],
    );
    assert_eq!(recorder.take_pending(), "I1 H6@26\n");

    // Both slots the same on every tick: no slot list.
    run_ticks(&mut recorder, state, 97..=128, |_, _| right(), &[S0, S1]);
    assert_eq!(recorder.take_pending(), "I D32\n");

    // A partial chunk is written before any other record.
    run_ticks(&mut recorder, state, 129..=133, |_, _| right(), &[S0, S1]);
    recorder.round_seed(1, 0x0BAD_F00D);
    assert_eq!(recorder.take_pending(), "I D5\nR 0BADF00D\n");

    // Each written chunk reads back as written.
    let text = "eDRF 2\nRS DIG 20261111\nM2\n20000\nI0 D32\nI01 D32 H32\nI1 H6@26\nI D32\n";
    let Recording::Edrf(edrf) = parse(text.as_bytes()).expect("a valid eDRF") else {
        panic!("an eDRF")
    };
    assert_eq!(edrf.controls(S0).remaining(), 128);
    assert_eq!(edrf.controls(S1).remaining(), 128);
}

#[test]
fn edrf_map_reuse_written() {
    let config = GameConfig::default();
    let mut recorder = EdrfRecorder::new(&config);
    let _ = recorder.take_pending();
    for n in 1..=9 {
        let level = level(n);
        recorder.level_plan(P0, level, config.levels.layout(level.plan()));
    }
    let text = recorder.take_pending();
    let lines: Vec<&str> = text.lines().collect();
    // Eight new maps of 11 lines each, then level 9 on the sixth.
    assert_eq!(lines.len(), 8 * 11 + 1, "{text}");
    assert_eq!(lines[0], "L 1");
    assert_eq!(lines[77], "L 8");
    assert_eq!(lines.last(), Some(&"L 9 6"));
    for (i, line) in lines.iter().enumerate() {
        if i % 11 != 0 && i < 88 {
            assert_eq!(line.len(), 16, "map row {line:?}");
            assert!(line.starts_with(' '), "map row {line:?}");
        }
    }
    // It reads back to the configuration's maps.
    let full = format!("eDRF 2\nRS DIG 20261111\n1\n20000\n{text}");
    let Recording::Edrf(edrf) = parse(full.as_bytes()).expect("a valid eDRF") else {
        panic!("an eDRF")
    };
    let levels = edrf.levels();
    assert_eq!(levels.len(), 9);
    for (record, n) in levels.iter().zip(1..) {
        assert_eq!(record.level, u32::from(n));
        assert_eq!(&record.layout, config.levels.layout(level(n).plan()));
    }

    // With two diggers the maps are written as played: plans 3 and 4 with
    // their start cells open (quirk P6, Q07).
    let config = mode_config(GameMode::TwoDiggers);
    let mut recorder = EdrfRecorder::new(&config);
    let _ = recorder.take_pending();
    let third = level(3);
    recorder.level_plan(P0, third, config.levels.layout(third.plan()));
    let text = recorder.take_pending();
    let bottom = text.lines().nth(10).expect("row 9");
    let played = config.levels.layout_as_played(third.plan(), true);
    assert_eq!(played.rows()[9][6], b'H');
    assert_eq!(played.rows()[9][8], b'H');
    assert_eq!(&bottom.as_bytes()[1..], &played.rows()[9]);
    assert_ne!(&played, config.levels.layout(third.plan()), "patched");
}

#[test]
fn checkpoint_then_round_end_order() {
    let game = Game::new(GameConfig::default(), 0);
    let state = game.state();
    let mut recorder = EdrfRecorder::new(&GameConfig::default());
    let _ = recorder.take_pending();
    run_ticks(&mut recorder, state, 1..=255, |_, _| Controls::NONE, &[S0]);
    let before = recorder.take_pending();
    assert_eq!(before.lines().count(), 7, "seven full chunks: {before}");
    assert!(!before.contains('C'));
    run_ticks(
        &mut recorder,
        state,
        256..=256,
        |_, _| Controls::NONE,
        &[S0],
    );
    // The round ends on tick 256: its controls, `C`, then `E`.
    recorder.round_end(state);
    let text = recorder.take_pending();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    assert_eq!(lines[0], "I @32");
    assert!(lines[1].starts_with("C ") && lines[1].len() == 10, "{text}");
    assert!(lines[2].starts_with("E ") && lines[2].len() == 10, "{text}");
    // No `C` on other multiples of 32.
    run_ticks(
        &mut recorder,
        state,
        257..=512,
        |_, _| Controls::NONE,
        &[S0],
    );
    let text = recorder.take_pending();
    assert_eq!(
        text.matches("\nC ").count() + usize::from(text.starts_with("C ")),
        1
    );
    assert!(text.ends_with(&format!(
        "I @32\nC {}\n",
        digger_core::state_hash::format_hash(digger_core::state_hash::hash(state))
    )));

    // In a live game every `C` follows the chunk of a multiple of 256 and
    // a round end there writes `C` then `E`.
    let config = GameConfig::default();
    let mut recorder = EdrfRecorder::new(&config);
    let game = live_game(&config, 5, None, &mut recorder);
    let text = recorder.take_pending();
    let mut ticks: u32 = 0;
    for line in edrf_records(&text) {
        if let Some(runs) = line.strip_prefix('I') {
            ticks += input_ticks(runs);
        } else if line.starts_with("C ") {
            assert_eq!(ticks % 256, 0, "C after tick {ticks}");
        }
    }
    assert_eq!(ticks, game.state().ticks());
    let report = verify(text.as_bytes(), ReplayOptions::default()).expect("a valid eDRF");
    assert_eq!(report.outcome, Outcome::Verified);
}

/// The number of ticks an `I` record (without its `I`) covers.
fn input_ticks(record: &str) -> u32 {
    let list = record.split(' ').nth(1).expect("a run list");
    let mut ticks = 0;
    let mut chars = list.chars().peekable();
    while chars.next().is_some() {
        let mut count = String::new();
        while let Some(c) = chars.peek().filter(|c| c.is_ascii_digit()) {
            count.push(*c);
            chars.next();
        }
        ticks += count.parse::<u32>().unwrap_or(1);
    }
    ticks
}

#[test]
fn one_player_game_has_single_level_record() {
    let start = |config: &GameConfig| {
        let mut recorder = EdrfRecorder::new(config);
        let mut game = Game::new(config.clone(), 1);
        // Step until the first tick: the game has set up its levels.
        while game.state().ticks() == 0 {
            game.step_observed(&FrameInput::NONE, &mut recorder);
        }
        recorder.take_pending()
    };
    let text = start(&GameConfig::default());
    let levels: Vec<&str> = edrf_records(&text)
        .into_iter()
        .filter(|l| l.starts_with("L "))
        .collect();
    assert_eq!(levels, ["L 1"], "{text}");
    for mode in [
        GameMode::TwoDiggers,
        GameMode::Gauntlet { time: gauntlet(60) },
    ] {
        let text = start(&mode_config(mode));
        let count = edrf_records(&text)
            .iter()
            .filter(|l| l.starts_with("L "))
            .count();
        assert_eq!(count, 1, "{mode:?}");
    }
    // Two alternating players: one level record each, the second reusing
    // the first's map.
    let text = start(&mode_config(GameMode::TwoPlayers));
    let levels: Vec<&str> = edrf_records(&text)
        .into_iter()
        .filter(|l| l.starts_with("L "))
        .collect();
    assert_eq!(levels, ["L 1", "L 1 1"], "{text}");
}

#[test]
fn edrf_prefix_at_chunk_boundary_is_playable() {
    let config = GameConfig::default();
    let mut recorder = EdrfRecorder::new(&config);
    let mut game = Game::new(config, 9);
    let mut generator = InputGen::new(9);
    let mut text = String::new();
    while game.state().ticks() < 96 {
        let state = game.state();
        let mut input = generator.controls();
        input.slots[0].fire &= will_sample(state, 0);
        game.step_observed(&input, &mut recorder);
        text.push_str(&recorder.take_pending());
    }
    assert_eq!(
        edrf_records(&text).last().map(|l| l.starts_with('I')),
        Some(true)
    );
    let recording = parse(text.as_bytes()).expect("a valid eDRF");
    let mut replay = Replay::new(&recording, ReplayOptions::default()).expect("playable");
    while let ReplayStep::Running(_) = replay.step(&FrameInput::NONE, &mut NoopObserver) {}
    let report = replay.finish();
    assert_eq!(report.outcome, Outcome::Truncated);
    assert_eq!(report.ticks, 96);
    let scores = game.state().scores();
    assert_eq!(report.scores[0], scores.total(digger_core::ScoreId::ALL[0]));
}

#[test]
fn conversion_bits_from_drf_records() {
    // One digger: a scripted round of control reads, then the end.
    let mut text = String::from("DRF\nAJ DOS 19990506\n1\n20000\n");
    let config = GameConfig::default();
    for map in config.levels.layouts() {
        for row in map.rows() {
            text.push_str(std::str::from_utf8(row).expect("ASCII"));
            text.push('\n');
        }
    }
    text.push_str("00000000\nr5U3l2sd4R\nEOL\n");
    let recording = parse(text.as_bytes()).expect("a valid DRF");
    let mut replay =
        Replay::recording_as(&recording, ReplayOptions::default(), RecordingKind::Edrf)
            .expect("playable");
    while let ReplayStep::Running(_) = replay.step(&FrameInput::NONE, &mut NoopObserver) {}
    let (report, converted) = replay.finish_with_recording();
    let converted = converted.expect("a recorder");
    // right 4, up 1, left 8, stop 0, down 2, fire 16; slot 1 unread.
    let Recording::Edrf(edrf) = parse(converted.as_bytes()).expect("a valid eDRF") else {
        panic!("an eDRF")
    };
    let values: Vec<u8> = {
        let mut runs = edrf.controls(S0).clone();
        std::iter::from_fn(|| {
            runs.next_value()
                .map(digger_core::recording::ControlValue::bits)
        })
        .collect()
    };
    let expected = [4, 4, 4, 4, 4, 17, 17, 17, 8, 8, 0, 2, 2, 2, 2, 20];
    assert_eq!(&values[..16], &expected, "{converted}");
    // The DRF ended at the 17th control read: that tick has no record.
    assert_eq!(values.len(), 17);
    assert_eq!(values[16], 0);
    assert_eq!(report.ticks, 17);
    assert!(converted.contains("\nQ 0\nZ 17 "), "{converted}");
    // The converted recording replays to the same result.
    let again = verify(converted.as_bytes(), ReplayOptions::default()).expect("valid");
    assert_eq!(again.outcome, Outcome::Verified);
    assert_eq!(
        (again.scores, again.level, again.ticks),
        (report.scores, report.level, report.ticks)
    );

    // Hand-driven: a tick without a control read records 0 for a slot in
    // play, and two diggers' records go to their own slots.
    let game = Game::new(GameConfig::default(), 0);
    let state = game.state();
    let mut recorder = EdrfRecorder::for_drf_playback(&mode_config(GameMode::TwoDiggers));
    let _ = recorder.take_pending();
    for tick in 1..=32 {
        for slot in SlotId::ALL {
            // The raw controls of a DRF playback are all off.
            recorder.controls(tick, slot, Controls::NONE);
            if tick <= 16 || slot == S1 {
                let direction = if slot == S0 {
                    Direction::Right
                } else {
                    Direction::Left
                };
                recorder.movement(slot, record(Some(direction), tick == 3));
            }
        }
        recorder.tick_end(tick, state);
    }
    assert_eq!(recorder.take_pending(), "I01 D2TD13@16 H2XH29\n");
}
