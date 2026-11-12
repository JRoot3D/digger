//! The DRF and eDRF recorders (game-recording spec, "Writing a DRF
//! recording", "Writing an eDRF recording", "Re-recording and conversion
//! between formats"; design D9 of `add-recording-and-conformance`).
//!
//! A recorder only observes: it is told every hook with the value the game
//! actually used and returns that value unchanged. It can therefore watch
//! a live game directly (`game.step_observed(&input, &mut recorder)`), or
//! ride inside a [`Replay`](super::Replay) to re-record a playback, which
//! converts between the formats.

use core::fmt::Write as _;

use crate::recording::{ControlValue, RecordingKind, WRITER_VERSION_LINE, mode_line_for};
use crate::state_hash;
use crate::{
    Controls, Direction, FinishReason, GameConfig, LevelLayout, LevelNumber, LevelSet, Movement,
    Observer, PlayerId, SlotId, StateView,
};

/// The longest run a DRF recorder writes; a longer one is split (quirk
/// P15).
pub const DRF_MAX_RUN: u32 = 999;
/// The DRF recorder starts a new line once the run text of the current one
/// reaches this many columns (quirk P15).
pub const DRF_WRAP_COLUMNS: u32 = 60;
/// The most ticks one eDRF `I` record covers; the controls are written
/// whenever the tick count is a multiple of this (quirk P14).
pub const EDRF_CHUNK: u32 = 32;
/// Map rows of a level.
const MAP_ROWS: usize = 10;
/// Number of level plans a DRF header holds.
const DRF_PLANS: usize = 8;
/// The eDRF tick checkpoint cadence (quirk P12).
const CHECKPOINT_EVERY: u32 = super::CHECKPOINT_EVERY;

/// The two header lines after the first, shared by both formats: version
/// and mode, then the bonus score; each ends in LF.
fn common_header(config: &GameConfig) -> String {
    format!(
        "{WRITER_VERSION_LINE}\n{}\n{}\n",
        mode_line_for(config.mode, config.start_level),
        config.levels.bonus_step()
    )
}

/// A map byte as it is written: the printable characters as they are,
/// anything else (a NUL, a control byte) as plain earth.
const fn map_char(byte: u8) -> char {
    if byte > b' ' && byte <= b'~' {
        byte as char
    } else {
        ' '
    }
}

/// The layout a level is played on: one supplied in place of the stored
/// one is used verbatim; otherwise the configuration's own, with the
/// two-digger start cells opened where the level set asks for it (quirks
/// P6, Q07).
fn layout_as_played(
    levels: &LevelSet,
    two_diggers: bool,
    level: LevelNumber,
    layout: &LevelLayout,
    replaced: bool,
) -> LevelLayout {
    if replaced {
        *layout
    } else {
        levels.layout_as_played(level.plan(), two_diggers)
    }
}

/// Whether the game is over because it was quit.
fn was_quit(state: StateView<'_>) -> bool {
    state.finish_reason() == Some(FinishReason::Quit)
}

/// Records a game as DRF text (design D9).
///
/// The header comes from the configuration: `DRF`, the version line
/// [`WRITER_VERSION_LINE`], the mode line, the bonus score and the eight
/// level plans of 15 characters per row. Then each round adds its seed in
/// eight upper-case hex digits, one run-length encoded record per control
/// read and `EOL`; `EOG` follows the last `EOL` when the game was quit.
/// Lines end in LF on every platform and the text has no length limit
/// (upstream's 256 KB buffer wrapped silently; fixed).
#[derive(Debug, Clone)]
pub struct DrfRecorder {
    /// The first four header lines.
    header: String,
    /// The plans written into the header: the configuration's, unless a
    /// playback supplied a level's map, in which case the first map
    /// supplied for a plan.
    maps: [LevelLayout; DRF_PLANS],
    /// Which plans have been set up so far.
    plans_seen: [bool; DRF_PLANS],
    /// The body written so far.
    body: String,
    /// The run being counted: its letter and length.
    run: Option<(char, u32)>,
    /// The column counter of the current body line (upstream's `reccc`).
    column: u32,
}

impl DrfRecorder {
    /// A recorder for a game started under `config`.
    pub fn new(config: &GameConfig) -> Self {
        let mut header = String::from(crate::recording::DRF_FIRST_LINE_TEXT);
        header.push('\n');
        header.push_str(&common_header(config));
        Self {
            header,
            maps: *config.levels.layouts(),
            plans_seen: [false; DRF_PLANS],
            body: String::new(),
            run: None,
            column: 0,
        }
    }

    /// The run letter of a record: `s r u l d` for stop, right, up, left
    /// and down, upper case with fire.
    const fn letter(movement: Movement) -> char {
        let letter = match movement.direction {
            None => 's',
            Some(Direction::Right) => 'r',
            Some(Direction::Up) => 'u',
            Some(Direction::Left) => 'l',
            Some(Direction::Down) => 'd',
        };
        if movement.fire {
            letter.to_ascii_uppercase()
        } else {
            letter
        }
    }

    /// Writes the run being counted, if any, and wraps the line once its
    /// column counter reaches [`DRF_WRAP_COLUMNS`] (quirk P15: each run
    /// counts 1 plus the digits of its count).
    fn put_run(&mut self) {
        let Some((letter, count)) = self.run.take() else {
            return;
        };
        self.body.push(letter);
        self.column = self.column.saturating_add(1);
        if count > 1 {
            let digits = count.to_string();
            self.body.push_str(&digits);
            self.column = self
                .column
                .saturating_add(u32::try_from(digits.len()).unwrap_or(u32::MAX));
        }
        if self.column >= DRF_WRAP_COLUMNS {
            self.body.push('\n');
            self.column = 0;
        }
    }

    /// Adds one record to the run being counted, writing that run first
    /// when the record differs or the run is [`DRF_MAX_RUN`] long.
    fn put_record(&mut self, letter: char) {
        match &mut self.run {
            Some((current, count)) if *current == letter && *count < DRF_MAX_RUN => {
                *count = count.saturating_add(1);
            }
            _ => {
                self.put_run();
                self.run = Some((letter, 1));
            }
        }
    }

    /// Notes that `level` is set up on `layout`. A layout the playback
    /// supplied (`replaced`) goes into the header for its plan, unless the
    /// plan was set up before.
    pub(crate) fn note_level(&mut self, level: LevelNumber, layout: &LevelLayout, replaced: bool) {
        let index = usize::from(level.plan().number()).saturating_sub(1);
        if let (Some(seen), Some(map)) = (self.plans_seen.get_mut(index), self.maps.get_mut(index))
        {
            if !*seen && replaced {
                *map = *layout;
            }
            *seen = true;
        }
    }

    /// Appends the initials entered for the high-score table, as the line
    /// `*abc` (each missing initial written as a space).
    pub fn add_initials(&mut self, initials: &str) {
        let mut chars = initials.chars();
        self.body.push('*');
        for _ in 0..3 {
            self.body.push(chars.next().unwrap_or(' '));
        }
        self.body.push('\n');
    }

    /// The recording as written so far: the header, the maps and the
    /// body. Complete once the game has ended.
    pub fn text(&self) -> String {
        let mut text = self.header.clone();
        for map in &self.maps {
            for row in map.rows() {
                text.extend(row.iter().map(|&byte| map_char(byte)));
                text.push('\n');
            }
        }
        text.push_str(&self.body);
        text
    }
}

impl Observer for DrfRecorder {
    fn round_seed(&mut self, _round: u32, derived: i32) -> i32 {
        let _ = writeln!(self.body, "{:08X}", derived.cast_unsigned());
        self.run = None;
        self.column = 0;
        derived
    }

    fn level_plan(
        &mut self,
        _player: PlayerId,
        level: LevelNumber,
        layout: &LevelLayout,
    ) -> Option<LevelLayout> {
        self.note_level(level, layout, false);
        None
    }

    fn movement(&mut self, _slot: SlotId, sample: Movement) -> Movement {
        self.put_record(Self::letter(sample));
        sample
    }

    fn round_end(&mut self, _state: StateView<'_>) {
        self.put_run();
        if self.column > 0 {
            self.body.push('\n');
            self.column = 0;
        }
        self.body.push_str("EOL\n");
    }

    fn game_end(&mut self, state: StateView<'_>) {
        if was_quit(state) {
            self.body.push_str("EOG\n");
        }
    }
}

/// One slot's controls on one tick of an eDRF chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotSample {
    /// The slot was read and held these controls.
    Read(ControlValue),
    /// No digger plays from the slot: it was not read. It agrees with any
    /// value when the chunk form is chosen (quirk P14).
    Unread,
}

impl SlotSample {
    /// The value written for it: an unread slot's is `@`.
    const fn value(self) -> ControlValue {
        match self {
            Self::Read(value) => value,
            Self::Unread => ControlValue::NONE,
        }
    }
}

/// Where an eDRF recorder takes a read slot's value from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ControlSource {
    /// The raw controls the slot was given (a live game, or an eDRF
    /// playback).
    Raw,
    /// The direction and fire a DRF playback supplied at the slot's
    /// control read, and nothing on a tick without one (game-recording
    /// spec, "Re-recording and conversion between formats").
    DrfMovement,
}

/// Records a game as eDRF text, streamed as the game goes (design D9).
///
/// The header lines are written at construction. Then come an `L` record
/// each time a level is set up (a map written before is referred to by
/// number), `R` with each round's seed, every slot's controls in `I`
/// records of at most [`EDRF_CHUNK`] ticks, `C` checkpoints, `E` round
/// ends, `Q` when the game was quit and `Z` at the end. Pending controls
/// are written before any other record. Finished lines collect until the
/// caller drains them with [`take_pending`](Self::take_pending); any
/// prefix drained at a chunk boundary is a playable, truncated recording.
#[derive(Debug, Clone)]
pub struct EdrfRecorder {
    /// The level set and mode, to write the maps as played.
    levels: LevelSet,
    two_diggers: bool,
    /// Slots below this number are in play and read on every tick.
    slots_in_play: usize,
    source: ControlSource,
    /// This tick's value of each slot, as the controls hook or (DRF
    /// playback) the movement hook gave it.
    this_tick: [ControlValue; SlotId::COUNT],
    /// The current chunk of each slot.
    chunk: [Vec<SlotSample>; SlotId::COUNT],
    /// The distinct maps written, in order.
    maps: Vec<LevelLayout>,
    /// The slot written in the `Q` record of a quit game, or none.
    quit_slot: Option<SlotId>,
    /// Lines not yet taken by the caller.
    pending: String,
}

impl EdrfRecorder {
    /// A recorder for a game started under `config` whose controls are
    /// live or come from an eDRF playback.
    pub fn new(config: &GameConfig) -> Self {
        Self::with_source(config, ControlSource::Raw)
    }

    /// A recorder for a DRF playback of a game under `config`: a read
    /// slot's value is the direction and fire the DRF supplied at its
    /// control read, 0 on a tick without one (upstream's `edrf_movement`
    /// overwrite). This is what converts a DRF to eDRF.
    pub fn for_drf_playback(config: &GameConfig) -> Self {
        Self::with_source(config, ControlSource::DrfMovement)
    }

    fn with_source(config: &GameConfig, source: ControlSource) -> Self {
        let mut pending = String::from(crate::recording::EDRF_FIRST_LINE_TEXT);
        pending.push('\n');
        pending.push_str(&common_header(config));
        let chunk_len = usize::try_from(EDRF_CHUNK).unwrap_or(0);
        Self {
            levels: config.levels,
            two_diggers: config.mode.two_diggers(),
            slots_in_play: usize::from(config.mode.diggers()),
            source,
            this_tick: [ControlValue::NONE; SlotId::COUNT],
            chunk: [Vec::with_capacity(chunk_len), Vec::with_capacity(chunk_len)],
            maps: Vec::new(),
            quit_slot: SlotId::new(0),
            pending,
        }
    }

    /// Sets the slot the `Q` record of a quit game names (slot 0 unless
    /// set), or none for no `Q` record: a re-recorded eDRF playback that
    /// ran out names the slot its recording names.
    pub fn set_quit_slot(&mut self, slot: Option<SlotId>) {
        self.quit_slot = slot;
    }

    /// Removes and returns the lines written since the last call (the
    /// header lines on the first call).
    pub fn take_pending(&mut self) -> String {
        core::mem::take(&mut self.pending)
    }

    /// The control bits of a DRF record.
    fn movement_value(movement: Movement) -> ControlValue {
        let mut controls = Controls::NONE;
        match movement.direction {
            Some(Direction::Up) => controls.up = true,
            Some(Direction::Down) => controls.down = true,
            Some(Direction::Left) => controls.left = true,
            Some(Direction::Right) => controls.right = true,
            None => {}
        }
        controls.fire = movement.fire;
        ControlValue::from_controls(controls)
    }

    /// The value every slot agrees on at chunk tick `i`, an unread slot
    /// agreeing with anything; `None` when two read slots differ. A tick
    /// no slot read gives 0.
    fn agreed(&self, i: usize) -> Option<ControlValue> {
        let mut agreed: Option<ControlValue> = None;
        for chunk in &self.chunk {
            match chunk.get(i).copied() {
                Some(SlotSample::Read(value)) => match agreed {
                    Some(other) if other != value => return None,
                    _ => agreed = Some(value),
                },
                Some(SlotSample::Unread) | None => {}
            }
        }
        Some(agreed.unwrap_or(ControlValue::NONE))
    }

    /// Writes ` ` and the run list of `values`, a run ending wherever the
    /// key changes.
    fn put_runs<T: PartialEq + Copy>(
        out: &mut String,
        values: impl Iterator<Item = T>,
        value: impl Fn(T) -> ControlValue,
    ) {
        out.push(' ');
        let mut run: Option<(T, u32)> = None;
        let put = |out: &mut String, (key, count): (T, u32)| {
            out.push(char::from(value(key).to_char()));
            if count > 1 {
                let _ = write!(out, "{count}");
            }
        };
        for key in values {
            match &mut run {
                Some((current, count)) if *current == key => *count = count.saturating_add(1),
                _ => {
                    if let Some(done) = run.take() {
                        put(out, done);
                    }
                    run = Some((key, 1));
                }
            }
        }
        if let Some(done) = run {
            put(out, done);
        }
    }

    /// Writes the controls of the chunk so far, if it holds any tick, and
    /// starts a new chunk (quirk P14).
    ///
    /// When every slot agrees on every tick the record has no slot list
    /// and one run list. Otherwise it lists each slot that holds a
    /// control on some tick and gives one run list per listed slot; there
    /// a run ends wherever the sample changes, so an unread tick and a
    /// read 0 make two runs although both print `@`.
    fn flush_controls(&mut self) {
        let len = self.chunk.first().map_or(0, Vec::len);
        if len > 0 {
            self.pending.push('I');
            let agreed: Option<Vec<ControlValue>> = (0..len).map(|i| self.agreed(i)).collect();
            if let Some(values) = agreed {
                Self::put_runs(&mut self.pending, values.into_iter(), |v| v);
            } else {
                let listed: Vec<bool> = self
                    .chunk
                    .iter()
                    .map(|chunk| chunk.iter().any(|s| s.value() != ControlValue::NONE))
                    .collect();
                for (slot, listed) in SlotId::ALL.iter().zip(&listed) {
                    if *listed {
                        let _ = write!(self.pending, "{}", slot.number());
                    }
                }
                for (chunk, listed) in self.chunk.iter().zip(&listed) {
                    if *listed {
                        Self::put_runs(&mut self.pending, chunk.iter().copied(), SlotSample::value);
                    }
                }
            }
            self.pending.push('\n');
        }
        for chunk in &mut self.chunk {
            chunk.clear();
        }
    }

    /// Writes a hash record (`C` or `E`) after the pending controls.
    fn put_state(&mut self, what: char, hash: u32) {
        self.flush_controls();
        let _ = writeln!(self.pending, "{what} {hash:08X}");
    }

    /// Writes the `L` record of `level` set up on `layout` (`replaced`
    /// when a playback supplied it, which is then written as it is).
    pub(crate) fn note_level(&mut self, level: LevelNumber, layout: &LevelLayout, replaced: bool) {
        self.flush_controls();
        let map = layout_as_played(&self.levels, self.two_diggers, level, layout, replaced);
        // Written maps are compared as written: unprintable bytes as earth.
        let written = |layout: &LevelLayout| layout.rows().map(|row| row.map(map_char));
        let key = written(&map);
        if let Some(index) = self.maps.iter().position(|m| written(m) == key) {
            let _ = writeln!(
                self.pending,
                "L {} {}",
                level.get(),
                index.saturating_add(1)
            );
            return;
        }
        self.maps.push(map);
        let _ = writeln!(self.pending, "L {}", level.get());
        for row in key.iter().take(MAP_ROWS) {
            self.pending.push(' ');
            self.pending.extend(row.iter());
            self.pending.push('\n');
        }
    }
}

impl Observer for EdrfRecorder {
    fn round_seed(&mut self, _round: u32, derived: i32) -> i32 {
        self.flush_controls();
        let _ = writeln!(self.pending, "R {:08X}", derived.cast_unsigned());
        derived
    }

    fn level_plan(
        &mut self,
        _player: PlayerId,
        level: LevelNumber,
        layout: &LevelLayout,
    ) -> Option<LevelLayout> {
        self.note_level(level, layout, false);
        None
    }

    fn controls(&mut self, _tick: u32, slot: SlotId, raw: Controls) -> Controls {
        let value = match self.source {
            ControlSource::Raw => ControlValue::from_controls(raw),
            // Filled in by the movement hook at a control read.
            ControlSource::DrfMovement => ControlValue::NONE,
        };
        *slot.of_mut(&mut self.this_tick) = value;
        raw
    }

    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement {
        if self.source == ControlSource::DrfMovement {
            *slot.of_mut(&mut self.this_tick) = Self::movement_value(sample);
        }
        sample
    }

    fn tick_end(&mut self, tick: u32, state: StateView<'_>) {
        for (index, (chunk, value)) in self.chunk.iter_mut().zip(&mut self.this_tick).enumerate() {
            let sample = if index < self.slots_in_play {
                SlotSample::Read(*value)
            } else {
                SlotSample::Unread
            };
            chunk.push(sample);
            *value = ControlValue::NONE;
        }
        if tick.is_multiple_of(EDRF_CHUNK) {
            self.flush_controls();
        }
        if tick.is_multiple_of(CHECKPOINT_EVERY) {
            self.put_state('C', state_hash::hash(state));
        }
    }

    fn round_end(&mut self, state: StateView<'_>) {
        let tick = state.ticks();
        self.put_state('E', state_hash::hash_at_round_end(state, tick));
    }

    fn game_end(&mut self, state: StateView<'_>) {
        self.flush_controls();
        if was_quit(state)
            && let Some(slot) = self.quit_slot
        {
            let _ = writeln!(self.pending, "Q {}", slot.number());
        }
        let _ = writeln!(
            self.pending,
            "Z {} {:08X}",
            state.ticks(),
            state_hash::hash(state)
        );
    }
}

/// A recorder of either format, as a [`Replay`](super::Replay) holds one.
#[derive(Debug, Clone)]
pub enum Recorder {
    /// Writing DRF.
    Drf(DrfRecorder),
    /// Writing eDRF.
    Edrf(EdrfRecorder),
}

impl Recorder {
    /// A recorder of `kind` for a game under `config`; `drf_playback`
    /// when the game is a DRF being played back (an eDRF recorder then
    /// takes the slots' values from the DRF's records).
    pub fn new(kind: RecordingKind, config: &GameConfig, drf_playback: bool) -> Self {
        match kind {
            RecordingKind::Drf => Self::Drf(DrfRecorder::new(config)),
            RecordingKind::Edrf if drf_playback => {
                Self::Edrf(EdrfRecorder::for_drf_playback(config))
            }
            RecordingKind::Edrf => Self::Edrf(EdrfRecorder::new(config)),
        }
    }

    /// The format being written.
    pub const fn kind(&self) -> RecordingKind {
        match self {
            Self::Drf(_) => RecordingKind::Drf,
            Self::Edrf(_) => RecordingKind::Edrf,
        }
    }

    /// The text not yet taken: for an eDRF the lines since the last call,
    /// for a DRF (which is written whole) the whole recording so far.
    pub fn take_pending(&mut self) -> String {
        match self {
            Self::Drf(drf) => drf.text(),
            Self::Edrf(edrf) => edrf.take_pending(),
        }
    }

    /// Notes a level set up, with whether a playback supplied its layout.
    pub(crate) fn note_level(&mut self, level: LevelNumber, layout: &LevelLayout, replaced: bool) {
        match self {
            Self::Drf(drf) => drf.note_level(level, layout, replaced),
            Self::Edrf(edrf) => edrf.note_level(level, layout, replaced),
        }
    }

    fn observer(&mut self) -> &mut dyn Observer {
        match self {
            Self::Drf(drf) => drf,
            Self::Edrf(edrf) => edrf,
        }
    }
}

impl Observer for Recorder {
    fn round_seed(&mut self, round: u32, derived: i32) -> i32 {
        self.observer().round_seed(round, derived)
    }

    fn level_plan(
        &mut self,
        player: PlayerId,
        level: LevelNumber,
        layout: &LevelLayout,
    ) -> Option<LevelLayout> {
        self.observer().level_plan(player, level, layout)
    }

    fn controls(&mut self, tick: u32, slot: SlotId, raw: Controls) -> Controls {
        self.observer().controls(tick, slot, raw)
    }

    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement {
        self.observer().movement(slot, sample)
    }

    fn tick_end(&mut self, tick: u32, state: StateView<'_>) {
        self.observer().tick_end(tick, state);
    }

    fn round_end(&mut self, state: StateView<'_>) {
        self.observer().round_end(state);
    }

    fn game_end(&mut self, state: StateView<'_>) {
        self.observer().game_end(state);
    }
}
