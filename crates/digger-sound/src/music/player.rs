//! The tune player (design D4; research 04 §3.1, §3.3).
//!
//! [`MusicPlayer`] holds the tune being played, if any, and the envelope
//! level. The level lives outside the tune because the original never
//! resets it: it is carried from note to note and from tune to tune, it is
//! 8 when the engine is created and the level-done jingle leaves it at 50
//! (kept quirk SQ6).

use super::envelope::{EnvelopeStage, NoteTiming, TuneParams, envelope_step, params};
use super::tables::{Note, TuneTables};
use crate::command::{Tempo, Tune};
use crate::units::{Divisor, Level};

/// The music level the engine starts with (kept quirk SQ6; research 04
/// §1.3).
pub const INITIAL_MUSIC_LEVEL: Level = Level::saturating(8);

/// A tune being played.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Playing {
    tune: Tune,
    tempo: Tempo,
    params: TuneParams,
    /// The index of the note loaded next.
    next: usize,
    /// Ticks left in the current note after this one; 0 means the next
    /// tick loads a note (upstream's `noteduration`).
    ticks_left: u32,
    /// Ticks of the current note played so far, counted from 1.
    note_ticks: u32,
    /// The tick of the note from which the release runs.
    gate: u32,
    /// The current note's divisor.
    divisor: Divisor,
    stage: EnvelopeStage,
    /// Whether the dirge's finish has been reported since it started.
    dirge_reported: bool,
}

impl Playing {
    /// `tune` at `tempo`, about to load its first note.
    const fn new(tune: Tune, tempo: Tempo) -> Self {
        Self {
            tune,
            tempo,
            params: params(tune),
            next: 0,
            ticks_left: 0,
            note_ticks: 0,
            gate: 0,
            divisor: Note::REST,
            stage: EnvelopeStage::Attack,
            dirge_reported: false,
        }
    }

    /// Loads the next note of `notes`; returns whether this is the moment
    /// the dirge's audible part has finished (its first rest after the
    /// first note).
    fn load(&mut self, notes: &[Note]) -> bool {
        let index = if self.next < notes.len() {
            self.next
        } else {
            0
        };
        let note = notes.get(index).copied().unwrap_or(Note {
            divisor: Note::REST,
            units: 1,
        });
        let NoteTiming { countdown, gate } = self.params.note_timing(note.units, self.tempo);
        self.ticks_left = countdown;
        self.gate = gate;
        self.note_ticks = 0;
        self.stage = EnvelopeStage::Attack;
        self.divisor = note.divisor;
        self.next = if index + 1 < notes.len() {
            index + 1
        } else {
            0
        };
        let finished =
            self.tune == Tune::Dirge && index > 0 && note.is_rest() && !self.dirge_reported;
        self.dirge_reported |= finished;
        finished
    }
}

/// What the player did on one tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct MusicTick {
    /// The divisor the tune sounds on this tick, or `None` on a rest, at
    /// level 1 and when no tune plays.
    pub sounding: Option<Divisor>,
    /// Whether the dirge's audible part finished on this tick.
    pub dirge_finished: bool,
}

/// The tune player: the tune being played, if any, and the envelope level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MusicPlayer {
    current: Option<Playing>,
    level: Level,
}

impl Default for MusicPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl MusicPlayer {
    /// No tune, level 8.
    pub const fn new() -> Self {
        Self {
            current: None,
            level: INITIAL_MUSIC_LEVEL,
        }
    }

    /// Starts `tune` from its first note, which loads on the next tick.
    /// Any tune playing is replaced; the level is kept.
    pub fn play(&mut self, tune: Tune, tempo: Tempo) {
        self.current = Some(Playing::new(tune, tempo));
    }

    /// Ends the tune, forgetting its position; the level is kept.
    pub fn stop(&mut self) {
        self.current = None;
    }

    /// The tune being played, if any.
    pub fn tune(&self) -> Option<Tune> {
        self.current.map(|playing| playing.tune)
    }

    /// The envelope level.
    pub const fn level(&self) -> Level {
        self.level
    }

    /// Sets the envelope level (the level-done jingle sets 50).
    pub fn set_level(&mut self, level: Level) {
        self.level = level;
    }

    /// Advances the tune by one tick, reading its notes from `tables`.
    ///
    /// A tick either loads the next note (when the current one is over) or
    /// counts the current one down; then the envelope takes one step, the
    /// release starting from the tick on which the note reaches its gate.
    /// Rests run the envelope like notes but sound nothing, and nothing
    /// sounds while the level is 1.
    pub fn tick(&mut self, tables: &TuneTables) -> MusicTick {
        let Some(playing) = self.current.as_mut() else {
            return MusicTick::default();
        };
        let mut dirge_finished = false;
        if playing.ticks_left == 0 {
            dirge_finished = playing.load(tables.notes(playing.tune));
        } else {
            playing.ticks_left -= 1;
        }
        playing.note_ticks += 1;
        let gated = playing.note_ticks >= playing.gate;
        (playing.stage, self.level) =
            envelope_step(playing.params, playing.stage, self.level, gated);
        let sounding =
            (playing.divisor != Note::REST && self.level != Level::MIN).then_some(playing.divisor);
        MusicTick {
            sounding,
            dirge_finished,
        }
    }
}
