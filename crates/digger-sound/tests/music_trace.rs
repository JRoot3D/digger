//! Tick-trace tests of the tunes, the dirge and the level-done jingle (task
//! group 5; design D12).
//!
//! Every test drives a [`ManualDriver`] tick by tick and checks the voice
//! and music level each tick reports against the numbers of the sound
//! spec. Ticks are counted from 1, the first being the tick on which the
//! command takes effect.

#![allow(
    clippy::indexing_slicing,
    reason = "traces are indexed by tick; an out-of-range tick is a test failure"
)]

use digger_sound::{
    Divisor, Hertz, ManualDriver, SampleRate, SoundCommand, SoundConfig, SoundEvent, SoundSink,
    Tempo, TickTrace, Tune, TuneTables, Voice, music::params,
};

/// The embedded tune tables.
#[expect(
    clippy::expect_used,
    reason = "the embedded tables are tested to decode"
)]
fn tables() -> TuneTables {
    TuneTables::embedded().expect("embedded tables decode")
}

/// A driver for a new engine with `config`.
fn driver_with(config: SoundConfig) -> ManualDriver {
    ManualDriver::new(config, tables())
}

/// A driver for a new engine with the default configuration.
fn new_driver() -> ManualDriver {
    driver_with(SoundConfig::default())
}

/// A driver with music switched off from the start.
fn music_off_driver() -> ManualDriver {
    driver_with(SoundConfig {
        music_on: false,
        ..SoundConfig::default()
    })
}

/// The command that plays `tune` at `tempo`.
fn music(tune: Tune, tempo: Tempo) -> SoundCommand {
    SoundCommand::Music { tune, tempo }
}

/// The command that plays `tune` at the standard tempo.
fn play(tune: Tune) -> SoundCommand {
    music(tune, Tempo::Standard)
}

/// The music divisor sounding on a tick, if music is heard.
fn music_divisor(trace: &TickTrace) -> Option<u16> {
    match trace.voice {
        Voice::Music { divisor, .. } => Some(divisor.get()),
        _ => None,
    }
}

/// The music levels of `traces`.
fn levels(traces: &[TickTrace]) -> Vec<u8> {
    traces.iter().map(|t| t.music_level.get()).collect()
}

/// The number of ticks one pass through `tune` lasts at `tempo`: each note
/// occupies its units times the ticks per unit, plus one tick.
fn tune_ticks(tune: Tune, tempo: Tempo) -> usize {
    let per_unit = usize::from(params(tune).ticks_per_unit(tempo));
    tables()
        .notes(tune)
        .iter()
        .map(|note| usize::from(note.units) * per_unit + 1)
        .sum()
}

// ---- Tunes -------------------------------------------------------------

#[test]
fn note_lengths() {
    // Normal tune: the first two notes (4063 and 4561) last 2 units each,
    // 2 x 6 + 1 = 13 ticks.
    let mut driver = new_driver();
    driver.send(play(Tune::Main));
    let traces = driver.advance_ticks(26);
    let divisors: Vec<_> = traces.iter().map(music_divisor).collect();
    let mut expected = vec![Some(4063); 13];
    expected.extend([Some(4561); 13]);
    assert_eq!(divisors, expected);
    assert_eq!(
        levels(&traces[..13]),
        [50, 35, 20, 8, 8, 8, 8, 8, 8, 8, 8, 7, 6]
    );
    // The second note starts a new attack.
    assert_eq!(traces[13].music_level.get(), 50);

    // Bonus tune: its first note lasts 2 units, 2 x 3 + 1 = 7 ticks, with
    // the release from tick 3 (one unit less than its duration); tick 8
    // loads the second note and attacks again.
    let mut driver = new_driver();
    driver.send(play(Tune::Bonus));
    let traces = driver.advance_ticks(8);
    assert_eq!(levels(&traces), [28, 48, 44, 40, 36, 32, 28, 48]);
    assert!(traces.iter().all(|t| music_divisor(t) == Some(4561)));
}

#[test]
fn tune_loops_to_first_note() {
    let pass = tune_ticks(Tune::Main, Tempo::Standard);
    let mut driver = new_driver();
    driver.send(play(Tune::Main));
    let traces = driver.advance_ticks(pass + 13);
    // The last note (2712, 4 units = 25 ticks) ends the pass; its release
    // reaches level 1 on its 18th tick, which is silent ...
    let last = &traces[pass - 25..pass];
    assert!(last[..17].iter().all(|t| music_divisor(t) == Some(2712)));
    assert!(last[17..].iter().all(|t| t.voice == Voice::Silent));
    assert!(last[17..].iter().all(|t| t.music_level.get() == 1));
    // ... and the first note follows, with the same 13 ticks as at the
    // start (the level carried in is the same, 6).
    let again = &traces[pass..];
    assert!(again.iter().all(|t| music_divisor(t) == Some(4063)));
    assert_eq!(levels(again), levels(&traces[..13]));
}

#[test]
fn tune_restarts_from_the_first_note() {
    let mut driver = new_driver();
    driver.send(play(Tune::Main));
    driver.advance_ticks(100);
    // A new play-music command loads the tune's first note on the next
    // tick, even for the tune already playing.
    driver.send(play(Tune::Bonus));
    assert_eq!(music_divisor(&driver.advance_ticks(1)[0]), Some(4561));
    driver.advance_ticks(50);
    driver.send(play(Tune::Main));
    let traces = driver.advance_ticks(13);
    assert!(traces.iter().all(|t| music_divisor(t) == Some(4063)));
}

#[test]
fn rest_is_silent_but_envelope_runs() {
    // The dirge opens with a 2-unit rest: 21 silent ticks over which the
    // envelope still attacks, decays to 25 and releases from tick 10.
    let mut driver = new_driver();
    driver.send(play(Tune::Dirge));
    let traces = driver.advance_ticks(22);
    assert!(traces[..21].iter().all(|t| t.voice == Voice::Silent));
    assert_eq!(
        levels(&traces[..21]),
        [
            50, 45, 40, 35, 30, 25, 25, 25, 25, 24, 23, 22, 21, 20, 19, 18, 17, 16, 15, 14, 13
        ]
    );
    // Tick 22 loads the first note.
    assert_eq!(
        traces[21].voice,
        Voice::Music {
            divisor: Divisor(4561),
            level: digger_sound::Level::MAX
        }
    );
}

#[test]
fn stop_music_ends_the_tune() {
    let mut driver = new_driver();
    driver.send(play(Tune::Main));
    driver.advance_ticks(5);
    driver.send(SoundCommand::MusicOff);
    let traces = driver.advance_ticks(100);
    assert!(traces.iter().all(|t| t.voice == Voice::Silent));
    // The level is kept where the tune left it.
    assert!(traces.iter().all(|t| t.music_level.get() == 8));
}

#[test]
fn level_carries_across_tunes() {
    let mut driver = new_driver();
    // A new engine starts at level 8, so the first attack of the bonus
    // tune gives 28.
    driver.send(play(Tune::Bonus));
    assert_eq!(levels(&driver.advance_ticks(1)), [28]);
    // Stopping and starting again keeps the level: 28 + 20 = 48.
    driver.send(SoundCommand::MusicOff);
    assert_eq!(levels(&driver.advance_ticks(10)), [28; 10]);
    driver.send(play(Tune::Bonus));
    assert_eq!(levels(&driver.advance_ticks(1)), [48]);
    // The normal tune decays from there to its sustain of 8 ...
    driver.send(play(Tune::Main));
    assert_eq!(levels(&driver.advance_ticks(4)), [50, 35, 20, 8]);
    // ... which the battle tune's attack starts from: 8 + 28 = 36.
    driver.send(play(Tune::LastLife));
    assert_eq!(levels(&driver.advance_ticks(2)), [36, 50]);
}

#[test]
fn battle_tune_is_transposed() {
    // The battle tune's first note is stored as 4561 and plays as
    // (4561 x 891 + 500) / 1000 = 4064.
    let mut driver = new_driver();
    driver.send(play(Tune::LastLife));
    let traces = driver.advance_ticks(9);
    assert!(traces.iter().all(|t| music_divisor(t) == Some(4064)));
    assert_eq!(levels(&traces), [36, 50, 42, 34, 26, 22, 18, 14, 10]);
}

#[test]
fn effect_displaces_music_for_its_ticks_only() {
    let mut plain = new_driver();
    plain.send(play(Tune::Main));
    let reference = plain.advance_ticks(40);

    let mut driver = new_driver();
    driver.send(play(Tune::Main));
    let mut traces = driver.advance_ticks(4);
    driver.send(SoundCommand::EmeraldClick);
    traces.extend(driver.advance_ticks(36));

    for (n, (got, want)) in traces.iter().zip(&reference).enumerate() {
        let tick = n + 1;
        if tick == 5 {
            // The click displaces the tune for its one tick ...
            assert_eq!(got.voice, Voice::Effect(Divisor(1000)), "tick {tick}");
        } else {
            // ... while the tune kept advancing underneath.
            assert_eq!(got.voice, want.voice, "tick {tick}");
        }
        assert_eq!(got.music_level, want.music_level, "tick {tick}");
    }

    // A silent offer (the death effect's 20000) still displaces music.
    let mut driver = new_driver();
    driver.send(play(Tune::Main));
    driver.advance_ticks(3);
    driver.send(SoundCommand::DiggerDeath);
    let first = driver.advance_ticks(1);
    assert_eq!(first[0].voice, Voice::Effect(Divisor(19_000)));
    assert_eq!(first[0].voice.tone(SampleRate::DEFAULT), None);
}

#[test]
fn death_stops_music() {
    let mut driver = new_driver();
    driver.send(play(Tune::Main));
    driver.advance_ticks(20);
    driver.send(SoundCommand::DiggerDeath);
    let death = driver.advance_ticks(51);
    assert!(death.iter().all(|t| matches!(t.voice, Voice::Effect(_))));
    // The tune does not resume when the effect ends.
    let after = driver.advance_ticks(500);
    assert!(after.iter().all(|t| t.voice == Voice::Silent));
}

// ---- The dirge ---------------------------------------------------------

/// The 1-based tick on which the dirge at `tempo` reports its finish, and
/// the traces of the ticks up to and including it.
fn dirge_until_finished(tempo: Tempo, limit: usize) -> (Option<usize>, Vec<TickTrace>) {
    let mut driver = new_driver();
    driver.send(music(Tune::Dirge, tempo));
    let mut traces = Vec::new();
    for tick in 1..=limit {
        traces.extend(driver.advance_ticks(1));
        let events = driver.events();
        if !events.is_empty() {
            assert_eq!(events, [SoundEvent::DirgeFinished], "tick {tick}");
            return (Some(tick), traces);
        }
    }
    (None, traces)
}

#[test]
fn dirge_finishes_on_tick_513() {
    let (finished, traces) = dirge_until_finished(Tempo::Standard, 600);
    assert_eq!(finished, Some(513));
    // The last tick of the audible part sounds; the first rest does not.
    assert_eq!(music_divisor(&traces[511]), Some(4561));
    assert_eq!(traces[512].voice, Voice::Silent);

    // The rests that follow are silent too.
    let mut driver = new_driver();
    driver.send(play(Tune::Dirge));
    driver.advance_ticks(512);
    let rests = driver.advance_ticks(12 * 161);
    assert!(rests.iter().all(|t| t.voice == Voice::Silent));
}

#[test]
fn quick_dirge_finishes_on_tick_363() {
    let (finished, traces) = dirge_until_finished(Tempo::Quick, 600);
    assert_eq!(finished, Some(363));
    assert_eq!(music_divisor(&traces[361]), Some(4561));
    assert_eq!(traces[362].voice, Voice::Silent);
}

#[test]
fn dirge_plays_with_music_off() {
    let mut driver = music_off_driver();
    driver.send(play(Tune::Dirge));
    let traces = driver.advance_ticks(513);
    // The opening rest, then the audible notes.
    assert!(traces[..21].iter().all(|t| t.voice == Voice::Silent));
    assert!(traces[21..512].iter().all(|t| music_divisor(t).is_some()));
    assert_eq!(driver.events(), [SoundEvent::DirgeFinished]);

    // The switch was never changed: after stop-music a normal-tune request
    // stays silent and frozen.
    driver.send(SoundCommand::MusicOff);
    driver.send(play(Tune::Main));
    let traces = driver.advance_ticks(200);
    assert!(traces.iter().all(|t| t.voice == Voice::Silent));
    let level = traces[0].music_level;
    assert!(traces.iter().all(|t| t.music_level == level));
}

#[test]
fn dirge_ends_death_effect() {
    let mut driver = new_driver();
    driver.send(SoundCommand::DiggerDeath);
    let death = driver.advance_ticks(11);
    assert!(death.iter().all(|t| t.effect_offer.is_some()));
    // The dirge starts on tick 12 of the death effect.
    driver.send(play(Tune::Dirge));
    let traces = driver.advance_ticks(100);
    assert!(traces.iter().all(|t| t.effect_offer.is_none()));
    // After its opening rest the dirge is heard.
    assert!(traces[21..].iter().all(|t| music_divisor(t).is_some()));
}

#[test]
fn dirge_event_reported_once() {
    // One pass through the dirge is 512 + 12 x 161 = 2444 ticks; by tick
    // 3000 its first rest has loaded a second time, but the finish of one
    // dirge is reported only once.
    let pass = tune_ticks(Tune::Dirge, Tempo::Standard);
    assert_eq!(pass, 2444);
    let mut driver = new_driver();
    driver.send(play(Tune::Dirge));
    driver.advance_ticks(3000);
    assert_eq!(driver.events(), [SoundEvent::DirgeFinished]);
    // A new dirge is a new occurrence.
    driver.send(play(Tune::Dirge));
    driver.advance_ticks(513);
    assert_eq!(driver.events(), [SoundEvent::DirgeFinished]);
}

// ---- The level-done jingle ----------------------------------------------

/// The jingle voice's carrier and beat on a tick.
fn jingle(trace: &TickTrace) -> Option<(Divisor, Hertz)> {
    match trace.voice {
        Voice::Jingle { carrier, beat } => Some((carrier, beat)),
        _ => None,
    }
}

#[test]
fn jingle_note_frequencies() {
    let mut driver = new_driver();
    driver.send(SoundCommand::LevelDone);
    let traces = driver.advance_ticks(231);
    let notes: Vec<_> = traces
        .chunks(21)
        .map(|note| {
            let first = jingle(&note[0]);
            assert!(note.iter().all(|t| jingle(t) == first));
            first
        })
        .collect();
    let carriers: Vec<u32> = notes
        .iter()
        .map(|n| n.map_or(0, |(carrier, _)| carrier.hertz().get()))
        .collect();
    let beats: Vec<u32> = notes
        .iter()
        .map(|n| n.map_or(0, |(_, beat)| beat.get()))
        .collect();
    assert_eq!(
        carriers,
        [515, 646, 766, 577, 684, 857, 646, 766, 959, 1015, 1015]
    );
    assert_eq!(beats, [8, 13, 17, 10, 14, 22, 13, 17, 28, 31, 31]);
    // The carrier divisor is the note's divisor plus 35.
    assert_eq!(notes[0].map(|(carrier, _)| carrier), Some(Divisor(2315)));
}

#[test]
fn jingle_lasts_231_ticks() {
    let mut driver = new_driver();
    driver.send(SoundCommand::LevelDone);
    let traces = driver.advance_ticks(230);
    assert!(traces.iter().all(|t| jingle(t).is_some()));
    assert_eq!(driver.events(), []);
    let last = driver.advance_ticks(1);
    assert!(jingle(&last[0]).is_some());
    assert_eq!(driver.events(), [SoundEvent::LevelDoneFinished]);
    // The jingle is over on tick 232.
    let after = driver.advance_ticks(100);
    assert!(after.iter().all(|t| t.voice == Voice::Silent));
    assert_eq!(driver.events(), []);
}

#[test]
fn jingle_freezes_effects_and_music() {
    let mut plain = new_driver();
    plain.send(play(Tune::Main));
    plain.send(SoundCommand::Gold);
    let reference = plain.advance_ticks(60);

    let mut driver = new_driver();
    driver.send(play(Tune::Main));
    driver.send(SoundCommand::Gold);
    let before = driver.advance_ticks(10);
    assert_eq!(before[..], reference[..10]);
    driver.send(SoundCommand::LevelDone);
    let during = driver.advance_ticks(231);
    // Nothing but the jingle sounds, and the jingle leaves the level at 50.
    assert!(during.iter().all(|t| jingle(t).is_some()));
    assert!(during.iter().all(|t| t.effect_offer.is_none()));
    assert!(during.iter().all(|t| t.music_level.get() == 50));

    // Afterwards gold resumes at its 11th tick (divisor 2100 ...) and the
    // tune at the tick where it was frozen.
    let after = driver.advance_ticks(50);
    assert_eq!(after[0].effect_offer, Some(Divisor(2100)));
    for (n, (got, want)) in after.iter().zip(&reference[10..]).enumerate() {
        assert_eq!(got.effect_offer, want.effect_offer, "tick {}", n + 11);
    }
    // Gold has ended by tick 22 of the resumed sequence (its 32nd), after
    // which the tune is heard on the same notes as without the jingle.
    let tune_after: Vec<_> = after[21..].iter().map(music_divisor).collect();
    let tune_reference: Vec<_> = reference[31..60].iter().map(music_divisor).collect();
    assert_eq!(tune_after, tune_reference);
}

#[test]
fn jingle_audible_before_any_music() {
    let mut driver = music_off_driver();
    driver.send(SoundCommand::LevelDone);
    let traces = driver.advance_ticks(231);
    assert!(
        traces
            .iter()
            .all(|t| t.voice.tone(SampleRate::DEFAULT).is_some())
    );
}

#[test]
fn stop_all_cuts_jingle_and_reports_once() {
    let mut driver = new_driver();
    driver.send(SoundCommand::LevelDone);
    driver.advance_ticks(49);
    assert_eq!(driver.events(), []);
    // Stop-all on tick 50 ends the jingle and reports it.
    driver.send(SoundCommand::StopAll);
    let traces = driver.advance_ticks(300);
    assert!(traces.iter().all(|t| t.voice == Voice::Silent));
    assert_eq!(driver.events(), [SoundEvent::LevelDoneFinished]);
    // A later stop-all has no jingle to cut.
    driver.send(SoundCommand::StopAll);
    driver.advance_ticks(1);
    assert_eq!(driver.events(), []);
    // A later music command plays normally.
    driver.send(play(Tune::Main));
    let traces = driver.advance_ticks(13);
    assert!(traces.iter().all(|t| music_divisor(t) == Some(4063)));
}
