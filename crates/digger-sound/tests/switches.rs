//! Tick-trace tests of the music and sound switches and of pause (task
//! group 6; design D4, D7, D12).
//!
//! Every test drives a [`ManualDriver`] tick by tick. Most compare a run
//! with a switch or a pause against a reference run without it, so they
//! check that state is frozen or kept exactly, not only that something
//! sounds. Ticks are counted from 1, the first being the tick on which the
//! first command takes effect.

#![allow(
    clippy::indexing_slicing,
    reason = "traces are indexed by tick; an out-of-range tick is a test failure"
)]

use digger_sound::{
    Divisor, ManualDriver, SoundCommand, SoundConfig, SoundControl, SoundEvent, SoundSink, Tempo,
    TickTrace, Tune, TuneTables, Voice, music::params,
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

/// A driver with sound switched off from the start.
fn quiet_driver() -> ManualDriver {
    driver_with(SoundConfig {
        sound_on: false,
        ..SoundConfig::default()
    })
}

/// The command that plays `tune` at the standard tempo.
fn play(tune: Tune) -> SoundCommand {
    SoundCommand::Music {
        tune,
        tempo: Tempo::Standard,
    }
}

/// The music divisor chosen on a tick, if music is the voice.
fn music_divisor(trace: &TickTrace) -> Option<u16> {
    match trace.voice {
        Voice::Music { divisor, .. } => Some(divisor.get()),
        _ => None,
    }
}

/// The voices chosen on `traces`.
fn voices(traces: &[TickTrace]) -> Vec<Voice> {
    traces.iter().map(|t| t.voice).collect()
}

/// The voices sent to the output on `traces`.
fn outputs(traces: &[TickTrace]) -> Vec<Voice> {
    traces.iter().map(|t| t.output).collect()
}

/// Whether nothing at all happens on `traces`: no voice, no offer.
fn all_silent(traces: &[TickTrace]) -> bool {
    traces
        .iter()
        .all(|t| t.voice == Voice::Silent && t.output == Voice::Silent && t.effect_offer.is_none())
}

/// The gold effect's 31 divisors (spec "Gold effect").
const GOLD: [u16; 31] = [
    4000, 531, 3516, 599, 3091, 675, 2717, 761, 2389, 858, 2100, 967, 1846, 1091, 1623, 1231, 1427,
    1388, 1255, 1566, 1104, 1766, 971, 1993, 855, 2249, 752, 2538, 661, 2864, 582,
];

/// The divisors offered on `traces`.
fn offers(traces: &[TickTrace]) -> Vec<Option<u16>> {
    traces
        .iter()
        .map(|t| t.effect_offer.map(Divisor::get))
        .collect()
}

// --- Music switch -------------------------------------------------------

#[test]
fn music_off_freezes_position() {
    // The first tick of the normal tune's 10th note, counted from 1.
    let notes = tables().notes(Tune::Main).to_vec();
    let main = params(Tune::Main);
    let tenth_start: usize = 1 + notes[..9]
        .iter()
        .map(|n| main.note_timing(n.units, Tempo::Standard).length() as usize)
        .sum::<usize>();
    let tenth = notes[9];
    assert!(
        !tenth.is_rest(),
        "the 10th note of the normal tune is a note"
    );
    // Switch off three ticks into the 10th note.
    let before = tenth_start + 2;

    let mut reference = new_driver();
    reference.send(play(Tune::Main));
    let ref_before = reference.advance_ticks(before);
    let ref_after = reference.advance_ticks(300);

    let mut driver = new_driver();
    driver.send(play(Tune::Main));
    let traces = driver.advance_ticks(before);
    assert_eq!(traces, ref_before);
    assert_eq!(
        music_divisor(&traces[before - 1]),
        Some(tenth.divisor.get())
    );

    driver.control(SoundControl::ToggleMusic);
    let off = driver.advance_ticks(200);
    assert!(!driver.synth().music_on());
    assert!(all_silent(&off), "music off is silent");
    // The envelope is frozen too.
    let level = traces[before - 1].music_level;
    assert!(off.iter().all(|t| t.music_level == level));

    driver.control(SoundControl::ToggleMusic);
    let on = driver.advance_ticks(300);
    assert!(driver.synth().music_on());
    // The tune resumes within its 10th note, exactly where it stopped.
    assert_eq!(music_divisor(&on[0]), Some(tenth.divisor.get()));
    assert_eq!(on, ref_after);
}

#[test]
fn music_off_keeps_effects_and_jingle() {
    // An effect over a tune that is switched off: the effect is heard.
    let mut driver = music_off_driver();
    driver.send(play(Tune::Main));
    driver.send(SoundCommand::Gold);
    let traces = driver.advance_ticks(40);
    assert_eq!(offers(&traces[..31]), GOLD.map(Some));
    assert_eq!(
        outputs(&traces[..31]),
        GOLD.map(|d| Voice::Effect(Divisor(d)))
    );
    assert!(all_silent(&traces[31..]), "the tune stays silent");

    // The jingle sounds the same with music on and off.
    let mut on = new_driver();
    let mut off = music_off_driver();
    on.send(SoundCommand::LevelDone);
    off.send(SoundCommand::LevelDone);
    let on_traces = on.advance_ticks(231);
    let off_traces = off.advance_ticks(231);
    assert_eq!(off_traces, on_traces);
    assert!(
        off_traces
            .iter()
            .all(|t| matches!(t.output, Voice::Jingle { .. }))
    );
    assert_eq!(off.events(), [SoundEvent::LevelDoneFinished]);

    // Toggling music during the jingle does not change it either.
    let mut toggled = new_driver();
    toggled.send(SoundCommand::LevelDone);
    let mut traces = toggled.advance_ticks(100);
    toggled.control(SoundControl::ToggleMusic);
    traces.extend(toggled.advance_ticks(131));
    assert_eq!(traces, on_traces);
}

#[test]
fn tune_requested_while_music_off_plays_when_enabled() {
    let mut reference = new_driver();
    reference.send(play(Tune::Bonus));
    let expected = reference.advance_ticks(200);

    let mut driver = music_off_driver();
    driver.send(play(Tune::Main));
    assert!(all_silent(&driver.advance_ticks(50)));
    driver.send(play(Tune::Bonus));
    assert!(all_silent(&driver.advance_ticks(20)));
    driver.control(SoundControl::ToggleMusic);
    let traces = driver.advance_ticks(200);
    // The bonus tune plays from its first note.
    assert_eq!(
        music_divisor(&traces[0]),
        Some(tables().notes(Tune::Bonus)[0].divisor.get())
    );
    assert_eq!(traces, expected);

    // Stop-music while off is honoured too: nothing plays when enabled.
    let mut driver = music_off_driver();
    driver.send(play(Tune::Main));
    driver.advance_ticks(5);
    driver.send(SoundCommand::MusicOff);
    driver.advance_ticks(5);
    driver.control(SoundControl::ToggleMusic);
    assert!(all_silent(&driver.advance_ticks(100)));
}

// --- Sound switch -------------------------------------------------------

#[test]
fn mute_does_not_freeze_state() {
    let mut reference = new_driver();
    reference.send(SoundCommand::BonusPulseStart);
    let expected = reference.advance_ticks(60);

    let mut driver = new_driver();
    driver.send(SoundCommand::BonusPulseStart);
    let mut traces = driver.advance_ticks(5);
    driver.control(SoundControl::ToggleSound);
    traces.extend(driver.advance_ticks(16));
    assert!(!driver.synth().sound_on());
    driver.control(SoundControl::ToggleSound);
    traces.extend(driver.advance_ticks(39));
    assert!(driver.synth().sound_on());

    // The siren's position advances while muted: the chosen voices and
    // offers are those of the unmuted reference on every tick...
    assert_eq!(voices(&traces), voices(&expected));
    assert_eq!(offers(&traces), offers(&expected));
    // ... and the siren really sounds during the muted stretch (ticks 6
    // to 21 hold its 1513 and 1230 phases) ...
    let muted = &traces[5..21];
    assert!(
        muted
            .iter()
            .any(|t| t.voice == Voice::Effect(Divisor(1513)))
    );
    assert!(
        muted
            .iter()
            .any(|t| t.voice == Voice::Effect(Divisor(1230)))
    );
    // ... but the output is silent for exactly those 16 ticks.
    for (i, trace) in traces.iter().enumerate() {
        let tick = i + 1;
        if (6..=21).contains(&tick) {
            assert_eq!(trace.output, Voice::Silent, "tick {tick}");
        } else {
            assert_eq!(trace.output, trace.voice, "tick {tick}");
        }
    }
}

#[test]
fn sound_on_forces_music_on() {
    let mut driver = new_driver();
    driver.send(play(Tune::Main));
    driver.advance_ticks(3);
    driver.control(SoundControl::ToggleMusic);
    driver.control(SoundControl::ToggleSound);
    driver.advance_ticks(1);
    assert!(!driver.synth().music_on());
    assert!(!driver.synth().sound_on());

    driver.control(SoundControl::ToggleSound);
    let traces = driver.advance_ticks(1);
    // Both read as on within one tick, and the tune is heard again.
    assert!(driver.synth().sound_on());
    assert!(driver.synth().music_on());
    assert!(matches!(traces[0].output, Voice::Music { .. }));

    // Switching sound off does not touch the music switch.
    driver.control(SoundControl::ToggleSound);
    driver.advance_ticks(1);
    assert!(driver.synth().music_on());
}

#[test]
fn started_quiet_can_be_enabled() {
    let mut reference = new_driver();
    reference.send(play(Tune::Main));
    let expected = reference.advance_ticks(400);

    let mut driver = quiet_driver();
    assert!(!driver.synth().sound_on());
    driver.send(play(Tune::Main));
    let quiet = driver.advance_ticks(250);
    assert!(outputs(&quiet).iter().all(|&v| v == Voice::Silent));
    assert_eq!(voices(&quiet), voices(&expected[..250]));

    driver.control(SoundControl::ToggleSound);
    let heard = driver.advance_ticks(150);
    assert!(driver.synth().sound_on());
    // The tune is heard at the position it has reached.
    assert_eq!(heard, expected[250..]);
    assert!(
        heard
            .iter()
            .any(|t| matches!(t.output, Voice::Music { .. }))
    );
}

// --- Pause --------------------------------------------------------------

#[test]
fn pause_freezes_gold_at_tick_10_and_resumes_at_11() {
    let mut driver = new_driver();
    driver.send(SoundCommand::Gold);
    let first = driver.advance_ticks(10);
    assert_eq!(
        offers(&first),
        GOLD[..10].iter().map(|&d| Some(d)).collect::<Vec<_>>()
    );

    driver.send(SoundCommand::Pause);
    let paused = driver.advance_ticks(500);
    assert!(driver.synth().paused());
    assert!(all_silent(&paused), "the output is silent during the pause");

    driver.send(SoundCommand::Resume);
    let rest = driver.advance_ticks(30);
    assert!(!driver.synth().paused());
    // The first tick after resuming is tick 11 of the gold effect.
    assert_eq!(rest[0].voice, Voice::Effect(Divisor(2100)));
    assert_eq!(
        offers(&rest[..21]),
        GOLD[10..].iter().map(|&d| Some(d)).collect::<Vec<_>>()
    );
    assert!(all_silent(&rest[21..]));
}

#[test]
fn stop_all_during_pause() {
    let mut driver = new_driver();
    driver.send(play(Tune::Main));
    driver.send(SoundCommand::Gold);
    driver.send(SoundCommand::BonusPulseStart);
    driver.send(SoundCommand::FallStart);
    driver.advance_ticks(10);
    driver.send(SoundCommand::Pause);
    driver.advance_ticks(5);
    driver.send(SoundCommand::StopAll);
    assert!(all_silent(&driver.advance_ticks(5)));
    driver.send(SoundCommand::Resume);
    // Nothing sounds when the pause ends.
    assert!(all_silent(&driver.advance_ticks(200)));

    // A jingle stopped during a pause is reported, and does not resume.
    let mut driver = new_driver();
    driver.send(SoundCommand::LevelDone);
    driver.advance_ticks(10);
    driver.send(SoundCommand::Pause);
    driver.advance_ticks(3);
    driver.send(SoundCommand::StopAll);
    driver.advance_ticks(1);
    assert_eq!(driver.events(), [SoundEvent::LevelDoneFinished]);
    driver.send(SoundCommand::Resume);
    assert!(all_silent(&driver.advance_ticks(300)));
    assert_eq!(driver.events(), []);
}

#[test]
fn pause_is_idempotent() {
    let mut reference = new_driver();
    reference.send(play(Tune::Main));
    reference.send(SoundCommand::Gold);
    let expected = reference.advance_ticks(100);

    let mut driver = new_driver();
    // Resuming when not paused does nothing.
    driver.send(SoundCommand::Resume);
    driver.send(play(Tune::Main));
    driver.send(SoundCommand::Gold);
    let mut traces = driver.advance_ticks(3);
    // Pausing twice needs only one resume.
    driver.send(SoundCommand::Pause);
    driver.send(SoundCommand::Pause);
    assert!(all_silent(&driver.advance_ticks(10)));
    driver.send(SoundCommand::Pause);
    assert!(all_silent(&driver.advance_ticks(10)));
    driver.send(SoundCommand::Resume);
    traces.extend(driver.advance_ticks(40));
    // A second resume does nothing either.
    driver.send(SoundCommand::Resume);
    traces.extend(driver.advance_ticks(57));
    assert_eq!(traces, expected);
}

#[test]
fn pause_is_separate_from_the_jingle_freeze() {
    let mut reference = new_driver();
    reference.send(play(Tune::Main));
    reference.send(SoundCommand::LevelDone);
    let expected = reference.advance_ticks(300);
    let expected_events = reference.events();

    // A pause during the jingle freezes the jingle itself; resuming
    // continues it and leaves effects and music frozen until it ends.
    let mut driver = new_driver();
    driver.send(play(Tune::Main));
    driver.send(SoundCommand::LevelDone);
    let mut traces = driver.advance_ticks(50);
    driver.send(SoundCommand::Pause);
    assert!(all_silent(&driver.advance_ticks(100)));
    assert_eq!(driver.events(), []);
    driver.send(SoundCommand::Resume);
    traces.extend(driver.advance_ticks(250));
    assert_eq!(traces, expected);
    assert_eq!(driver.events(), expected_events);
    assert_eq!(expected_events, [SoundEvent::LevelDoneFinished]);
}

#[test]
fn sound_off_during_pause_stays_off_after_it() {
    let mut driver = new_driver();
    driver.send(SoundCommand::Gold);
    driver.advance_ticks(2);
    driver.send(SoundCommand::Pause);
    driver.control(SoundControl::ToggleSound);
    driver.advance_ticks(4);
    driver.send(SoundCommand::Resume);
    let traces = driver.advance_ticks(1);
    assert_eq!(traces[0].voice, Voice::Effect(Divisor(3516)));
    assert_eq!(traces[0].output, Voice::Silent);
}
