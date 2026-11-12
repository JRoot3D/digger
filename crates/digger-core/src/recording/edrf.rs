//! The extended recording format, `eDRF 2`: the four header lines, then one
//! record per line in game order (game-recording spec, "eDRF record
//! grammar", "eDRF control values and input records", "eDRF inline level
//! maps", "End of an eDRF recording"; research 05 §5.2).
//!
//! Unlike a DRF body, an eDRF body is indexed by tick and slot, so it is
//! read eagerly, before play starts, into one queue per kind of record
//! (design D3). Control runs are kept as `(value, count)` pairs and used up
//! by counting down: a run of four thousand million costs one pair.
//!
//! [`EdrfRecords`] reads the records one at a time, which is what a tool
//! that copies or cuts a recording needs; [`EdrfRecording::parse`] folds
//! them into the queues playback uses.

use std::collections::VecDeque;

use super::error::RecordingError;
use super::header::{Header, Line, LineReader, RecordingKind};
use crate::sim::{Controls, LevelLayout, SlotId};

/// Number of input slots an eDRF recording holds controls for.
pub const EDRF_SLOTS: usize = SlotId::COUNT;
/// Rows of one inline level map.
const MAP_ROWS: usize = 10;
/// Map characters of one row.
const MAP_COLUMNS: usize = 15;
/// Hex digits of a hash or a seed.
const HEX_DIGITS: usize = 8;
/// The character for "no controls"; a control value is this plus its bits.
const FIRST_CONTROL_CHAR: u8 = b'@';
/// The highest control character, `'@' + 31`.
const LAST_CONTROL_CHAR: u8 = b'_';

/// Bits of a control value (research 05 §5.2, `input.h`).
const UP: u8 = 1;
const DOWN: u8 = 2;
const RIGHT: u8 = 4;
const LEFT: u8 = 8;
const FIRE: u8 = 16;

/// The raw controls of one slot on one tick, as an eDRF control character
/// stores them: up = 1, down = 2, right = 4, left = 8, fire = 16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ControlValue(u8);

impl ControlValue {
    /// No control held (`@`).
    pub const NONE: Self = Self(0);

    /// The value with these bits, or `None` above 31.
    pub const fn new(bits: u8) -> Option<Self> {
        if bits <= LAST_CONTROL_CHAR - FIRST_CONTROL_CHAR {
            Some(Self(bits))
        } else {
            None
        }
    }

    /// The value a control character stands for, or `None` outside
    /// `'@'..='_'`.
    pub const fn from_char(byte: u8) -> Option<Self> {
        if byte >= FIRST_CONTROL_CHAR && byte <= LAST_CONTROL_CHAR {
            Some(Self(byte - FIRST_CONTROL_CHAR))
        } else {
            None
        }
    }

    /// The control character of this value.
    pub const fn to_char(self) -> u8 {
        FIRST_CONTROL_CHAR + self.0
    }

    /// The bits, 0 to 31.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// The controls this value holds.
    pub const fn controls(self) -> Controls {
        Controls {
            up: self.0 & UP != 0,
            down: self.0 & DOWN != 0,
            left: self.0 & LEFT != 0,
            right: self.0 & RIGHT != 0,
            fire: self.0 & FIRE != 0,
        }
    }

    /// The value holding these controls.
    pub const fn from_controls(controls: Controls) -> Self {
        let mut bits = 0;
        if controls.up {
            bits |= UP;
        }
        if controls.down {
            bits |= DOWN;
        }
        if controls.right {
            bits |= RIGHT;
        }
        if controls.left {
            bits |= LEFT;
        }
        if controls.fire {
            bits |= FIRE;
        }
        Self(bits)
    }
}

impl From<ControlValue> for Controls {
    fn from(value: ControlValue) -> Self {
        value.controls()
    }
}

/// One value repeated for a number of ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Run {
    /// The control value.
    pub value: ControlValue,
    /// How many ticks it lasts, at least 1.
    pub count: u64,
}

/// The run lists of one `I` record.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InputRecord {
    /// No slot list: one run list for every slot (`I @8D13`).
    All(Vec<Run>),
    /// A slot list: the run list of each listed slot, `None` for a slot
    /// not listed (`I1 D4H2`, `I01 D3 @2P`).
    Slots([Option<Vec<Run>>; EDRF_SLOTS]),
}

impl InputRecord {
    /// The ticks the run list of `runs` covers.
    fn ticks_of(runs: &[Run]) -> u64 {
        runs.iter()
            .fold(0_u64, |sum, run| sum.saturating_add(run.count))
    }

    /// The runs `slot` receives from this record: its own list, the
    /// common list, or, for a slot not listed, "no controls" for as many
    /// ticks as the first listed slot's list covers.
    pub fn runs_for(&self, slot: SlotId) -> Vec<Run> {
        match self {
            Self::All(runs) => runs.clone(),
            Self::Slots(lists) => {
                if let Some(runs) = slot.of(lists) {
                    return runs.clone();
                }
                let first = lists.iter().flatten().next();
                let ticks = first.map_or(0, |runs| Self::ticks_of(runs));
                if ticks == 0 {
                    Vec::new()
                } else {
                    vec![Run {
                        value: ControlValue::NONE,
                        count: ticks,
                    }]
                }
            }
        }
    }
}

/// The map of an `L` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LevelMap {
    /// A map given inline in the ten lines after the record.
    Inline(LevelLayout),
    /// The n-th (1-based) map given inline earlier in the recording.
    Reuse(usize),
}

/// An `L` record: the level that starts and its map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LevelRecord {
    /// The level number as recorded.
    pub level: u32,
    /// The map, inline or by reference.
    pub map: LevelMap,
}

/// One eDRF record (research 05 §5.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Record {
    /// `I`: the controls of the ticks since the previous record.
    Input(InputRecord),
    /// `L`: a level starts.
    Level(LevelRecord),
    /// `R`: the random-number seed of the round that starts.
    Seed(u32),
    /// `C`: the state hash on a tick whose count is a multiple of 256.
    Checkpoint(u32),
    /// `E`: the state hash at the end of a round.
    RoundEnd(u32),
    /// `Q`: the game was quit by this slot's player.
    Quit(SlotId),
    /// `Z`: the tick total and the state hash at game end.
    End(EndRecord),
}

/// The `Z` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EndRecord {
    /// The game's tick total.
    pub ticks: u32,
    /// The state hash at game end.
    pub hash: u32,
}

/// The bad-record error for `line`.
fn bad(line: &Line<'_>) -> RecordingError {
    RecordingError::BadRecord {
        line: line.number,
        text: String::from_utf8_lossy(line.text).into_owned(),
    }
}

/// A decimal number of at least one digit and nothing else, fitting `u32`.
fn decimal(text: &[u8]) -> Option<u32> {
    if text.is_empty() || !text.iter().all(u8::is_ascii_digit) {
        return None;
    }
    text.iter().try_fold(0_u32, |value, digit| {
        value.checked_mul(10)?.checked_add(u32::from(digit - b'0'))
    })
}

/// Exactly eight hexadecimal digits of either case and nothing else.
fn hex8(text: &[u8]) -> Option<u32> {
    if text.len() != HEX_DIGITS {
        return None;
    }
    text.iter().try_fold(0_u32, |value, &byte| {
        let digit = char::from(byte).to_digit(16)?;
        Some((value << 4) | digit)
    })
}

/// One run list: control characters, each with an optional decimal count
/// from 1 to 4294967295. `None` when empty or malformed.
fn run_list(text: &[u8]) -> Option<Vec<Run>> {
    let mut runs = Vec::new();
    let mut rest = text;
    while let Some((&byte, tail)) = rest.split_first() {
        let value = ControlValue::from_char(byte)?;
        let digits = tail.iter().take_while(|b| b.is_ascii_digit()).count();
        let (count_text, tail) = tail.split_at_checked(digits)?;
        let count = if count_text.is_empty() {
            1
        } else {
            match decimal(count_text)? {
                0 => return None,
                count => count,
            }
        };
        runs.push(Run {
            value,
            count: u64::from(count),
        });
        rest = tail;
    }
    (!runs.is_empty()).then_some(runs)
}

/// The body of an `I` record, after the `I`.
fn input_record(text: &[u8]) -> Option<InputRecord> {
    let digits = text.iter().take_while(|b| b.is_ascii_digit()).count();
    let (slot_text, rest) = text.split_at_checked(digits)?;
    let mut listed = [false; EDRF_SLOTS];
    let mut last = None;
    for &digit in slot_text {
        let slot = usize::from(digit - b'0');
        if slot >= EDRF_SLOTS || last.is_some_and(|last| slot <= last) {
            return None;
        }
        *listed.get_mut(slot)? = true;
        last = Some(slot);
    }
    let rest = rest.strip_prefix(b" ")?;
    let mut fields = rest.split(|&b| b == b' ');
    let record = if last.is_none() {
        InputRecord::All(run_list(fields.next()?)?)
    } else {
        let mut lists: [Option<Vec<Run>>; EDRF_SLOTS] = Default::default();
        for (list, _) in lists.iter_mut().zip(listed).filter(|(_, l)| *l) {
            *list = Some(run_list(fields.next()?)?);
        }
        InputRecord::Slots(lists)
    };
    // A run list too many.
    fields.next().is_none().then_some(record)
}

/// The text after a one-letter record's `"X "`.
fn argument(text: &[u8], letter: u8) -> Option<&[u8]> {
    text.strip_prefix(&[letter, b' '])
}

/// A record that fits on one line; `None` for `L` and anything bad.
fn one_line_record(text: &[u8]) -> Option<Record> {
    match text.first()? {
        b'I' => input_record(text.get(1..)?).map(Record::Input),
        b'R' => hex8(argument(text, b'R')?).map(Record::Seed),
        b'C' => hex8(argument(text, b'C')?).map(Record::Checkpoint),
        b'E' => hex8(argument(text, b'E')?).map(Record::RoundEnd),
        b'Q' => match argument(text, b'Q')? {
            b"0" => SlotId::new(0).map(Record::Quit),
            b"1" => SlotId::new(1).map(Record::Quit),
            _ => None,
        },
        b'Z' => {
            let rest = argument(text, b'Z')?;
            let space = rest.iter().position(|&b| b == b' ')?;
            let (ticks, hash) = rest.split_at_checked(space)?;
            Some(Record::End(EndRecord {
                ticks: decimal(ticks)?,
                hash: hex8(hash.get(1..)?)?,
            }))
        }
        _ => None,
    }
}

/// The level number and reused map index of an `L` line.
fn level_line(text: &[u8]) -> Option<(u32, Option<u32>)> {
    let rest = argument(text, b'L')?;
    match rest.iter().position(|&b| b == b' ') {
        None => Some((decimal(rest)?, None)),
        Some(space) => {
            let (level, index) = rest.split_at_checked(space)?;
            Some((decimal(level)?, Some(decimal(index.get(1..)?)?)))
        }
    }
}

/// One row of an inline map: an empty line, or a space and up to 15 map
/// characters, padded with earth.
fn map_row(text: &[u8]) -> Option<[u8; MAP_COLUMNS]> {
    let chars = if text.is_empty() {
        text
    } else {
        text.strip_prefix(b" ")?
    };
    if chars.len() > MAP_COLUMNS {
        return None;
    }
    let mut row = [b' '; MAP_COLUMNS];
    for (cell, &byte) in row.iter_mut().zip(chars) {
        *cell = byte;
    }
    Some(row)
}

/// Reads the records of an eDRF body one by one, each with the number of
/// its (first) line.
///
/// Blank lines are skipped, except inside a map. A last line without a
/// line end is dropped as incomplete and marks the recording as truncated
/// ([`EdrfRecords::truncated`]). After a `Z` record only blank lines may
/// follow. The first error ends the iteration.
#[derive(Debug, Clone)]
pub struct EdrfRecords<'a> {
    lines: LineReader<'a>,
    inline_maps: usize,
    ended: bool,
    truncated: bool,
    done: bool,
}

impl<'a> EdrfRecords<'a> {
    /// A reader over the lines after the header.
    pub(crate) const fn new(lines: LineReader<'a>) -> Self {
        Self {
            lines,
            inline_maps: 0,
            ended: false,
            truncated: false,
            done: false,
        }
    }

    /// Whether the data ended in a line without a line end, which was
    /// dropped. Meaningful once the iteration has ended.
    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    /// The next line that is part of the recording: `None` at the end of
    /// the data or for a last line without a line end.
    fn next_complete_line(&mut self) -> Option<Line<'a>> {
        let line = self.lines.next_line()?;
        if line.terminated {
            Some(line)
        } else {
            self.truncated = true;
            None
        }
    }

    /// The ten rows after the `L` line `head`.
    fn inline_map(&mut self, head: &Line<'_>) -> Result<LevelLayout, RecordingError> {
        let mut rows = [[b' '; MAP_COLUMNS]; MAP_ROWS];
        for row in &mut rows {
            // A missing row is a bad `L` record, however the data ends.
            let line = self.next_complete_line().ok_or_else(|| bad(head))?;
            *row = map_row(line.text).ok_or_else(|| bad(&line))?;
        }
        Ok(LevelLayout::from_rows(rows))
    }

    /// The record that starts with `line`.
    fn record(&mut self, line: &Line<'_>) -> Result<Record, RecordingError> {
        if self.ended {
            return Err(bad(line));
        }
        if line.text.first() != Some(&b'L') {
            return one_line_record(line.text).ok_or_else(|| bad(line));
        }
        let (level, reuse) = level_line(line.text).ok_or_else(|| bad(line))?;
        let map = match reuse {
            None => {
                let layout = self.inline_map(line)?;
                self.inline_maps = self.inline_maps.saturating_add(1);
                LevelMap::Inline(layout)
            }
            Some(index) => {
                let index = usize::try_from(index).map_err(|_| bad(line))?;
                if index == 0 || index > self.inline_maps {
                    return Err(bad(line));
                }
                LevelMap::Reuse(index)
            }
        };
        Ok(Record::Level(LevelRecord { level, map }))
    }
}

impl Iterator for EdrfRecords<'_> {
    type Item = Result<(u32, Record), RecordingError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        loop {
            let Some(line) = self.next_complete_line() else {
                self.done = true;
                return None;
            };
            if line.text.is_empty() {
                continue;
            }
            let result = self.record(&line);
            match &result {
                Ok(Record::End(_)) => self.ended = true,
                Ok(_) => {}
                Err(_) => self.done = true,
            }
            return Some(result.map(|record| (line.number, record)));
        }
    }
}

/// The controls of one slot still to be played, as runs used up by
/// counting down.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ControlRuns {
    runs: VecDeque<Run>,
    remaining: u64,
}

impl ControlRuns {
    /// An empty queue.
    pub const fn new() -> Self {
        Self {
            runs: VecDeque::new(),
            remaining: 0,
        }
    }

    /// Appends runs at the end.
    pub fn extend(&mut self, runs: impl IntoIterator<Item = Run>) {
        for run in runs {
            if run.count > 0 {
                self.remaining = self.remaining.saturating_add(run.count);
                self.runs.push_back(run);
            }
        }
    }

    /// Takes the value of the next tick, or `None` when used up.
    pub fn next_value(&mut self) -> Option<ControlValue> {
        let run = self.runs.front_mut()?;
        let value = run.value;
        run.count = run.count.saturating_sub(1);
        if run.count == 0 {
            self.runs.pop_front();
        }
        self.remaining = self.remaining.saturating_sub(1);
        Some(value)
    }

    /// The ticks still held.
    pub const fn remaining(&self) -> u64 {
        self.remaining
    }

    /// Whether every value is used up.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// The runs still held, next first.
    pub fn runs(&self) -> impl Iterator<Item = &Run> {
        self.runs.iter()
    }
}

/// A level that starts during the game, with its map resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdrfLevel {
    /// The level number as recorded.
    pub level: u32,
    /// The map to play it on, exactly as recorded.
    pub layout: LevelLayout,
    /// The line of the `L` record.
    pub line: u32,
}

/// An eDRF recording read into one queue per kind of record (design D3).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EdrfRecording {
    header: Header,
    controls: [ControlRuns; EDRF_SLOTS],
    seeds: Vec<u32>,
    levels: Vec<EdrfLevel>,
    maps: Vec<LevelLayout>,
    checkpoints: Vec<u32>,
    round_ends: Vec<u32>,
    quit: Option<SlotId>,
    end: Option<EndRecord>,
    truncated: bool,
}

impl EdrfRecording {
    /// Reads an eDRF recording: the four header lines and every record.
    ///
    /// # Errors
    ///
    /// Those of [`Header::parse`]; [`RecordingError::NotARecording`] also
    /// for a DRF file; [`RecordingError::BadRecord`] for the first line
    /// that matches no record form, for an `L` record whose map is
    /// incomplete or refers to a map not given, and for any record after
    /// `Z`.
    pub fn parse(bytes: &[u8]) -> Result<Self, RecordingError> {
        let mut lines = LineReader::new(bytes);
        let header = Header::read(&mut lines)?;
        if header.kind() != RecordingKind::Edrf {
            return Err(RecordingError::NotARecording);
        }
        let mut recording = Self {
            header,
            controls: Default::default(),
            seeds: Vec::new(),
            levels: Vec::new(),
            maps: Vec::new(),
            checkpoints: Vec::new(),
            round_ends: Vec::new(),
            quit: None,
            end: None,
            truncated: false,
        };
        let mut records = EdrfRecords::new(lines);
        for item in &mut records {
            let (line, record) = item?;
            recording.add(line, record);
        }
        recording.truncated = records.truncated();
        Ok(recording)
    }

    /// Adds one record to its queue. A reused map index has been checked
    /// by the reader.
    fn add(&mut self, line: u32, record: Record) {
        match record {
            Record::Input(input) => {
                for (slot, queue) in SlotId::ALL.into_iter().zip(&mut self.controls) {
                    queue.extend(input.runs_for(slot));
                }
            }
            Record::Level(LevelRecord { level, map }) => {
                let layout = match map {
                    LevelMap::Inline(layout) => {
                        self.maps.push(layout);
                        Some(layout)
                    }
                    LevelMap::Reuse(index) => {
                        index.checked_sub(1).and_then(|i| self.maps.get(i)).copied()
                    }
                };
                if let Some(layout) = layout {
                    self.levels.push(EdrfLevel {
                        level,
                        layout,
                        line,
                    });
                }
            }
            Record::Seed(seed) => self.seeds.push(seed),
            Record::Checkpoint(hash) => self.checkpoints.push(hash),
            Record::RoundEnd(hash) => self.round_ends.push(hash),
            Record::Quit(slot) => self.quit = Some(slot),
            Record::End(end) => self.end = Some(end),
        }
    }

    /// The header lines.
    pub const fn header(&self) -> &Header {
        &self.header
    }

    /// The recorded controls of `slot`, first tick first.
    pub fn controls(&self, slot: SlotId) -> &ControlRuns {
        slot.of(&self.controls)
    }

    /// The round seeds, first round first.
    pub fn seeds(&self) -> &[u32] {
        &self.seeds
    }

    /// The levels started, in order, with their maps resolved.
    pub fn levels(&self) -> &[EdrfLevel] {
        &self.levels
    }

    /// The distinct maps given inline, in order; `L n m` refers to the
    /// m-th of them.
    pub fn maps(&self) -> &[LevelLayout] {
        &self.maps
    }

    /// The tick checkpoint hashes (`C`), in order.
    pub fn checkpoints(&self) -> &[u32] {
        &self.checkpoints
    }

    /// The round-end hashes (`E`), in order.
    pub fn round_ends(&self) -> &[u32] {
        &self.round_ends
    }

    /// The slot whose player quit (`Q`), if the recording says.
    pub const fn quit(&self) -> Option<SlotId> {
        self.quit
    }

    /// The end record (`Z`), absent when the recording was cut short.
    pub const fn end(&self) -> Option<EndRecord> {
        self.end
    }

    /// Whether the data ended in a line without a line end (dropped).
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "eDRF 2\nMS MPL 20190212\n1\n20000\n";

    /// The records of `body` after a standard header.
    fn records(body: &str) -> Result<Vec<Record>, RecordingError> {
        let text = format!("{HEADER}{body}");
        let mut lines = LineReader::new(text.as_bytes());
        Header::read(&mut lines)?;
        EdrfRecords::new(lines)
            .map(|item| item.map(|(_, record)| record))
            .collect()
    }

    fn parse(body: &str) -> Result<EdrfRecording, RecordingError> {
        EdrfRecording::parse(format!("{HEADER}{body}").as_bytes())
    }

    fn bad_record(line: u32, text: &str) -> RecordingError {
        RecordingError::BadRecord {
            line,
            text: text.to_owned(),
        }
    }

    fn slot(n: u8) -> SlotId {
        SlotId::new(n).unwrap_or(SlotId::ALL[0])
    }

    fn run(c: u8, count: u64) -> Run {
        Run {
            value: ControlValue::from_char(c).unwrap_or_default(),
            count,
        }
    }

    /// Every value of a queue, expanded (only for short test queues).
    fn values(runs: &ControlRuns) -> String {
        let mut runs = runs.clone();
        let mut out = String::new();
        while let Some(value) = runs.next_value() {
            out.push(char::from(value.to_char()));
        }
        out
    }

    #[test]
    fn accepted_one_line_records() {
        let cases = [
            ("R 00000000", Record::Seed(0)),
            ("R 0001e240", Record::Seed(0x0001_E240)),
            ("C FEB235D5", Record::Checkpoint(0xFEB2_35D5)),
            ("E 0123ABCD", Record::RoundEnd(0x0123_ABCD)),
            ("Q 0", Record::Quit(slot(0))),
            ("Q 1", Record::Quit(slot(1))),
            (
                "Z 1024 DEADBEEF",
                Record::End(EndRecord {
                    ticks: 1024,
                    hash: 0xDEAD_BEEF,
                }),
            ),
            ("Z 0 00000000", Record::End(EndRecord { ticks: 0, hash: 0 })),
            (
                "Z 4294967295 FFFFFFFF",
                Record::End(EndRecord {
                    ticks: u32::MAX,
                    hash: u32::MAX,
                }),
            ),
            ("I @", Record::Input(InputRecord::All(vec![run(b'@', 1)]))),
            (
                "I _4294967295",
                Record::Input(InputRecord::All(vec![run(b'_', 4_294_967_295)])),
            ),
            (
                "I0 D3",
                Record::Input(InputRecord::Slots([Some(vec![run(b'D', 3)]), None])),
            ),
        ];
        for (line, record) in cases {
            assert_eq!(records(&format!("{line}\n")), Ok(vec![record]), "{line}");
        }
    }

    #[test]
    fn bad_record_forms() {
        for text in [
            "X 1234",
            "C 12G4",
            "C 1 2",
            "Q 2",
            "Z 100",
            "I0",
            "C",
            "C ",
            "C 1234567",
            "C 123456789",
            "C  FEB235D5",
            "CFEB235D5",
            "C FEB235D5 ",
            "R 0000000G",
            "E 1",
            "Q",
            "Q 00",
            "Q -1",
            "Q 0 x",
            "Z 100 ",
            "Z  100 DEADBEEF",
            "Z 100 DEADBEEF x",
            "Z 4294967296 DEADBEEF",
            "Z x DEADBEEF",
            " ",
            "i @",
            "I",
            "I ",
            "I@",
            "I @0",
            "I @x",
            "I `",
            "I ?",
            "I @  @",
            "I @ ",
            "I @ @",
            "I @4294967296",
            "I2 @",
            "I00 @",
            "I10 D3 @3",
            "I01 D3",
            "I0 D3 @3",
            "I1 D3 @3",
            "L",
            "L ",
            "L x",
            "L 1 0",
            "L 1 1",
            "L 1 2 3",
            "L 1 x",
            "L 1  1",
        ] {
            // Four header lines and 52 seeds put the record on line 57.
            let body = format!("{}{text}\n", "R 00000000\n".repeat(52));
            let error = parse(&body).err();
            assert_eq!(error, Some(bad_record(57, text)), "{text:?}");
            let message = error.map(|e| e.to_string()).unwrap_or_default();
            assert!(message.contains("57"), "{message}");
            assert!(message.contains(&format!("\"{text}\"")), "{message}");
        }
    }

    #[test]
    fn bad_record_line_number_counts_header_and_crlf() {
        let text = format!("{HEADER}R 00000000\n\nX 1234\n").replace('\n', "\r\n");
        assert_eq!(
            EdrfRecording::parse(text.as_bytes()).err(),
            Some(bad_record(7, "X 1234"))
        );
    }

    #[test]
    fn crlf_equals_lf() {
        let body = "L 1\n S\n\n\n\n\n\n\n\n\n HH\nR 00000001\nI @3D\nC 0000ABCD\nE 00000002\nQ 0\nZ 4 00000003\n";
        let lf = parse(body);
        let crlf = EdrfRecording::parse(format!("{HEADER}{body}").replace('\n', "\r\n").as_bytes());
        assert!(lf.is_ok());
        assert_eq!(lf, crlf);
    }

    #[test]
    fn blank_lines_are_skipped() {
        assert_eq!(
            records("\n\nR 00000001\n\r\n\nC 00000002\n\n"),
            Ok(vec![Record::Seed(1), Record::Checkpoint(2)])
        );
    }

    #[test]
    fn unterminated_last_line() {
        let recording = parse("R 00000001\nI @3\nI @8D13");
        let (truncated, ticks) = recording
            .as_ref()
            .map(|r| (r.truncated(), r.controls(slot(0)).remaining()))
            .unwrap_or_default();
        assert!(truncated);
        assert_eq!(ticks, 3);
        // A bad line without a line end is dropped too.
        assert_eq!(parse("R 00000001\nX 1234").map(|r| r.truncated()), Ok(true));
        // Terminated: not truncated.
        assert_eq!(parse("R 00000001\n").map(|r| r.truncated()), Ok(false));
        assert_eq!(parse("").map(|r| r.truncated()), Ok(false));
    }

    #[test]
    fn record_after_the_end_record() {
        assert_eq!(
            parse("Z 1 00000000\nR 00000000\n").err(),
            Some(bad_record(6, "R 00000000"))
        );
        assert_eq!(
            parse("Z 1 00000000\nZ 1 00000000\n").err(),
            Some(bad_record(6, "Z 1 00000000"))
        );
        // Blank lines may follow.
        let end = parse("Q 0\nZ 1 00000000\n\n\r\n").map(|r| (r.quit(), r.end()));
        assert_eq!(
            end,
            Ok((Some(slot(0)), Some(EndRecord { ticks: 1, hash: 0 })))
        );
    }

    #[test]
    fn control_characters() {
        let decoded: Vec<Controls> = b"@ABDHPQT"
            .iter()
            .filter_map(|&c| ControlValue::from_char(c))
            .map(ControlValue::controls)
            .collect();
        let none = Controls::NONE;
        assert_eq!(
            decoded,
            [
                none,
                Controls { up: true, ..none },
                Controls { down: true, ..none },
                Controls {
                    right: true,
                    ..none
                },
                Controls { left: true, ..none },
                Controls { fire: true, ..none },
                Controls {
                    fire: true,
                    up: true,
                    ..none
                },
                Controls {
                    fire: true,
                    right: true,
                    ..none
                },
            ]
        );
        for bits in 0..32 {
            let value = ControlValue::new(bits);
            assert_eq!(value.map(ControlValue::bits), Some(bits));
            let value = value.unwrap_or_default();
            assert_eq!(ControlValue::from_controls(value.controls()), value);
            assert_eq!(ControlValue::from_char(value.to_char()), Some(value));
        }
        assert_eq!(ControlValue::new(32), None);
        assert_eq!(ControlValue::from_char(b'?'), None);
        assert_eq!(ControlValue::from_char(b'`'), None);
    }

    #[test]
    fn record_for_all_slots() {
        let recording = parse("I @8D13A5@3P2@\n");
        let expected = format!(
            "{}{}{}{}{}{}",
            "@".repeat(8),
            "D".repeat(13),
            "A".repeat(5),
            "@".repeat(3),
            "PP",
            "@"
        );
        assert_eq!(expected.len(), 32);
        for n in 0..2 {
            let got = recording.as_ref().map(|r| values(r.controls(slot(n))));
            assert_eq!(got.as_deref(), Ok(expected.as_str()), "slot {n}");
        }
    }

    #[test]
    fn record_for_one_slot() {
        let recording = parse("I1 D4H2\n");
        let got = recording
            .as_ref()
            .map(|r| (values(r.controls(slot(0))), values(r.controls(slot(1)))));
        assert_eq!(got, Ok(("@@@@@@".to_owned(), "DDDDHH".to_owned())));
    }

    #[test]
    fn record_for_both_slots_separately() {
        let recording = parse("I01 D3 @2P\n");
        let got = recording
            .as_ref()
            .map(|r| (values(r.controls(slot(0))), values(r.controls(slot(1)))));
        assert_eq!(got, Ok(("DDD".to_owned(), "@@P".to_owned())));
    }

    #[test]
    fn slot_list_padding_follows_first_listed_slot() {
        // Slot 0 unlisted; padded to slot 1's 3 ticks.
        let recording = parse("I1 @2P\nI0 D\n");
        let got = recording
            .as_ref()
            .map(|r| (values(r.controls(slot(0))), values(r.controls(slot(1)))));
        assert_eq!(got, Ok(("@@@D".to_owned(), "@@P@".to_owned())));
        // Both listed with different lengths: kept as given.
        let recording = parse("I01 D3 @\n");
        let got = recording
            .as_ref()
            .map(|r| (values(r.controls(slot(0))), values(r.controls(slot(1)))));
        assert_eq!(got, Ok(("DDD".to_owned(), "@".to_owned())));
    }

    #[test]
    fn wrong_number_of_run_lists() {
        for text in ["I01 D3", "I10 D3 @3", "I0 D3 @3", "I @ @", "I01 D3 @3 @3"] {
            assert_eq!(
                parse(&format!("{text}\n")).err(),
                Some(bad_record(5, text)),
                "{text}"
            );
        }
    }

    #[test]
    fn zero_count_is_bad() {
        for text in ["I @0", "I D3@0D", "I0 @00", "I01 D @0"] {
            assert_eq!(
                parse(&format!("{text}\n")).err(),
                Some(bad_record(5, text)),
                "{text}"
            );
        }
        // Leading zeros of a non-zero count are allowed.
        assert_eq!(
            parse("I @007\n").map(|r| r.controls(slot(0)).remaining()),
            Ok(7)
        );
    }

    #[test]
    fn edrf_huge_count_is_not_expanded() {
        let recording = parse("I @4000000000\n");
        let mut runs = recording
            .as_ref()
            .map(|r| r.controls(slot(0)).clone())
            .unwrap_or_default();
        assert_eq!(runs.remaining(), 4_000_000_000);
        assert_eq!(runs.runs().count(), 1);
        for _ in 0..3 {
            assert_eq!(runs.next_value(), Some(ControlValue::NONE));
        }
        assert_eq!(runs.remaining(), 3_999_999_997);
        assert_eq!(runs.runs().count(), 1);
        // Padding of an unlisted slot is one run as well, even past u32.
        let recording = parse("I0 @4000000000@4000000000\n");
        let padded = recording.as_ref().map(|r| {
            let runs = r.controls(slot(1));
            (runs.remaining(), runs.runs().count())
        });
        assert_eq!(padded, Ok((8_000_000_000, 1)));
    }

    #[test]
    fn control_runs_queue() {
        let mut runs = ControlRuns::new();
        assert!(runs.is_empty());
        assert_eq!(runs.next_value(), None);
        runs.extend([run(b'A', 2), run(b'B', 0), run(b'D', 1)]);
        assert_eq!(runs.remaining(), 3);
        assert_eq!(runs.runs().count(), 2);
        assert_eq!(values(&runs), "AAD");
        while runs.next_value().is_some() {}
        assert!(runs.is_empty());
        assert_eq!(runs.remaining(), 0);
    }

    /// The ten rows of a map as lines, each prefixed with a space.
    fn map_lines(rows: [&str; 10]) -> String {
        rows_text(&rows)
    }

    /// Rows as map lines, each a space, the row and a line end.
    fn rows_text(rows: &[&str]) -> String {
        let mut out = String::new();
        for row in rows {
            out.push(' ');
            out.push_str(row);
            out.push('\n');
        }
        out
    }

    const ROWS: [&str; 10] = [
        "S   B     HHHHS",
        "V  CC  C  V B",
        "VB CC  C  V",
        "V  CCB CB V CCC",
        "V  CC  C  V CCC",
        "HH CC  C  V CCC",
        " V    B B V",
        " HHHH     V",
        "C   V     V   C",
        "CC  HHHHHHH  CC",
    ];

    fn padded(rows: [&str; 10]) -> LevelLayout {
        let mut out = [[b' '; MAP_COLUMNS]; MAP_ROWS];
        for (target, row) in out.iter_mut().zip(rows) {
            for (cell, byte) in target.iter_mut().zip(row.bytes()) {
                *cell = byte;
            }
        }
        LevelLayout::from_rows(out)
    }

    #[test]
    fn edrf_inline_map() {
        let recording = parse(&format!("L 1\n{}", map_lines(ROWS)));
        let levels = recording.as_ref().map(|r| r.levels().to_vec());
        assert_eq!(
            levels,
            Ok(vec![EdrfLevel {
                level: 1,
                layout: padded(ROWS),
                line: 5,
            }])
        );
        // An empty line inside a map is a row of earth, not skipped; a row
        // of one space is earth too; any byte is kept as given.
        let mut rows = ROWS;
        rows[2] = "";
        rows[3] = "xyz";
        let mut text = format!("L 2\n{}R 00000000\n", map_lines(rows));
        text = text.replacen("\n \n", "\n\n", 1);
        let recording = parse(&text);
        let got = recording
            .as_ref()
            .map(|r| (r.levels().first().map(|l| l.layout), r.seeds().to_vec()));
        assert_eq!(got, Ok((Some(padded(rows)), vec![0])));
        assert_eq!(
            padded(rows).rows().get(2),
            Some(&[b' '; MAP_COLUMNS]),
            "earth row"
        );
    }

    #[test]
    fn edrf_map_reuse() {
        let mut other = ROWS;
        other[0] = "SSSSSSSSSSSSSSS";
        let text = format!(
            "L 1\n{}L 2\n{}L 3 1\nL 4 2\nL 5 1\n",
            map_lines(ROWS),
            map_lines(other)
        );
        let recording = parse(&text);
        let got = recording.as_ref().map(|r| {
            (
                r.maps().len(),
                r.levels()
                    .iter()
                    .map(|l| (l.level, l.layout, l.line))
                    .collect::<Vec<_>>(),
            )
        });
        let (a, b) = (padded(ROWS), padded(other));
        assert_eq!(
            got,
            Ok((
                2,
                vec![(1, a, 5), (2, b, 16), (3, a, 27), (4, b, 28), (5, a, 29)]
            ))
        );
        // The reader keeps the reference.
        let first = records(&text).ok().and_then(|r| r.get(2).cloned());
        assert_eq!(
            first,
            Some(Record::Level(LevelRecord {
                level: 3,
                map: LevelMap::Reuse(1),
            }))
        );
    }

    #[test]
    fn edrf_map_reuse_unknown_index() {
        let mut maps = String::new();
        for n in 1..=6 {
            maps.push_str("L ");
            maps.push_str(&n.to_string());
            maps.push('\n');
            maps.push_str(&map_lines(ROWS));
        }
        assert_eq!(
            parse(&format!("{maps}L 9 6\n")).map(|r| r.levels().len()),
            Ok(7)
        );
        assert_eq!(
            parse(&format!("{maps}L 9 7\n")).err(),
            Some(bad_record(4 + 6 * 11 + 1, "L 9 7"))
        );
        assert_eq!(parse("L 1 1\n").err(), Some(bad_record(5, "L 1 1")));
    }

    #[test]
    fn edrf_map_row_too_long() {
        let mut rows = ROWS;
        rows[4] = "V  CC  C  V CCCC";
        assert_eq!(
            parse(&format!("L 1\n{}", map_lines(rows))).err(),
            Some(bad_record(10, " V  CC  C  V CCCC"))
        );
    }

    #[test]
    fn edrf_map_row_without_space() {
        let text = format!("L 1\n{}", map_lines(ROWS)).replacen("\n V  CC", "\nV  CC", 1);
        assert_eq!(parse(&text).err(), Some(bad_record(7, "V  CC  C  V B")));
    }

    #[test]
    fn edrf_map_missing_rows() {
        let rows = rows_text(ROWS.get(..9).unwrap_or_default());
        assert_eq!(
            parse(&format!("L 1\n{rows}")).err(),
            Some(bad_record(5, "L 1"))
        );
        // A last row without a line end is dropped, so the map is short.
        let text = format!("L 1\n{rows} CC");
        assert_eq!(parse(&text).err(), Some(bad_record(5, "L 1")));
    }

    #[test]
    fn queues_per_kind() {
        let text = format!(
            "L 1\n{}R 0000000A\nI @3D\nC 0000000B\nE 0000000C\nL 2 1\nR 0000000D\nI1 A2\nE 0000000E\nQ 1\nZ 6 0000000F\n",
            map_lines(ROWS)
        );
        let recording = parse(&text);
        let got = recording.as_ref().map(|r| {
            (
                r.seeds().to_vec(),
                r.checkpoints().to_vec(),
                r.round_ends().to_vec(),
                r.levels().iter().map(|l| l.level).collect::<Vec<_>>(),
                values(r.controls(slot(0))),
                values(r.controls(slot(1))),
                r.quit(),
                r.end(),
                r.truncated(),
            )
        });
        assert_eq!(
            got,
            Ok((
                vec![10, 13],
                vec![11],
                vec![12, 14],
                vec![1, 2],
                "@@@D@@".to_owned(),
                "@@@DAA".to_owned(),
                Some(slot(1)),
                Some(EndRecord { ticks: 6, hash: 15 }),
                false,
            ))
        );
        let header = recording.as_ref().map(|r| r.header().bonus_score());
        assert_eq!(header, Ok(20000));
    }

    #[test]
    fn header_errors_and_kind() {
        assert_eq!(
            EdrfRecording::parse(b"DRF\nAJ DOS 19990506\n1\n20000\n").err(),
            Some(RecordingError::NotARecording)
        );
        assert!(matches!(
            EdrfRecording::parse(b"eDRF 2\nMS MPL 20190212\n3\n20000\n"),
            Err(RecordingError::BadHeader { line: 3, .. })
        ));
        assert!(matches!(
            EdrfRecording::parse(b"eDRF 2\nAJ DOS 19980101\n1\n20000\n"),
            Err(RecordingError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn non_utf8_line_is_quoted_lossily() {
        let mut bytes = HEADER.as_bytes().to_vec();
        bytes.extend_from_slice(b"I \xff\n");
        assert_eq!(
            EdrfRecording::parse(&bytes).err(),
            Some(bad_record(5, "I \u{fffd}"))
        );
    }
}
