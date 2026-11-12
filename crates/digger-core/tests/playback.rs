//! Playing back DRF and eDRF recordings through [`Replay`] (task group 6
//! of `add-recording-and-conformance`; game-recording spec, "DRF playback
//! supplies one record per control read", "DRF records replace the
//! resolved direction and the fire input", "DRF round boundaries and end
//! of data", "eDRF playback supplies raw controls per slot per tick",
//! "eDRF inline level maps", "Two-digger start-cell patch applies to DRF,
//! not eDRF", "End of an eDRF recording"; quirks P1–P6).
//!
//! The recordings here are written out in the tests, so that each one
//! exercises exactly one rule.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;
use std::path::Path;

use digger_core::recording::{OutOfStep, Recording, parse};
use digger_core::replay::{Failure, PlaybackFailure, Replay, ReplayOptions, ReplayStep};
use digger_core::{
    Cell, CellPos, Controls, DiggerId, Direction, FinishReason, FrameInput, FrameKind, GameSummary,
    LevelLayout, LevelNumber, Movement, NoopObserver, Observer, PlayerId, SlotId, StateView,
};

const S0: SlotId = SlotId::ALL[0];
const S1: SlotId = SlotId::ALL[1];
const FIRST: DiggerId = DiggerId::ALL[0];

/// The most frames a test playback may take.
const MAX_FRAMES: usize = 20_000;

/// An eDRF with this mode line and body.
fn edrf(mode: &str, body: &str) -> Recording {
    parse(format!("eDRF 2\nRS DIG 20261111\n{mode}\n20000\n{body}").as_bytes())
        .expect("a valid eDRF")
}

/// Ten map rows of earth with `cells` (column, row, character) set.
fn rows_with(cells: &[(usize, usize, u8)]) -> [String; 10] {
    let mut rows: [Vec<u8>; 10] = std::array::from_fn(|_| vec![b' '; 15]);
    for &(h, v, c) in cells {
        rows[v][h] = c;
    }
    rows.map(|row| String::from_utf8(row).expect("ASCII"))
}

/// A DRF with this mode line, plan 1 and plan 3 given (the other plans
/// earth), and this body.
fn drf(mode: &str, plan1: &[String; 10], plan3: &[String; 10], body: &str) -> Recording {
    let mut text = format!("DRF\nAJ DOS 19990506\n{mode}\n20000\n");
    for plan in 1..=8 {
        for row in 0..10 {
            match plan {
                1 => text.push_str(&plan1[row]),
                3 => text.push_str(&plan3[row]),
                _ => {}
            }
            text.push('\n');
        }
    }
    text.push_str(body);
    parse(text.as_bytes()).expect("a valid DRF")
}

/// An eDRF `L` record with an inline map.
fn level_record(level: u16, rows: &[String; 10]) -> String {
    let mut text = format!("L {level}\n");
    for row in rows {
        text.push(' ');
        text.push_str(row);
        text.push('\n');
    }
    text
}

/// What one tick showed.
#[derive(Debug, Clone, Default)]
struct TickLog {
    /// The raw controls used per slot.
    controls: Vec<(SlotId, Controls)>,
    /// The movement samples used, in call order.
    movements: Vec<(SlotId, Movement)>,
    /// Digger 1's position and whether its fireball is out, at tick end.
    position: (i32, i32),
    firing: bool,
    /// Cells (6,9), (7,9) and (8,9) at tick end.
    start_cells: [Cell; 3],
}

/// Logs every hook by tick (index 0 is tick 1).
#[derive(Debug, Default)]
struct Log {
    ticks: Vec<TickLog>,
    current: TickLog,
    levels: Vec<(PlayerId, LevelNumber, LevelLayout)>,
    seeds: Vec<i32>,
    round_ends: Vec<u32>,
}

impl Log {
    /// The ticks with a movement sample of `slot`.
    fn reads(&self, slot: SlotId) -> Vec<(u32, Movement)> {
        let mut reads = Vec::new();
        for (n, tick) in (1..).zip(&self.ticks) {
            for &(s, movement) in &tick.movements {
                if s == slot {
                    reads.push((n, movement));
                }
            }
        }
        reads
    }

    fn tick(&self, tick: u32) -> &TickLog {
        &self.ticks[usize::try_from(tick - 1).expect("small")]
    }
}

fn cell(h: u8, v: u8) -> CellPos {
    CellPos::new(h, v).expect("inside the grid")
}

impl Observer for Log {
    fn round_seed(&mut self, _round: u32, derived: i32) -> i32 {
        self.seeds.push(derived);
        derived
    }

    fn level_plan(
        &mut self,
        player: PlayerId,
        level: LevelNumber,
        layout: &LevelLayout,
    ) -> Option<LevelLayout> {
        self.levels.push((player, level, *layout));
        None
    }

    fn controls(&mut self, _tick: u32, slot: SlotId, raw: Controls) -> Controls {
        self.current.controls.push((slot, raw));
        raw
    }

    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement {
        self.current.movements.push((slot, sample));
        sample
    }

    fn tick_end(&mut self, tick: u32, state: StateView<'_>) {
        let mut done = std::mem::take(&mut self.current);
        let digger = state.digger(FIRST);
        done.position = (digger.position().x, digger.position().y);
        done.firing = !digger.not_firing();
        done.start_cells = [6, 7, 8].map(|h| state.field().cell(cell(h, 9)));
        self.ticks.push(done);
        assert_eq!(self.ticks.len(), usize::try_from(tick).expect("small"));
    }

    fn round_end(&mut self, state: StateView<'_>) {
        self.round_ends.push(state.ticks());
    }
}

/// Plays `recording` to its end with `input` on every frame, logging.
fn play_with(recording: &Recording, input: FrameInput) -> (Replay, GameSummary, Log) {
    let mut replay = Replay::new(recording, ReplayOptions::default()).expect("playable");
    let mut log = Log::default();
    for _ in 0..MAX_FRAMES {
        if let ReplayStep::Finished(summary) = replay.step(&input, &mut log) {
            return (replay, summary, log);
        }
    }
    panic!("the playback did not finish in {MAX_FRAMES} frames");
}

fn play(recording: &Recording) -> (Replay, GameSummary, Log) {
    play_with(recording, FrameInput::NONE)
}

const RIGHT: Controls = Controls {
    right: true,
    ..Controls::NONE
};

fn movement(direction: Option<Direction>, fire: bool) -> Movement {
    Movement { direction, fire }
}

/// Every control held on both slots, with pause and quit off.
fn mash() -> FrameInput {
    let all = Controls {
        up: true,
        down: true,
        left: true,
        right: true,
        fire: true,
    };
    FrameInput {
        slots: [all; 2],
        ..FrameInput::NONE
    }
}

// --- eDRF -----------------------------------------------------------------

#[test]
fn edrf_controls_pass_through_resolver() {
    // Slot 0: nothing for 9 ticks, then right, right+up, right, nothing.
    let recording = edrf("1", "R 00000000\nI @9DED@20\n");
    let (_, summary, log) = play_with(&recording, mash());
    assert_eq!(summary.ticks, 32);
    let used: Vec<Controls> = (10..=12)
        .map(|t| {
            let tick = log.tick(t);
            tick.controls
                .iter()
                .find(|(s, _)| *s == S0)
                .expect("slot 0 offered")
                .1
        })
        .collect();
    let up_right = Controls { up: true, ..RIGHT };
    assert_eq!(used, [RIGHT, up_right, RIGHT], "the recorded raw values");
    // The resolver turns them into right, up (last pressed wins), right;
    // the caller's mashed controls have no effect.
    let resolved: Vec<Option<Direction>> = (10..=12)
        .map(|t| log.tick(t).movements[0].1.direction)
        .collect();
    assert_eq!(
        resolved,
        [
            Some(Direction::Right),
            Some(Direction::Up),
            Some(Direction::Right)
        ]
    );
    assert!(log.tick(9).movements[0].1.direction.is_none());
    assert!(log.ticks.iter().all(|t| !t.firing), "no fire was recorded");
}

#[test]
fn edrf_unread_slot_consumes_value() {
    // One digger; slot 1 is never read but its values are used up, one
    // per tick, and offered to the observer.
    let recording = edrf("1", "R 00000000\nI01 @10 DHDHDHDHDH\n");
    let (replay, summary, log) = play(&recording);
    assert_eq!(summary.ticks, 10, "both slots ran out after tick 10");
    assert_eq!(summary.reason, FinishReason::Quit);
    assert!(replay.recording_exhausted());
    let left = Controls {
        left: true,
        ..Controls::NONE
    };
    for (n, tick) in (1..).zip(&log.ticks) {
        let slot1: Vec<Controls> = tick
            .controls
            .iter()
            .filter(|(s, _)| *s == S1)
            .map(|(_, c)| *c)
            .collect();
        let expected = if n % 2 == 1 { RIGHT } else { left };
        assert_eq!(slot1, [expected], "tick {n}");
        assert!(tick.movements.iter().all(|(s, _)| *s == S0));
    }

    // A slot 1 longer than slot 0 keeps the game going until it too has
    // run out: slot 1's values are used up on every tick.
    let recording = edrf("1", "R 00000000\nI01 @5 @12\n");
    let (_, summary, _) = play(&recording);
    assert_eq!(summary.ticks, 12);
}

#[test]
fn edrf_level_number_mismatch() {
    let map = rows_with(&[(0, 0, b'C')]);
    let recording = edrf(
        "I3",
        &format!("{}R 00000000\nI @40\n", level_record(4, &map)),
    );
    let (replay, summary, log) = play(&recording);
    let failure = replay.failure().expect("a level mismatch");
    let three = LevelNumber::new(3).expect("a level");
    assert_eq!(
        *failure,
        Failure::Playback(PlaybackFailure::LevelMismatch {
            started: three,
            recorded: 4
        })
    );
    assert_eq!(
        failure.to_string(),
        "eDRF: level 3 starts, recorded level 4"
    );
    // The recorded map is used all the same, and the playback stops at
    // the first failure, before any tick.
    assert_eq!(log.levels.len(), 1);
    let (_, level, used) = log.levels[0];
    assert_eq!(level, three);
    assert_eq!(used.rows()[0][0], b'C');
    assert_eq!(summary.ticks, 0);
    assert_eq!(summary.reason, FinishReason::Quit);

    // The matching level plays on.
    let recording = edrf(
        "I3",
        &format!("{}R 00000000\nI @40\n", level_record(3, &map)),
    );
    let (replay, summary, _) = play(&recording);
    assert!(replay.failure().is_none());
    assert_eq!(summary.ticks, 40);
}

#[test]
fn edrf_quits_when_exhausted() {
    let recording = edrf("1", "R 00000000\nI @20\n");
    let mut replay = Replay::new(&recording, ReplayOptions::default()).expect("playable");
    let mut frames = 0;
    let summary = loop {
        frames += 1;
        assert!(frames < MAX_FRAMES);
        let ticks = replay.game().state().ticks();
        match replay.step(&FrameInput::NONE, &mut NoopObserver) {
            ReplayStep::Running(_) => {}
            ReplayStep::Finished(summary) => {
                // The quit comes on the frame after the last value, and
                // runs no tick.
                assert_eq!(ticks, 20);
                break summary;
            }
        }
    };
    assert_eq!(summary.ticks, 20);
    assert_eq!(summary.reason, FinishReason::Quit);
    assert!(replay.recording_exhausted());
    assert!(replay.failure().is_none());

    // A recording without controls quits on its first frame.
    let recording = edrf("1", "R 00000000\n");
    let (_, summary, log) = play(&recording);
    assert_eq!(summary.ticks, 0);
    assert!(log.ticks.is_empty());
}

#[test]
fn edrf_seeds_come_from_the_recording() {
    // Game-recording spec, "Seeds come from the recording": the first
    // round's seed is the recorded one (the log sees the value used).
    let recording = edrf("1", "R 1A2B3C4D\nI @4\n");
    let (_, _, log) = play(&recording);
    assert_eq!(log.seeds, [0x1A2B_3C4D]);
}

// --- Two-digger start cells -----------------------------------------------

#[test]
fn two_digger_patch_drf_only() {
    let earth = rows_with(&[]);
    let emerald = rows_with(&[(0, 0, b'C')]);
    let tunnel = Cell::from_layout_char(b'H');
    let untouched = Cell::from_layout_char(b' ');

    // DRF: plan 3 has earth at (6,9) and (8,9); both open as tunnel.
    let recording = drf("M2I3", &earth, &emerald, "00000000\ns4\nEOL\n");
    let (_, summary, log) = play(&recording);
    assert!(summary.ticks >= 1);
    let [six, seven, eight] = log.tick(1).start_cells;
    assert_eq!(six, tunnel, "DRF: (6,9) opened");
    assert_eq!(eight, tunnel, "DRF: (8,9) opened");
    assert_eq!(seven, untouched, "DRF: (7,9) is not a start cell");

    // eDRF: the recorded map is used exactly as recorded.
    let body = format!("{}R 00000000\nI @4\n", level_record(3, &emerald));
    let recording = edrf("M2I3", &body);
    let (replay, summary, log) = play(&recording);
    assert!(replay.failure().is_none());
    assert_eq!(summary.ticks, 4);
    let [six, seven, eight] = log.tick(1).start_cells;
    assert_eq!(six, untouched, "eDRF: (6,9) stays earth");
    assert_eq!(eight, untouched, "eDRF: (8,9) stays earth");
    assert_eq!(seven, untouched);
}

// --- DRF --------------------------------------------------------------------

/// Plan 1 with a bag in the bottom-right cell, which the digger starting
/// at (7,9) and walking right pushes against the wall, and an emerald far
/// away so that the level does not end.
fn bag_at_the_wall() -> [String; 10] {
    rows_with(&[(14, 9, b'B'), (0, 0, b'C')])
}

#[test]
fn drf_record_consumed_only_on_control_read() {
    let plan = bag_at_the_wall();
    let recording = drf("1", &plan, &plan, "00000000\nr60\nEOL\n");
    let (replay, summary, log) = play(&recording);
    assert!(replay.failure().is_none());
    let reads = log.reads(S0);
    // 60 records, one per control read, then the read that finds `EOL`.
    assert_eq!(reads.len(), 61);
    let (last_tick, last) = reads[60];
    assert!(
        reads[..60]
            .iter()
            .all(|(_, m)| *m == movement(Some(Direction::Right), false))
    );
    assert_eq!(last, Movement::default());
    assert_eq!(summary.ticks, last_tick);
    // Some ticks have no control read (the pause after each blocked push)
    // and consume nothing; no tick has more than one.
    assert!(log.ticks.iter().all(|t| t.movements.len() <= 1));
    let paused = log.ticks.iter().filter(|t| t.movements.is_empty()).count();
    assert!(paused > 0, "push pauses take no record");
    assert_eq!(
        usize::try_from(summary.ticks).expect("small"),
        reads.len() + paused
    );
}

#[test]
fn drf_two_diggers_interleave() {
    let plan = rows_with(&[(0, 0, b'C')]);
    let recording = drf("M2", &plan, &plan, "00000000\nRl\nd4\nEOL\n");
    let (replay, summary, log) = play(&recording);
    assert!(replay.failure().is_none());
    let first = log.tick(1);
    assert_eq!(
        first.movements,
        [
            (S0, movement(Some(Direction::Right), true)),
            (S1, movement(Some(Direction::Left), false)),
        ],
        "digger 1 then digger 2 from the one stream"
    );
    let down = movement(Some(Direction::Down), false);
    for t in 2..=3 {
        assert_eq!(log.tick(t).movements, [(S0, down), (S1, down)], "tick {t}");
    }
    // Tick 4: digger 1 finds `EOL`; digger 2 reads nothing more either.
    assert_eq!(
        log.tick(4).movements,
        [(S0, Movement::default()), (S1, Movement::default())]
    );
    assert_eq!(summary.ticks, 4);
}

#[test]
fn drf_end_marker_counts_the_tick() {
    let plan = rows_with(&[(0, 0, b'C')]);
    let recording = drf("1", &plan, &plan, "00000000\nr5\nEOL\nEOG\n");
    let (replay, summary, log) = play(&recording);
    assert!(replay.failure().is_none());
    assert!(replay.recording_exhausted());
    let reads = log.reads(S0);
    assert_eq!(reads.len(), 6);
    let (end_tick, end) = reads[5];
    assert_eq!(end, Movement::default(), "no direction, no fire");
    // The tick that found `EOL` completes and is counted; the digger does
    // not move on it; the game is quit after it.
    assert_eq!(summary.ticks, end_tick);
    assert_eq!(summary.reason, FinishReason::Quit);
    assert_eq!(log.tick(end_tick).position, log.tick(end_tick - 1).position);
    assert_ne!(
        log.tick(end_tick - 1).position,
        log.tick(end_tick - 2).position
    );
    assert_eq!(log.round_ends, [end_tick], "the round ends with the quit");
}

#[test]
fn drf_end_of_data_is_like_eol() {
    // Game-recording spec, "Stream ends without EOL".
    let plan = rows_with(&[(0, 0, b'C')]);
    let recording = drf("1", &plan, &plan, "00000000\nr5");
    let (replay, summary, log) = play(&recording);
    assert!(replay.failure().is_none());
    assert_eq!(log.reads(S0).len(), 6);
    assert_eq!(summary.ticks, log.reads(S0)[5].0);
}

#[test]
fn drf_fire_does_not_linger_over_pause() {
    let plan = bag_at_the_wall();
    // Walk right into the bag until every read is a blocked push followed
    // by a pause, then one read with fire.
    let recording = drf("1", &plan, &plan, "00000000\nr50R\nr20\nEOL\n");
    // The caller holds fire (and everything else) on every frame: live
    // controls have no effect, so a pause tick never fires.
    let (replay, _, log) = play_with(&recording, mash());
    assert!(replay.failure().is_none());
    let reads = log.reads(S0);
    let (n, fire) = reads[50];
    assert_eq!(fire, movement(Some(Direction::Right), true));
    assert!(log.tick(n + 1).movements.is_empty(), "tick N+1 is a pause");
    let first_firing = (1..)
        .zip(&log.ticks)
        .find(|(_, t)| t.firing)
        .map(|(t, _)| t);
    assert_eq!(first_firing, Some(n + 1), "fire takes effect on tick N+1");
}

#[test]
fn drf_out_of_step_at_round_end() {
    // An emerald right of the start cell is the level's only one: eating
    // it ends the round while the run still has repeats to give.
    let plan = rows_with(&[(8, 9, b'C')]);
    let recording = drf("1", &plan, &plan, "00000000\nr50\nEOL\n");
    let (replay, summary, log) = play(&recording);
    let reads = u32::try_from(log.reads(S0).len()).expect("small");
    let Some(Failure::Playback(PlaybackFailure::OutOfStep {
        tick,
        problem: OutOfStep::RepeatsPending { remaining },
    })) = replay.failure()
    else {
        panic!("out of step expected, got {:?}", replay.failure());
    };
    assert_eq!(*remaining, 50 - reads);
    assert_eq!(log.round_ends.first(), Some(tick));
    assert!(
        replay
            .failure()
            .expect("failed")
            .to_string()
            .contains(&format!(
                "out of step with the recording at the round end on tick {tick}"
            ))
    );
    // The playback stops at the first failure.
    assert_eq!(summary.reason, FinishReason::Quit);
    assert_eq!(summary.ticks, *tick);
}

// --- The replay driver ----------------------------------------------------

#[test]
fn replay_steps_one_frame_at_a_time() {
    let recording = edrf("1", "R 00000000\nI @32\nI D18\n");
    let mut replay = Replay::new(&recording, ReplayOptions::default()).expect("playable");
    let mut frames = 0;
    let mut waits = 0;
    let summary = loop {
        frames += 1;
        assert!(frames < MAX_FRAMES);
        let before = replay.game().state().ticks();
        let step = replay.step(&FrameInput::NONE, &mut NoopObserver);
        let after = replay.game().state().ticks();
        match step {
            ReplayStep::Running(FrameKind::Tick) => assert_eq!(after, before + 1),
            ReplayStep::Running(_) => {
                waits += 1;
                assert_eq!(after, before);
            }
            ReplayStep::Finished(summary) => break summary,
        }
    };
    assert!(waits > 0, "the level draw takes frames without a tick");
    assert_eq!(summary.ticks, 50);
    // Once finished, every step reports the same summary and changes
    // nothing.
    for _ in 0..3 {
        assert_eq!(
            replay.step(&FrameInput::NONE, &mut NoopObserver),
            ReplayStep::Finished(summary)
        );
    }
    assert_eq!(replay.game().state().ticks(), 50);

    // The caller's quit stops the playback on that frame.
    let mut replay = Replay::new(&recording, ReplayOptions::default()).expect("playable");
    let quit = FrameInput {
        quit: true,
        ..FrameInput::NONE
    };
    for _ in 0..30 {
        replay.step(&FrameInput::NONE, &mut NoopObserver);
    }
    let ticks = replay.game().state().ticks();
    let ReplayStep::Finished(summary) = replay.step(&quit, &mut NoopObserver) else {
        panic!("quit ends the playback");
    };
    assert_eq!(summary.ticks, ticks);
    assert!(!replay.recording_exhausted());

    // The tick limit is a safety net.
    let options = ReplayOptions {
        max_ticks: 7,
        ..ReplayOptions::default()
    };
    let mut replay = Replay::new(&recording, options).expect("playable");
    let summary = loop {
        if let ReplayStep::Finished(summary) = replay.step(&FrameInput::NONE, &mut NoopObserver) {
            break summary;
        }
    };
    assert_eq!(summary.ticks, 7);
    assert!(replay.tick_limit_reached());
}

#[test]
fn replay_dror0_to_completion() {
    // Plays upstream's `dror0.edrf` to its end without checking hashes:
    // no panic (debug build, overflow checks on), and the tick total.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/recordings/data/dror0.edrf");
    let bytes = fs::read(&path).expect("the corpus file");
    let recording = parse(&bytes).expect("a valid recording");
    let options = ReplayOptions {
        stop_at_first_failure: false,
        ..ReplayOptions::default()
    };
    let mut replay = Replay::new(&recording, options).expect("playable");
    let mut frames = 0_u32;
    let summary = loop {
        frames += 1;
        assert!(frames < 1_000_000, "dror0 did not finish");
        if let ReplayStep::Finished(summary) = replay.step(&FrameInput::NONE, &mut NoopObserver) {
            break summary;
        }
    };
    println!(
        "dror0.edrf: {} ticks, score {}, level {}, {:?}, failure: {:?}",
        summary.ticks,
        summary.scores[0].total,
        summary.level.get(),
        summary.reason,
        replay.failure()
    );
    assert!(summary.ticks > 0);
}
