//! The sound engine against the simulation (design D9 and D12, item 6;
//! tasks 9.4 and 9.5).
//!
//! Two upstream recordings from `tests/recordings/data/` are the scripted
//! games: `muu27700.edrf` (one player: four levels completed, deaths, bonus
//! mode, the battle tune and an extra life) and `2test01.edrf` (two
//! diggers). Each is played with a pause inserted, which a playback allows
//! without leaving the recording's ticks.
//!
//! - `simulation_unaffected_by_sound` plays each game twice, once sending
//!   every frame's commands to a [`ManualDriver`] that renders one frame of
//!   samples (with the frontend's switches toggled on the way) and once to
//!   a [`NullDriver`], and compares the state hash after every frame, the
//!   frame counts and the playback reports.
//! - `emitted_commands_cover_the_boundary` checks the order and content of
//!   the commands the simulation emits in those games.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]

use std::path::Path;

use digger_core::replay::{Outcome, Replay, ReplayOptions, ReplayReport, ReplayStep};
use digger_core::{DiggerId, FrameInput, FrameKind, GameMode, NoopObserver, state_hash};
use digger_sound::{
    ManualDriver, NullDriver, SoundCommand, SoundConfig, SoundControl, SoundSink, Tempo, Tune,
    TuneTables,
};

/// The one-player game.
const ONE_PLAYER: &str = "muu27700.edrf";
/// The two-digger game.
const TWO_DIGGERS: &str = "2test01.edrf";

/// The frame on which the pause is requested, and the frame on which the
/// resume is requested.
const PAUSE_AT: usize = 500;
const RESUME_AT: usize = 560;

/// Samples per frame of 80 ms at 44100 Hz.
const SAMPLES_PER_FRAME: usize = 3528;

/// What one frame produced.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Frame {
    kind: FrameKind,
    hash: u32,
    commands: Vec<SoundCommand>,
    lives: u32,
    bonus_mode: bool,
}

/// A whole played game.
#[derive(Debug)]
struct Played {
    mode: GameMode,
    frames: Vec<Frame>,
    report: ReplayReport,
}

impl Played {
    /// Every command with the index of its frame, in emission order.
    fn commands(&self) -> impl Iterator<Item = (usize, SoundCommand)> + '_ {
        self.frames
            .iter()
            .enumerate()
            .flat_map(|(i, f)| f.commands.iter().map(move |&c| (i, c)))
    }
}

/// Plays `file` from the corpus with a pause from [`PAUSE_AT`] to
/// [`RESUME_AT`], handing each frame's commands to `sink` together with
/// the frame's index.
fn play(file: &str, sink: &mut dyn FnMut(usize, &[SoundCommand])) -> Played {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/recordings/data")
        .join(file);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let recording = digger_core::recording::parse(&bytes).expect("a corpus recording parses");
    let mut replay = Replay::new(&recording, ReplayOptions::default()).expect("playable");
    let mode = replay.game().state().mode();
    let mut frames = Vec::new();
    loop {
        let index = frames.len();
        let input = FrameInput {
            pause: index == PAUSE_AT || index == RESUME_AT,
            ..FrameInput::NONE
        };
        let step = replay.step(&input, &mut NoopObserver);
        let commands: Vec<SoundCommand> = replay.drain_sound().collect();
        sink(index, &commands);
        let state = replay.game().state();
        let kind = match step {
            ReplayStep::Running(kind) => kind,
            ReplayStep::Finished(_) => FrameKind::Wait,
        };
        frames.push(Frame {
            kind,
            hash: state_hash::hash(state),
            commands,
            lives: state.lives(DiggerId::ALL[0]),
            bonus_mode: state.bonus_mode(),
        });
        if matches!(step, ReplayStep::Finished(_)) {
            break;
        }
    }
    Played {
        mode,
        frames,
        report: replay.finish(),
    }
}

/// Plays `file` with every command sent to a rendering [`ManualDriver`];
/// the frontend toggles music, sound and the filter on the way. Returns
/// the game and the number of non-zero samples rendered.
fn play_with_sound(file: &str) -> (Played, usize) {
    let tables = TuneTables::embedded().expect("embedded tables decode");
    let mut driver = ManualDriver::new(SoundConfig::default(), tables);
    let mut audible = 0;
    let mut samples = vec![0_i16; SAMPLES_PER_FRAME];
    let played = play(file, &mut |frame, commands| {
        for &command in commands {
            driver.send(command);
        }
        match frame {
            1000 | 1400 => driver.control(SoundControl::ToggleMusic),
            1800 | 1900 => driver.control(SoundControl::ToggleSound),
            2200 => driver.control(SoundControl::SetFilter(false)),
            _ => {}
        }
        driver.render_into(&mut samples);
        audible += samples.iter().filter(|&&s| s != 0).count();
        // Events are read and ignored, as a frontend may.
        let _ = driver.events();
    });
    (played, audible)
}

/// Plays `file` with every command sent to a [`NullDriver`].
fn play_without_sound(file: &str) -> Played {
    let mut null = NullDriver::new();
    play(file, &mut |_, commands| {
        for &command in commands {
            null.send(command);
        }
    })
}

#[test]
fn simulation_unaffected_by_sound() {
    for file in [ONE_PLAYER, TWO_DIGGERS] {
        let (with, audible) = play_with_sound(file);
        let without = play_without_sound(file);
        assert!(audible > 0, "{file}: the driver rendered sound");
        assert_eq!(with.report.outcome, Outcome::Verified, "{file}");
        assert_eq!(without.report.outcome, Outcome::Verified, "{file}");
        assert_eq!(with.report, without.report, "{file}: reports");
        assert_eq!(with.frames.len(), without.frames.len(), "{file}: frames");
        for (i, (a, b)) in with.frames.iter().zip(&without.frames).enumerate() {
            assert_eq!(a, b, "{file}: frame {i}");
        }
        // The scripted game covers what the test is about.
        let has = |f: &dyn Fn(&SoundCommand) -> bool| with.commands().any(|(_, c)| f(&c));
        assert!(
            has(&|c| *c == SoundCommand::LevelDone),
            "{file}: a level completed"
        );
        assert!(
            has(&|c| *c == SoundCommand::DiggerDeath),
            "{file}: a digger killed"
        );
        assert!(
            with.frames.iter().any(|f| f.bonus_mode),
            "{file}: bonus mode entered"
        );
    }
}

/// Checks the command order rules shared by both games.
fn check_common_rules(file: &str, played: &Played) {
    let commands: Vec<(usize, SoundCommand)> = played.commands().collect();

    // Emerald-click immediately before every emerald note, in the same
    // frame.
    let mut notes = 0;
    for (i, &(frame, command)) in commands.iter().enumerate() {
        if let SoundCommand::EmeraldNote(_) = command {
            notes += 1;
            let before = i.checked_sub(1).map(|j| commands[j]);
            assert_eq!(
                before,
                Some((frame, SoundCommand::EmeraldClick)),
                "{file}: emerald note at command {i} not preceded by the click"
            );
        }
    }
    assert!(notes > 0, "{file}: emeralds eaten");

    // Per fireball: launch, explode, then the fire-stop of the removed
    // fireball; never a fire-stop of a fireball that has not exploded.
    let mut exploded = 0;
    for id in digger_sound::FireballId::ALL {
        let mut state = "idle";
        for &(frame, command) in &commands {
            state = match (state, command) {
                (_, SoundCommand::Fire(f)) if f == id => "flying",
                ("flying", SoundCommand::Explode(f)) if f == id => {
                    exploded += 1;
                    "exploded"
                }
                (s, SoundCommand::Explode(f)) if f == id => {
                    panic!("{file}: frame {frame}: explode of {id:?} while {s}")
                }
                ("exploded", SoundCommand::FireStop(f)) if f == id => "idle",
                (s, SoundCommand::FireStop(f)) if f == id => {
                    panic!("{file}: frame {frame}: fire-stop of {id:?} while {s}")
                }
                (s, _) => s,
            };
        }
    }
    assert!(exploded > 0, "{file}: fireballs exploded");

    // Pause on the requested frame, nothing while paused, resume on the
    // frame the resume is requested.
    let pause = &played.frames[PAUSE_AT];
    assert_eq!(
        pause.commands.last(),
        Some(&SoundCommand::Pause),
        "{file}: pause cue on frame {PAUSE_AT}"
    );
    for frame in &played.frames[PAUSE_AT + 1..RESUME_AT] {
        assert_eq!(frame.kind, FrameKind::Paused, "{file}: paused stretch");
        assert!(frame.commands.is_empty(), "{file}: nothing while paused");
    }
    assert_eq!(
        played.frames[RESUME_AT].commands.first(),
        Some(&SoundCommand::Resume),
        "{file}: resume cue on frame {RESUME_AT}"
    );
    let count = |want: SoundCommand| commands.iter().filter(|(_, c)| *c == want).count();
    assert_eq!(count(SoundCommand::Pause), 1, "{file}");
    assert_eq!(count(SoundCommand::Resume), 1, "{file}");

    // One level-done per completed level, each followed by the next
    // level's music, never by a dirge.
    let level_done: Vec<usize> = commands
        .iter()
        .filter(|(_, c)| *c == SoundCommand::LevelDone)
        .map(|&(f, _)| f)
        .collect();
    let levels_completed = usize::from(played.report.level) - 1;
    assert_eq!(
        level_done.len(),
        levels_completed,
        "{file}: level-done count"
    );
    for &frame in &level_done {
        let next_music = commands
            .iter()
            .find(|&&(f, c)| f > frame && matches!(c, SoundCommand::Music { .. }));
        assert!(
            matches!(
                next_music,
                Some((
                    _,
                    SoundCommand::Music {
                        tune: Tune::Main | Tune::LastLife,
                        ..
                    }
                ))
            ),
            "{file}: frame {frame}: level-done followed by {next_music:?}"
        );
    }
}

#[test]
fn emitted_commands_cover_the_boundary() {
    let one = play_without_sound(ONE_PLAYER);
    let two = play_without_sound(TWO_DIGGERS);
    assert_eq!(one.mode, GameMode::OnePlayer);
    assert_eq!(two.mode, GameMode::TwoDiggers);
    check_common_rules(ONE_PLAYER, &one);
    check_common_rules(TWO_DIGGERS, &two);

    // The battle tune exactly when the player has one life left.
    let mut battle = 0;
    for frame in &one.frames {
        for command in &frame.commands {
            match command {
                SoundCommand::Music {
                    tune: Tune::LastLife,
                    tempo,
                } => {
                    battle += 1;
                    assert_eq!(*tempo, Tempo::Standard);
                    assert_eq!(frame.lives, 1, "battle tune on the last life");
                }
                SoundCommand::Music {
                    tune: Tune::Main, ..
                } => assert_ne!(frame.lives, 1, "normal tune not on the last life"),
                _ => {}
            }
        }
    }
    assert!(battle > 0, "{ONE_PLAYER}: the battle tune was played");

    // The dirge at the standard tempo with one digger and at the quick
    // tempo with two.
    for (played, want) in [(&one, Tempo::Standard), (&two, Tempo::Quick)] {
        let dirges: Vec<Tempo> = played
            .commands()
            .filter_map(|(_, c)| match c {
                SoundCommand::Music {
                    tune: Tune::Dirge,
                    tempo,
                } => Some(tempo),
                _ => None,
            })
            .collect();
        assert!(!dirges.is_empty(), "{:?}: a dirge", played.mode);
        assert!(
            dirges.iter().all(|&t| t == want),
            "{:?}: {dirges:?}",
            played.mode
        );
    }

    // The bonus siren and the bonus tune in bonus mode, and the extra
    // life of the one-player game.
    let has = |played: &Played, want: SoundCommand| played.commands().any(|(_, c)| c == want);
    assert!(has(&one, SoundCommand::BonusPulseStart));
    assert!(has(
        &one,
        SoundCommand::Music {
            tune: Tune::Bonus,
            tempo: Tempo::Standard
        }
    ));
    assert!(has(&one, SoundCommand::ExtraLife));
}
