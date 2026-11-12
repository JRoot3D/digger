//! Shell tests of task group 9: the game-over hand-over, initials entry
//! by keyboard and gamepad, flashing, eligibility and score saving, and
//! recording saves.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]

use std::fs;
use std::path::Path;

use digger_core::geom::Px;
use digger_core::recording::{self, RecordingKind};
use digger_core::replay::ReplayOptions;
use digger_core::video::{Framebuffer, Palette, TextColor};
use digger_core::{Direction, FinishReason, SoundCommand};

use super::harness::Harness;
use super::initials::CLOSING_FRAMES;
use super::{Request, Scene, Setup, Shell};
use crate::config::file::tests::TempDir;
use crate::config::{GameMode, Settings};
use crate::input::{PadButton, PadInput, PadSignal};
use crate::scores::store::ScoreStore;
use crate::scores::{Entry, LevelSet, ScoreTable, Scores, TableKey, TableKind};

/// The frames the core shows `GAME OVER` or `TIME UP` for.
const NOTICE_FRAMES: usize = 50;

/// Settings for `mode` with a ten-second gauntlet clock.
fn settings(mode: GameMode) -> Settings {
    Settings {
        mode,
        gauntlet_time: 10,
        ..Settings::default()
    }
}

/// A table of ten entries `TOP` with `score` points each.
fn full_table(score: u32) -> ScoreTable {
    ScoreTable::from_entries([Entry::new(*b"TOP", score); 10]).unwrap()
}

/// Score tables where every table of the built-in levels is full with
/// 1000 points, so that a short game does not qualify.
fn high_tables() -> Scores {
    let mut scores = Scores::default();
    for kind in TableKind::ALL {
        scores.set_table(
            TableKey {
                levels: LevelSet::Builtin,
                kind,
            },
            full_table(1000),
        );
    }
    scores
}

/// A harness for `mode` whose shell keeps `scores` in memory, set up with
/// `setup`.
fn harness(mode: GameMode, scores: Scores, setup: Setup) -> Harness {
    let settings = settings(mode);
    let mut harness = Harness::with(settings.clone(), setup.clone());
    harness.shell = Shell::with_scores(settings, setup, scores);
    harness
}

/// The table of `mode` on the built-in levels.
fn key(mode: GameMode) -> TableKey {
    TableKey::from_mode(mode, LevelSet::Builtin)
}

/// Whether `text` is shown at `at` in `color` on `picture`.
fn shows(picture: &Framebuffer, text: &str, at: (i32, i32), color: TextColor) -> bool {
    let mut copy = picture.clone();
    copy.draw_text(text, Px::new(at.0, at.1), color);
    copy == *picture
}

impl Harness {
    /// The new-high-score screen's initials so far, if one is shown.
    fn typed(&self) -> Option<String> {
        match self.shell.scene() {
            Scene::Playing(play) => play.initials().map(super::initials::InitialsScene::typed),
            _ => None,
        }
    }

    /// Whether a step of the current game reported a player out.
    fn player_out(&self) -> bool {
        match self.shell.scene() {
            Scene::Playing(play) => play
                .last_outcome()
                .is_some_and(|outcome| outcome.player_out.is_some()),
            _ => false,
        }
    }

    /// Starts a game, digs for 40 frames with `keys` held (so that the
    /// diggers score), then lets the game run until a player is out or
    /// the title returns. Returns the number of frames run after the
    /// digging.
    fn play_until_out(&mut self, keys: &[&str]) -> usize {
        self.play();
        self.dig(keys);
        self.until_out()
    }

    /// Holds `keys` for 40 frames.
    fn dig(&mut self, keys: &[&str]) {
        for key in keys {
            self.press(key);
        }
        self.run(40);
        for key in keys {
            self.release(key);
        }
    }

    /// Runs frames until a player is out or the title returns.
    fn until_out(&mut self) -> usize {
        let mut frames = 0;
        loop {
            self.frame();
            frames += 1;
            if self.player_out() || self.on_title() {
                return frames;
            }
            assert!(frames < 20_000, "the game does not end");
        }
    }

    /// Runs frames until the title returns; returns how many.
    fn until_title(&mut self) -> usize {
        let mut frames = 0;
        while !self.on_title() {
            self.frame();
            frames += 1;
            assert!(frames < 20_000, "the title does not return");
        }
        frames
    }

    /// Types `initials` one letter per frame.
    fn enter(&mut self, initials: &str) {
        for c in initials.chars() {
            let name = format!("Key{}", c.to_ascii_uppercase());
            self.type_key(&name, &c.to_string());
            self.frame();
        }
    }

    /// Runs the closing frames after the third letter and the frame that
    /// completes the entry.
    fn close_entry(&mut self) {
        self.run(usize::from(CLOSING_FRAMES));
        assert!(self.typed().is_some(), "the screen stays 20 frames");
        self.frame();
    }

    /// The current game's picture shows `text` at `at` in colour 3.
    fn game_shows(&self, text: &str, at: (i32, i32)) -> bool {
        shows(self.game().framebuffer(), text, at, TextColor::Three)
    }
}

#[test]
fn game_over_sequences() {
    // One player, a score that does not qualify: no entry, `GAME OVER`
    // for 50 frames, then the title.
    let mut h = harness(GameMode::OnePlayer, high_tables(), Setup::default());
    h.play_until_out(&["ArrowLeft"]);
    assert_eq!(h.typed(), None, "no entry for 50 points against 1000");
    assert!(h.game_shows("GAME OVER", (104, 0)));
    assert_eq!(h.until_title(), NOTICE_FRAMES);
    assert_eq!(*h.shell.scores(), high_tables());

    // One player, a qualifying score: the entry screen on the frame the
    // player is out, all sound stopped, then the notice.
    let mut h = harness(GameMode::OnePlayer, Scores::default(), Setup::default());
    h.play_until_out(&["ArrowLeft"]);
    assert_eq!(h.typed().as_deref(), Some(""));
    let out_frame = h.frames.last().unwrap();
    assert_eq!(out_frame.sound.last(), Some(&SoundCommand::StopAll));
    let picture = h.shell.picture();
    assert!(shows(picture, "PLAYER 1", (108, 0), TextColor::Two));
    h.enter("MSX");
    h.close_entry();
    assert_eq!(h.typed(), None, "the game resumes");
    assert!(h.game_shows("GAME OVER", (104, 0)));
    // The completing frame stepped the game: 49 notice frames remain.
    assert_eq!(h.until_title(), NOTICE_FRAMES - 1);
    let first = h.shell.scores().table(&key(GameMode::OnePlayer)).entries()[0];
    assert_eq!(first.initials, *b"MSX");
    assert_eq!(first.score, 50);

    // Gauntlet: without a qualifying score `TIME UP` follows at once …
    let mut h = harness(GameMode::Gauntlet, high_tables(), Setup::default());
    h.play_until_out(&["ArrowLeft"]);
    assert_eq!(h.typed(), None);
    assert!(h.game_shows("TIME UP", (120, 0)));
    assert!(!h.game_shows("GAME OVER", (104, 0)));
    assert_eq!(h.until_title(), NOTICE_FRAMES);
    // … and with one, after the entry; `GAME OVER` is never shown.
    let mut h = harness(GameMode::Gauntlet, Scores::default(), Setup::default());
    h.play_until_out(&["ArrowLeft"]);
    assert!(h.typed().is_some());
    h.enter("GNT");
    h.close_entry();
    assert!(h.game_shows("TIME UP", (120, 0)));
    assert!(!h.game_shows("GAME OVER", (104, 0)));
    h.until_title();
    let table = h.shell.scores().table(&key(GameMode::Gauntlet));
    assert_eq!(table.entries()[0].initials, *b"GNT");
    assert_eq!(
        h.shell.scores().table(&key(GameMode::OnePlayer)),
        &ScoreTable::EMPTY,
        "gauntlet scores stay apart"
    );

    // Two diggers: `PLAYER 1`, then `PLAYER 2`, both in the two-Digger
    // table.
    let mut h = harness(GameMode::TwoDiggers, Scores::default(), Setup::default());
    h.play_until_out(&["ArrowLeft", "KeyS"]);
    assert!(shows(
        h.shell.picture(),
        "PLAYER 1",
        (108, 0),
        TextColor::Two
    ));
    h.enter("ONE");
    h.close_entry();
    assert_eq!(h.typed().as_deref(), Some(""), "the second digger's entry");
    assert!(shows(
        h.shell.picture(),
        "PLAYER 2",
        (108, 0),
        TextColor::Two
    ));
    h.enter("TWO");
    h.close_entry();
    assert_eq!(h.typed(), None);
    assert!(h.game_shows("GAME OVER", (104, 0)));
    h.until_title();
    let table = h.shell.scores().table(&key(GameMode::TwoDiggers));
    let initials: Vec<[u8; 3]> = table.entries()[..2].iter().map(|e| e.initials).collect();
    assert!(initials.contains(b"ONE") && initials.contains(b"TWO"));
    assert_eq!(
        h.shell.scores().table(&key(GameMode::OnePlayer)),
        &ScoreTable::EMPTY
    );

    // Alternating players: player 1 is out first while player 2 has
    // lives; player 1's entry runs and player 2's turn follows.
    let mut h = harness(GameMode::TwoPlayers, Scores::default(), Setup::default());
    h.play_until_out(&["ArrowLeft"]);
    assert!(shows(
        h.shell.picture(),
        "PLAYER 1",
        (108, 0),
        TextColor::Two
    ));
    h.enter("ALT");
    h.close_entry();
    assert!(h.game_shows("GAME OVER", (104, 0)));
    h.run(NOTICE_FRAMES);
    assert!(!h.on_title(), "player 2 plays on");
    assert!(matches!(h.shell.scene(), Scene::Playing(_)));
    // Player 2 scores nothing and does not qualify: no second entry.
    h.until_out();
    assert_eq!(h.typed(), None);
    h.until_title();
    let table = h.shell.scores().table(&key(GameMode::OnePlayer));
    assert_eq!(table.entries()[0].initials, *b"ALT", "the one-Digger table");
}

/// A shell showing a new-high-score screen of a one-player game with
/// `settings`.
fn at_entry_with(settings: Settings) -> Harness {
    let mut h = Harness::with_settings(settings);
    h.play_until_out(&["ArrowLeft"]);
    assert_eq!(h.typed().as_deref(), Some(""), "an entry is due");
    h
}

/// A shell showing a new-high-score screen of a one-player game.
fn at_entry() -> Harness {
    at_entry_with(Settings::default())
}

#[test]
fn initials_three_letters() {
    let mut h = at_entry();
    // Lower case is stored and shown in upper case.
    h.type_key("KeyM", "m");
    h.frame();
    h.type_key("KeyS", "S");
    h.frame();
    assert_eq!(h.typed().as_deref(), Some("MS"));
    let picture = h.shell.picture();
    assert!(shows(picture, "M", (128, 130), TextColor::Three));
    assert!(shows(picture, "S", (152, 130), TextColor::Three));
    assert!(shows(picture, "_", (176, 130), TextColor::Three));
    h.type_key("KeyX", "x");
    h.frame();
    assert_eq!(h.typed().as_deref(), Some("MSX"));
    assert!(shows(h.shell.picture(), "X", (176, 130), TextColor::Three));
    // Further letters during the closing frames change nothing.
    h.type_key("KeyQ", "q");
    h.frame();
    assert_eq!(h.typed().as_deref(), Some("MSX"));
    h.run(usize::from(CLOSING_FRAMES) - 1);
    assert!(h.typed().is_some(), "the screen stays for 20 frames");
    h.frame();
    assert_eq!(h.typed(), None, "then the entry is made");
    h.until_title();
    let first = h.shell.scores().table(&key(GameMode::OnePlayer)).entries()[0];
    assert_eq!(first, Entry::new(*b"MSX", 50));
    assert!(shows(
        h.shell.picture(),
        "MSX      50",
        (16, 44),
        TextColor::Two
    ));
}

#[test]
fn initials_arrow_keys_ignored() {
    let mut h = at_entry();
    let sound_on = h.shell.sound_on();
    let music_on = h.shell.music_on();
    let frame_time = h.shell.frame_time();
    // Arrow keys, digits, function keys and their actions are ignored.
    for (name, text) in [
        ("ArrowRight", None),
        ("ArrowUp", None),
        ("Digit1", Some("1")),
        ("F1", None),
        ("F7", None),
        ("F9", None),
        ("Equal", Some("=")),
        ("Space", Some(" ")),
        ("KeyN", None),
    ] {
        match text {
            Some(text) => h.type_key(name, text),
            None => h.tap(name),
        }
        let frame = h.frame().clone();
        assert!(frame.controls.is_empty(), "{name} toggles nothing");
        assert_eq!(h.typed().as_deref(), Some(""), "{name} types nothing");
    }
    assert_eq!(h.shell.sound_on(), sound_on);
    assert_eq!(h.shell.music_on(), music_on);
    assert_eq!(h.shell.frame_time(), frame_time);
    assert_eq!(h.shell.mode(), GameMode::OnePlayer);
    // `ArrowRight` then `m`: the first initial is `M`, and no `O`.
    h.tap("ArrowRight");
    h.frame();
    h.type_key("KeyM", "m");
    h.frame();
    assert_eq!(h.typed().as_deref(), Some("M"));
    // The fullscreen key keeps working.
    h.tap("F11");
    assert_eq!(h.frame().requests, [Request::ToggleFullscreen]);
    assert_eq!(h.typed().as_deref(), Some("M"));
}

#[test]
fn initials_backspace() {
    let mut h = at_entry();
    // Backspace at the first position changes nothing.
    h.tap("Backspace");
    h.frame();
    assert_eq!(h.typed().as_deref(), Some(""));
    h.enter("AB");
    assert!(shows(h.shell.picture(), "B", (152, 130), TextColor::Three));
    h.tap("Backspace");
    h.frame();
    assert_eq!(h.typed().as_deref(), Some("A"));
    assert!(
        shows(h.shell.picture(), "_", (152, 130), TextColor::Three),
        "the underscore is shown again"
    );
    h.enter("C");
    assert_eq!(h.typed().as_deref(), Some("AC"));
    // Delete steps back too.
    h.tap("Delete");
    h.frame();
    assert_eq!(h.typed().as_deref(), Some("A"));
    h.enter("CE");
    h.close_entry();
    h.until_title();
    let first = h.shell.scores().table(&key(GameMode::OnePlayer)).entries()[0];
    assert_eq!(first.initials, *b"ACE");
}

/// Golden CRC of the one-player new-high-score screen with 50 points,
/// recorded from the port.
const INITIALS_SCREEN_CRC: u32 = 3_484_113_356;

#[test]
fn initials_screen_crc() {
    let h = at_entry();
    let mut expected = Framebuffer::new();
    expected.draw_number(50, 6, Px::new(0, 0), TextColor::Three);
    expected.draw_text("PLAYER 1", Px::new(108, 0), TextColor::Two);
    expected.draw_text(" NEW HIGH SCORE ", Px::new(64, 40), TextColor::Two);
    expected.draw_text("ENTER YOUR", Px::new(100, 70), TextColor::Three);
    expected.draw_text(" INITIALS", Px::new(100, 90), TextColor::Three);
    expected.draw_text("_ _ _", Px::new(128, 130), TextColor::Three);
    assert!(*h.shell.picture() == expected, "the screen's layout");
    assert_eq!(h.shell.picture().crc32(), INITIALS_SCREEN_CRC, "golden CRC");
}

#[test]
fn initials_palette_alternates() {
    let mut h = at_entry();
    let start = h.frames.len() - 1;
    assert_eq!(h.frames[start].palette, Palette::Zero);
    h.run(5);
    h.enter("PAL");
    h.close_entry();
    // The completing frame shows the game again.
    let shown = &h.frames[start..h.frames.len() - 1];
    for pair in shown[..shown.len() - 1].windows(2) {
        assert_ne!(pair[0].palette, pair[1].palette, "one change per frame");
    }
    assert_eq!(
        shown.last().unwrap().palette,
        Palette::Zero,
        "ends on the normal palette"
    );
    assert_eq!(h.frames.last().unwrap().palette, Palette::Zero);
}

#[test]
fn initials_reduce_flashing_steady() {
    let mut h = at_entry_with(Settings {
        reduce_flashing: true,
        ..Settings::default()
    });
    let start = h.frames.len() - 1;
    h.run(5);
    h.enter("RED");
    h.close_entry();
    assert!(
        h.frames[start..]
            .iter()
            .all(|frame| frame.palette == Palette::Zero),
        "the colours do not change"
    );
}

impl Harness {
    /// Presses and releases `input` on the pad of slot 0 within a frame,
    /// then runs the frame.
    fn pad_tap(&mut self, input: PadInput) {
        for pressed in [true, false] {
            self.pad(PadSignal::Input {
                slot: 0,
                input,
                pressed,
            });
        }
        self.frame();
    }
}

#[test]
fn initials_with_pad_cab() {
    let mut h = at_entry();
    let up = PadInput::DPad(Direction::Up);
    let fire = PadInput::Button(PadButton::South);
    h.pad_tap(up);
    assert!(
        shows(h.shell.picture(), "B", (128, 130), TextColor::Three),
        "the chosen letter is shown"
    );
    h.pad_tap(up);
    h.pad_tap(fire);
    assert_eq!(h.typed().as_deref(), Some("C"));
    h.pad_tap(fire);
    assert_eq!(h.typed().as_deref(), Some("CA"));
    // Down wraps from A to Z, the stick steps too, d-pad left steps back.
    h.pad_tap(PadInput::DPad(Direction::Down));
    assert!(shows(h.shell.picture(), "Z", (176, 130), TextColor::Three));
    h.pad_tap(PadInput::DPad(Direction::Left));
    assert_eq!(h.typed().as_deref(), Some("C"));
    h.pad_tap(PadInput::Button(PadButton::West));
    assert_eq!(h.typed().as_deref(), Some("CA"));
    h.pad_tap(PadInput::Stick(Direction::Up));
    h.pad_tap(fire);
    assert_eq!(h.typed().as_deref(), Some("CAB"));
    h.close_entry();
    h.until_title();
    let first = h.shell.scores().table(&key(GameMode::OnePlayer)).entries()[0];
    assert_eq!(first.initials, *b"CAB", "entry completes");
}

/// The DRF recording of a one-player game that ends with 100 points
/// without asking for initials (the tables are full).
fn recorded_game() -> (String, u32) {
    let mut h = harness(GameMode::OnePlayer, high_tables(), Setup::default());
    h.play_until_out(&["ArrowUp"]);
    h.until_title();
    let last = h.shell.last_game().unwrap();
    let score = u32::try_from(last.summary.scores[0].visible).unwrap();
    (last.text.clone(), score)
}

#[test]
fn playback_never_enters_scores() {
    let (text, score) = recorded_game();
    assert!(score > 0, "the recording scores");
    let recording = recording::parse(text.as_bytes()).unwrap();
    let mut h = Harness::new();
    h.shell.start_playback(&recording).unwrap();
    let mut frames = 0;
    while !h.on_title() {
        h.frame();
        assert!(!h.shell.entering_initials(), "no initials during playback");
        frames += 1;
        assert!(frames < 20_000);
    }
    assert_eq!(*h.shell.scores(), Scores::default(), "tables unchanged");
}

#[test]
fn taken_over_game_never_enters_scores() {
    // `--play game.drf --take-over-at 20`, with empty tables that any
    // score would enter.
    let (text, _) = recorded_game();
    let dir = TempDir::new("take-over-scores");
    let file = dir.path().join("game.drf");
    fs::write(&file, &text).unwrap();
    let setup = Setup {
        play: vec![file],
        take_over_at: Some(20),
        ..Setup::default()
    };
    let mut h = harness(GameMode::OnePlayer, Scores::default(), setup);
    h.run(60);
    let Scene::Playback(playback) = h.shell.scene() else {
        panic!("a playback");
    };
    assert!(playback.taken_over(), "taken over");
    assert!(h.game().state().ticks() > 20);
    // The player digs, scoring above the empty tables, and the game ends.
    h.dig(&["ArrowLeft"]);
    let mut frames = 0;
    while !h.on_title() {
        h.frame();
        assert!(!h.shell.entering_initials(), "no initials after take-over");
        frames += 1;
        assert!(frames < 20_000);
    }
    assert_eq!(*h.shell.scores(), Scores::default(), "tables unchanged");
    // The game is kept for the save-recording action like a live one.
    assert!(h.shell.last_game().is_some(), "the taken-over game is kept");
}

#[test]
fn unlimited_lives_is_eligible() {
    let mut h = at_entry_with(Settings {
        unlimited_lives: true,
        ..Settings::default()
    });
    h.enter("UNL");
    h.close_entry();
    h.until_title();
    let first = h.shell.scores().table(&key(GameMode::OnePlayer)).entries()[0];
    assert_eq!(first.initials, *b"UNL");
}

#[test]
fn entry_saved_before_title() {
    let dir = TempDir::new("shell");
    let path = dir.path().join("scores.toml");
    let (store, _) = ScoreStore::open(path.clone());
    let settings = Settings::default();
    let mut h = Harness::with_settings(settings.clone());
    h.shell = Shell::with_store(settings, Setup::default(), store);
    h.play_until_out(&["ArrowLeft"]);
    h.enter("MSX");
    h.run(usize::from(CLOSING_FRAMES));
    assert!(
        !path.exists(),
        "nothing is written before the entry is made"
    );
    h.frame();
    assert!(!h.on_title(), "the notice is still to come");
    let (scores, _) = crate::scores::store::load(&path);
    assert_eq!(
        scores.table(&key(GameMode::OnePlayer)).entries()[0],
        Entry::new(*b"MSX", 50),
        "saved before the title returns"
    );
    h.until_title();
    assert!(shows(
        h.shell.picture(),
        "MSX      50",
        (16, 44),
        TextColor::Two
    ));
}

#[test]
fn record_option_saves_each_game() {
    let dir = TempDir::new("shell");
    let path = dir.path().join("Run.EDRF");
    let setup = Setup {
        record_kind: super::recording::kind_for_path(&path),
        record_path: Some(path.clone()),
        ..Setup::default()
    };
    assert_eq!(setup.record_kind, RecordingKind::Edrf);
    assert_eq!(
        super::recording::kind_for_path(Path::new("game.drf")),
        RecordingKind::Drf
    );
    let mut h = harness(GameMode::OnePlayer, high_tables(), setup);
    h.play_until_out(&["ArrowUp"]);
    h.until_title();
    assert!(h.shell.last_game().is_none(), "written at once");
    let text = fs::read(&path).unwrap();
    assert!(text.starts_with(b"eDRF"), "an extended recording");
    let report = digger_core::replay::verify(&text, ReplayOptions::default()).unwrap();
    assert_eq!(report.exit_status(), 0, "{}", report.result_line());
    assert_eq!(report.scores[0], 100);

    // A second game, quit, replaces the file.
    h.play();
    h.run(10);
    h.tap("F10");
    h.frame();
    assert!(h.on_title());
    let second = fs::read(&path).unwrap();
    assert_ne!(second, text, "replaced by the second game");
    let report = digger_core::replay::verify(&second, ReplayOptions::default()).unwrap();
    assert_eq!(report.exit_status(), 0, "{}", report.result_line());
    assert_eq!(report.scores[0], 0);
}

/// The files in `dir`, by name, sorted.
fn files_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn f8_saves_once_with_auto_name() {
    let dir = TempDir::new("shell");
    let recordings = dir.path().join("recordings");
    let setup = Setup {
        recordings_dir: Some(recordings.clone()),
        ..Setup::default()
    };
    let mut h = harness(GameMode::OnePlayer, Scores::default(), setup);
    h.play_until_out(&["ArrowLeft"]);
    h.enter("ABC");
    h.close_entry();
    h.until_title();
    h.tap("F8");
    let frame = h.frame().clone();
    assert!(h.on_title(), "the title stays");
    assert_eq!(frame.requests, [] as [Request; 0]);
    assert_eq!(files_in(&recordings), ["ABC50.drf"]);
    let saved = fs::read(recordings.join("ABC50.drf")).unwrap();
    let report = digger_core::replay::verify(&saved, ReplayOptions::default()).unwrap();
    assert_eq!(report.exit_status(), 0, "{}", report.result_line());
    assert!(h.shell.last_game().is_none());
    // A second F8 writes nothing.
    h.tap("F8");
    h.frame();
    assert_eq!(files_in(&recordings), ["ABC50.drf"]);

    // A game without an entry is named `___<score>`; a name that is
    // taken gets a numeric suffix.
    h.play();
    h.run(10);
    h.tap("F10");
    h.frame();
    fs::write(recordings.join("___0.drf"), "taken").unwrap();
    h.tap("F8");
    h.frame();
    assert_eq!(
        files_in(&recordings),
        ["ABC50.drf", "___0-2.drf", "___0.drf"]
    );
    assert_eq!(fs::read(recordings.join("___0.drf")).unwrap(), b"taken");
}

#[test]
fn f8_without_game_does_nothing() {
    let dir = TempDir::new("shell");
    let recordings = dir.path().join("recordings");
    let setup = Setup {
        recordings_dir: Some(recordings.clone()),
        ..Setup::default()
    };
    let mut h = harness(GameMode::OnePlayer, Scores::default(), setup);
    h.frame();
    h.tap("F8");
    let frame = h.frame().clone();
    assert!(h.on_title(), "the title stays");
    assert_eq!(frame.requests, [] as [Request; 0]);
    assert!(!recordings.exists(), "no file is written");
}

#[test]
fn initials_line_in_drf() {
    for kind in [RecordingKind::Drf, RecordingKind::Edrf] {
        let mut h = Harness::recording(kind);
        h.play_until_out(&["ArrowLeft"]);
        h.enter("ABC");
        h.close_entry();
        h.until_title();
        let last = h.shell.last_game().unwrap();
        assert_eq!(last.summary.reason, FinishReason::AllPlayersOut);
        assert_eq!(last.entry, Some(Entry::new(*b"ABC", 50)));
        let has_line = last.text.lines().any(|line| line == "*ABC");
        assert_eq!(has_line, kind == RecordingKind::Drf, "{kind:?}");
        let report =
            digger_core::replay::verify(last.text.as_bytes(), ReplayOptions::default()).unwrap();
        assert_eq!(
            report.exit_status(),
            0,
            "{kind:?}: {}",
            report.result_line()
        );
        assert_eq!(report.scores[0], 50);
    }
}

#[test]
fn quit_or_close_during_initials_adds_no_entry() {
    // Quit: back to the title at once, without an entry and without the
    // notice; the game's recording ends as a quit.
    let mut h = at_entry();
    h.enter("A");
    h.tap("F10");
    let frame = h.frame().clone();
    assert!(h.on_title());
    assert_eq!(frame.sound.last(), Some(&SoundCommand::StopAll));
    assert_eq!(*h.shell.scores(), Scores::default());
    assert_eq!(
        h.shell.last_game().unwrap().summary.reason,
        FinishReason::Quit
    );

    // Close: the program ends with status 0 and the score file is left
    // unchanged.
    let dir = TempDir::new("shell");
    let path = dir.path().join("scores.toml");
    let (store, _) = ScoreStore::open(path.clone());
    let settings = Settings::default();
    let mut h = Harness::with_settings(settings.clone());
    h.shell = Shell::with_store(settings, Setup::default(), store);
    h.play_until_out(&["ArrowLeft"]);
    h.enter("A");
    h.close();
    assert_eq!(h.frame().requests, [Request::Exit(0)]);
    assert_eq!(*h.shell.scores(), Scores::default());
    assert!(!path.exists(), "the score file is not written");
}
