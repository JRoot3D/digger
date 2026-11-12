//! The game and audio halves of a linked engine on two threads (task
//! group 8; design D8, fixed quirk SF8).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use digger_sound::{
    FireballId, SoundCommand, SoundConfig, SoundControl, SoundSink, Tempo, Tune, TuneTables,
    Volume, sound_link,
};

/// How many inputs the game thread sends.
const SENDS: u64 = 10_000;

/// The longest the whole exchange may take before it counts as a deadlock.
const TIMEOUT: Duration = Duration::from_secs(60);

/// The `i`-th input of a fixed mix of commands and controls.
fn input(i: u64, sink: &mut dyn SoundSink) {
    let [zero, one] = FireballId::ALL;
    let fireball = if (i / 16).is_multiple_of(2) {
        zero
    } else {
        one
    };
    match i % 16 {
        0 => sink.send(SoundCommand::FallStart),
        1 => sink.send(SoundCommand::FallStop),
        2 => sink.send(SoundCommand::Fire(fireball)),
        3 => sink.send(SoundCommand::Explode(fireball)),
        4 => sink.send(SoundCommand::Gold),
        5 => sink.send(SoundCommand::EmeraldClick),
        6 => sink.send(SoundCommand::Music {
            tune: Tune::Main,
            tempo: Tempo::Standard,
        }),
        7 => sink.send(SoundCommand::LevelDone),
        8 => sink.control(SoundControl::ToggleMusic),
        9 => sink.control(SoundControl::ToggleSound),
        10 => sink.control(SoundControl::SetVolume(Volume::new(
            u8::try_from(i % 101).unwrap_or(100),
        ))),
        11 => sink.control(SoundControl::SetFilter(i.is_multiple_of(3))),
        12 => sink.send(SoundCommand::Pause),
        13 => sink.send(SoundCommand::Resume),
        14 => sink.send(SoundCommand::DiggerDeath),
        _ => sink.send(SoundCommand::StopAll),
    }
}

/// Sends the mix from one thread while another fills 512-sample buffers,
/// and returns (sent, applied, dropped).
#[expect(clippy::expect_used, reason = "a failure here is a test failure")]
fn exchange() -> (u64, u64, u64) {
    let tables = TuneTables::embedded().expect("embedded tables decode");
    let (mut handle, mut source) =
        sound_link(SoundConfig::default(), &tables).expect("valid configuration");
    let done = Arc::new(AtomicBool::new(false));

    let audio_done = Arc::clone(&done);
    let audio = thread::spawn(move || {
        let mut buffer = [0_i16; 512];
        while !audio_done.load(Ordering::Acquire) {
            source.fill(&mut buffer);
        }
        // Everything sent is in the ring now; two more buffers include at
        // least one tick (606 samples), which drains it.
        source.fill(&mut buffer);
        source.fill(&mut buffer);
        source.applied()
    });

    for i in 0..SENDS {
        input(i, &mut handle);
        // Events are read now and then, as a frontend would.
        while handle.poll_event().is_some() {}
        if i % 64 == 0 {
            let _ = handle.status();
            thread::yield_now();
        }
    }
    done.store(true, Ordering::Release);

    let applied = audio.join().expect("the audio thread does not panic");
    let dropped = u64::from(handle.status().dropped);
    (SENDS, applied, dropped)
}

#[test]
#[expect(clippy::expect_used, reason = "a failure here is a test failure")]
fn two_thread_smoke() {
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = exchange();
        let _ = tx.send(result);
    });
    let (sent, applied, dropped) = rx
        .recv_timeout(TIMEOUT)
        .expect("the exchange finishes without deadlock or panic");
    worker.join().expect("the test thread does not panic");
    assert_eq!(
        sent,
        applied + dropped,
        "applied {applied}, dropped {dropped}"
    );
}
