//! Determinism (game-flow spec, "Determinism", scenarios "Repeated run"
//! and "Headless run"; design D11; task 15.1).
//!
//! One scripted game per mode is run twice with the same configuration,
//! base seed and per-frame inputs, comparing after every frame the step's
//! outcome, a CRC-32 of the picture, the tick trace line and the sound
//! commands of the frame. It is then run once more with the sound queue
//! never drained, and once headless (nothing reads the picture or the
//! sound), which must not change anything the simulation does.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]

#[path = "common/inputs.rs"]
mod inputs;

use std::collections::HashSet;
use std::sync::OnceLock;

use digger_core::{
    FrameInput, FrameKind, Game, GameConfig, GameMode, GameSummary, GauntletSeconds, SoundCommand,
    StepOutcome,
};

use inputs::InputGen;

/// The most frames a scripted game runs. Every scripted game finishes
/// well before (the assertions say so); the frames after the finish
/// show that a finished game stays as it is.
const MAX_FRAMES: usize = 4000;

/// Frames run after the game has finished.
const AFTER_FINISH: usize = 3;

/// The five modes, each with its own base seed.
fn modes() -> [(GameMode, u64); 5] {
    let time = GauntletSeconds::new(60);
    [
        (GameMode::OnePlayer, 1),
        (GameMode::TwoPlayers, 2),
        (GameMode::TwoDiggers, 3),
        (GameMode::Gauntlet { time }, 4),
        (GameMode::GauntletTwoDiggers { time }, 5),
    ]
}

/// The scripted controls of a game.
fn script(seed: u64) -> Vec<FrameInput> {
    let mut generator = InputGen::new(seed);
    (0..MAX_FRAMES).map(|_| generator.controls()).collect()
}

/// Frame from which the script asks for a pause.
const PAUSE_FROM: usize = 300;
/// Frames the pause lasts.
const PAUSE_FRAMES: usize = 60;

/// The script's one pause: requested on the first tick frame from
/// [`PAUSE_FROM`] on (again on the next one if the request fell on a
/// wind-down tick, which ignores it), resumed [`PAUSE_FRAMES`] later.
/// It follows the game, which is deterministic, so every run of the same
/// game pauses on the same frames.
#[derive(Debug, Clone, Copy)]
enum Pauser {
    Waiting,
    Requested,
    Checking,
    Paused { since: usize },
    Done,
}

impl Pauser {
    /// Whether frame `frame` carries a pause request, given the outcome of
    /// the frame before.
    fn request(&mut self, frame: usize, last: Option<FrameKind>) -> bool {
        match *self {
            Self::Waiting if frame >= PAUSE_FROM && last == Some(FrameKind::Tick) => {
                *self = Self::Requested;
                true
            }
            // The request's own frame is still a tick; the next shows
            // whether the pause began.
            Self::Requested => {
                *self = Self::Checking;
                false
            }
            Self::Checking => {
                *self = if last == Some(FrameKind::Paused) {
                    Self::Paused { since: frame - 1 }
                } else {
                    Self::Waiting
                };
                self.request(frame, last)
            }
            Self::Paused { since } if frame == since + PAUSE_FRAMES => {
                *self = Self::Done;
                true
            }
            _ => false,
        }
    }
}

/// What one frame produced.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Frame {
    outcome: StepOutcome,
    crc: Option<u32>,
    trace: String,
    sounds: Vec<SoundCommand>,
}

/// A whole scripted game.
#[derive(Debug)]
struct Played {
    frames: Vec<Frame>,
    summary: Option<GameSummary>,
    /// The CRC-32 of the last picture, read once the game is over.
    final_crc: u32,
    dropped: u64,
    /// What was left in the queue at the end, when it was never drained.
    left_in_queue: usize,
}

/// How a run looks at the game after each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Watch {
    /// Reads the picture and drains the sound, like a frontend.
    Full,
    /// Reads the picture but never drains the sound.
    NeverDrained,
    /// Reads neither the picture nor the sound.
    Headless,
}

fn trace(game: &Game) -> String {
    let mut line = String::new();
    game.state()
        .trace_line(&mut line)
        .unwrap_or_else(|_| unreachable!("writing to a String cannot fail"));
    line
}

/// Plays `mode` from `seed` on its script, recording every frame.
fn play(mode: GameMode, seed: u64, watch: Watch) -> Played {
    let config = GameConfig {
        mode,
        ..GameConfig::default()
    };
    let mut game = Game::new(config, seed);
    let mut frames = Vec::new();
    let mut after_finish = 0;
    let mut pauser = Pauser::Waiting;
    let mut last = None;
    for (frame, mut input) in script(seed).into_iter().enumerate() {
        input.pause = pauser.request(frame, last);
        let outcome = game.step(&input);
        last = Some(outcome.frame);
        let crc = (watch != Watch::Headless).then(|| game.framebuffer().crc32());
        let sounds = if watch == Watch::Full {
            game.drain_sound().collect()
        } else {
            Vec::new()
        };
        frames.push(Frame {
            outcome,
            crc,
            trace: trace(&game),
            sounds,
        });
        if outcome.finished.is_some() {
            after_finish += 1;
            if after_finish > AFTER_FINISH {
                break;
            }
        }
    }
    let dropped = game.dropped_sound_commands();
    let left_in_queue = game.drain_sound().count();
    Played {
        frames,
        summary: game.summary(),
        final_crc: game.framebuffer().crc32(),
        dropped,
        left_in_queue,
    }
}

/// The reference game of mode number `index` of [`modes`], played once
/// and shared by the tests.
fn reference(index: usize) -> &'static Played {
    static REFERENCE: [OnceLock<Played>; 5] = [const { OnceLock::new() }; 5];
    let (mode, seed) = *modes().get(index).expect("a mode number");
    REFERENCE
        .get(index)
        .expect("a mode number")
        .get_or_init(|| play(mode, seed, Watch::Full))
}

/// Runs `check` for every mode, each on its own thread, with the mode,
/// its seed and its reference game; returns what each returned.
fn for_each_mode<T: Send>(check: impl Fn(GameMode, u64, &Played) -> T + Sync) -> Vec<T> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = modes()
            .into_iter()
            .enumerate()
            .map(|(index, (mode, seed))| {
                let check = &check;
                scope.spawn(move || check(mode, seed, reference(index)))
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect()
    })
}

/// Asserts that `a` and `b` agree on every frame in the parts `compare`
/// selects, naming the first frame that differs.
fn assert_same(name: &str, a: &[Frame], b: &[Frame], compare: impl Fn(&Frame, &Frame) -> bool) {
    assert_eq!(a.len(), b.len(), "{name}: frame counts differ");
    for (index, (x, y)) in a.iter().zip(b).enumerate() {
        assert!(
            compare(x, y),
            "{name}: frame {index} differs:\n  {x:?}\n  {y:?}"
        );
    }
}

#[test]
fn scripted_games_play_to_the_end() {
    for_each_mode(|mode, _, played| {
        let summary = played
            .summary
            .unwrap_or_else(|| panic!("{mode:?}: did not finish in {MAX_FRAMES} frames"));
        assert!(
            summary.ticks > 500,
            "{mode:?}: only {} ticks",
            summary.ticks
        );
        assert!(played.frames.iter().any(|f| !f.sounds.is_empty()));
        assert_eq!(
            played.dropped, 0,
            "{mode:?}: a drained queue dropped commands"
        );
        let crcs: HashSet<_> = played.frames.iter().map(|f| f.crc).collect();
        assert!(crcs.len() > 100, "{mode:?}: the picture hardly changed");
        let paused = played
            .frames
            .iter()
            .filter(|f| f.outcome.frame == FrameKind::Paused)
            .count();
        // The frame that resumes is a paused frame too.
        assert_eq!(paused, PAUSE_FRAMES + 1, "{mode:?}: the script's pause");
        // A finished game stays as it is.
        let last = played.frames.last().expect("frames were played");
        let tail = played.frames.iter().rev().take(AFTER_FINISH + 1);
        assert!(
            tail.into_iter()
                .all(|f| f.crc == last.crc && f.trace == last.trace)
        );
    });
}

#[test]
fn repeated_run_is_identical_in_every_mode() {
    for_each_mode(|mode, seed, first| {
        let second = play(mode, seed, Watch::Full);
        assert_same(
            &format!("{mode:?} repeated"),
            &first.frames,
            &second.frames,
            |x, y| x == y,
        );
        assert_eq!(first.summary, second.summary);
        assert_eq!(first.final_crc, second.final_crc);
    });
}

#[test]
fn undrained_sound_queue_changes_nothing() {
    let overflowed = for_each_mode(|mode, seed, drained| {
        let undrained = play(mode, seed, Watch::NeverDrained);
        assert_same(
            &format!("{mode:?} undrained"),
            &drained.frames,
            &undrained.frames,
            |x, y| x.outcome == y.outcome && x.crc == y.crc && x.trace == y.trace,
        );
        assert_eq!(drained.summary, undrained.summary);
        // The queue keeps the newest commands and counts the rest.
        let produced: usize = drained.frames.iter().map(|f| f.sounds.len()).sum();
        let produced = u64::try_from(produced).unwrap_or(u64::MAX);
        let kept = u64::try_from(undrained.left_in_queue).unwrap_or(u64::MAX);
        assert_eq!(kept + undrained.dropped, produced, "{mode:?}");
        assert!(kept <= 64);
        undrained.dropped > 0
    });
    // At least one game overflowed the queue, without a panic in this
    // debug build (design D8).
    assert!(overflowed.contains(&true));
}

#[test]
fn headless_run_has_the_same_states() {
    for_each_mode(|mode, seed, shown| {
        let headless = play(mode, seed, Watch::Headless);
        assert_same(
            &format!("{mode:?} headless"),
            &shown.frames,
            &headless.frames,
            |x, y| x.outcome == y.outcome && x.trace == y.trace,
        );
        assert_eq!(shown.summary, headless.summary);
        // The picture nobody looked at is the same picture.
        assert_eq!(shown.final_crc, headless.final_crc, "{mode:?}");
    });
}
