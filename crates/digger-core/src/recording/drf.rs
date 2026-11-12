//! The DRF text format: a header with eight level maps, then a body of
//! round seeds and run-length encoded movement records (game-recording
//! spec, "DRF header level maps", "DRF body grammar", "Malformed input
//! never crashes"; research 05 §4.1, §4.2).
//!
//! The body cannot be decoded without running the game, because one record
//! is taken per control read and only the simulation knows when a digger's
//! controls are read. [`DrfRecording`] therefore keeps the body as bytes and
//! a [`DrfCursor`] reads it on demand during play (design D3). The cursor
//! skips every byte below 0x20 as it reads, so line breaks are
//! insignificant, and reports positions as byte offsets in the file. Runs
//! are never expanded: a run of four thousand million costs one count.

use super::error::{DrfProblem, HeaderProblem, RecordingError};
use super::header::{Header, LineReader, RecordingKind};
use crate::sim::{Direction, LevelLayout, Movement};

/// Number of level plans in a DRF header.
pub const DRF_PLANS: usize = 8;
/// Rows of one level map.
const MAP_ROWS: usize = 10;
/// Bytes of a map line that are used.
const MAP_COLUMNS: usize = 15;
/// Map lines in a DRF header: 8 plans of 10 rows.
const MAP_LINES: usize = DRF_PLANS * MAP_ROWS;
/// Hex digits of a round seed.
const SEED_DIGITS: usize = 8;
/// Characters of a set of initials after its `*`.
const INITIALS: usize = 3;
/// The round end marker.
const EOL: &[u8; 3] = b"EOL";
/// Bytes below this value are dropped from the body (research 05 §4.1).
const FIRST_BODY_BYTE: u8 = 0x20;

/// One movement record: the direction a control read resolves to (none
/// for stop) and the fire input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DrfRecord {
    /// The direction, or none for `s`/`S`.
    pub direction: Option<Direction>,
    /// Whether fire is pressed (an upper-case run letter).
    pub fire: bool,
}

impl DrfRecord {
    /// The record a run letter stands for, or `None` for any other byte.
    pub const fn from_letter(letter: u8) -> Option<Self> {
        let fire = letter.is_ascii_uppercase();
        let direction = match letter.to_ascii_lowercase() {
            b's' => None,
            b'r' => Some(Direction::Right),
            b'u' => Some(Direction::Up),
            b'l' => Some(Direction::Left),
            b'd' => Some(Direction::Down),
            _ => return None,
        };
        Some(Self { direction, fire })
    }
}

impl From<DrfRecord> for Movement {
    fn from(record: DrfRecord) -> Self {
        Self {
            direction: record.direction,
            fire: record.fire,
        }
    }
}

/// A DRF recording: its header, its eight level maps and its body.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DrfRecording {
    header: Header,
    maps: [LevelLayout; DRF_PLANS],
    body: Vec<u8>,
    body_offset: usize,
}

/// The map character a header byte stands for: the five map letters as
/// they are, anything else plain earth.
const fn map_char(byte: u8) -> u8 {
    match byte {
        b'S' | b'V' | b'H' | b'B' | b'C' => byte,
        _ => b' ',
    }
}

impl DrfRecording {
    /// Reads a DRF recording: the four header lines, the 80 map lines and
    /// the body. The body is only checked as it is played.
    ///
    /// # Errors
    ///
    /// Those of [`Header::parse`]; [`RecordingError::NotARecording`] also
    /// for an eDRF file; [`RecordingError::BadHeader`] with
    /// [`HeaderProblem::MissingMapRows`] when the file ends before the 80th
    /// map line.
    pub fn parse(bytes: &[u8]) -> Result<Self, RecordingError> {
        let mut lines = LineReader::new(bytes);
        let header = Header::read(&mut lines)?;
        if header.kind() != RecordingKind::Drf {
            return Err(RecordingError::NotARecording);
        }
        let mut maps = [LevelLayout::EARTH; DRF_PLANS];
        let mut found: u8 = 0;
        for map in &mut maps {
            let mut rows = [[b' '; MAP_COLUMNS]; MAP_ROWS];
            for row in &mut rows {
                let Some(line) = lines.next_line() else {
                    return Err(RecordingError::BadHeader {
                        line: lines.line_number().saturating_add(1),
                        reason: HeaderProblem::MissingMapRows { found },
                    });
                };
                for (cell, byte) in row.iter_mut().zip(line.text) {
                    *cell = map_char(*byte);
                }
                found = found.saturating_add(1);
            }
            *map = LevelLayout::from_rows(rows);
        }
        debug_assert_eq!(usize::from(found), MAP_LINES);
        Ok(Self {
            header,
            maps,
            body: lines.rest().to_vec(),
            body_offset: lines.position(),
        })
    }

    /// The header lines.
    pub const fn header(&self) -> &Header {
        &self.header
    }

    /// The eight level maps of the header, plan 1 first, exactly as
    /// written (the two-digger start-cell patch is not applied).
    pub const fn maps(&self) -> &[LevelLayout; DRF_PLANS] {
        &self.maps
    }

    /// The body bytes as stored, line ends and all.
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// The byte offset of the body in the file.
    pub const fn body_offset(&self) -> usize {
        self.body_offset
    }
}

/// What the next control read takes from the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrfStep {
    /// A movement record.
    Record(DrfRecord),
    /// The end marker: `E` or `e` (of `EOL` or `EOG`) or the end of the
    /// data. The playback ends as a quit; the marker is not consumed.
    End,
}

/// Why a round end does not fall where the recording says it should.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutOfStep {
    /// A run still has this many repeats to give.
    RepeatsPending {
        /// The repeats left.
        remaining: u32,
    },
    /// The body does not hold `EOL` here.
    NoEol {
        /// The byte offset in the file where `EOL` was expected.
        offset: usize,
    },
}

/// The reading position in a DRF body during one playback (design D3).
///
/// It replaces upstream's never-reset static run counter: every playback
/// starts with a fresh cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DrfCursor {
    /// Index into the stored body of the next unread byte.
    pos: usize,
    /// The record of the current run and the repeats it still gives.
    run: Option<(DrfRecord, u32)>,
}

impl DrfCursor {
    /// A cursor at the start of a body.
    pub const fn new() -> Self {
        Self { pos: 0, run: None }
    }

    /// The byte offset in the file of the next unread body byte.
    pub const fn offset(&self, recording: &DrfRecording) -> usize {
        recording.body_offset.saturating_add(self.pos)
    }

    /// The repeats the current run still gives.
    pub fn pending_repeats(&self) -> u32 {
        self.run.map_or(0, |(_, remaining)| remaining)
    }

    /// The next byte from `pos` that is not dropped, with its index.
    fn peek_from(body: &[u8], pos: usize) -> Option<(usize, u8)> {
        body.get(pos..)?
            .iter()
            .enumerate()
            .find(|&(_, &byte)| byte >= FIRST_BODY_BYTE)
            .map(|(skip, &byte)| (pos + skip, byte))
    }

    /// The error for `reason` at body index `index`.
    fn error_at(recording: &DrfRecording, index: usize, reason: DrfProblem) -> RecordingError {
        RecordingError::BadDrfBody {
            offset: recording.body_offset.saturating_add(index),
            reason,
        }
    }

    /// Reads the seed of the round that starts: optional initials (`*` and
    /// three characters, skipped), then 8 hexadecimal digits of either
    /// case, most significant first.
    ///
    /// # Errors
    ///
    /// [`RecordingError::BadDrfBody`] with [`DrfProblem::BadSeed`] at the
    /// first byte that is not a hex digit, or [`DrfProblem::SeedCutOff`] at
    /// the end of the file when the body ends first. The cursor does not
    /// move on an error.
    pub fn next_seed(&mut self, recording: &DrfRecording) -> Result<u32, RecordingError> {
        let body = recording.body();
        let cut_off = || Self::error_at(recording, body.len(), DrfProblem::SeedCutOff);
        let mut pos = self.pos;
        if let Some((index, b'*')) = Self::peek_from(body, pos) {
            pos = index + 1;
            for _ in 0..INITIALS {
                let (index, _) = Self::peek_from(body, pos).ok_or_else(cut_off)?;
                pos = index + 1;
            }
        }
        let mut seed: u32 = 0;
        for _ in 0..SEED_DIGITS {
            let (index, byte) = Self::peek_from(body, pos).ok_or_else(cut_off)?;
            let digit = char::from(byte)
                .to_digit(16)
                .ok_or_else(|| Self::error_at(recording, index, DrfProblem::BadSeed { byte }))?;
            seed = (seed << 4) | digit;
            pos = index + 1;
        }
        self.pos = pos;
        Ok(seed)
    }

    /// Takes the record for one control read: the next repeat of the
    /// current run, or else the next run (a run letter and an optional
    /// decimal count; 0 and 1 both mean once).
    ///
    /// # Errors
    ///
    /// [`RecordingError::BadDrfBody`] with [`DrfProblem::UnknownRunLetter`]
    /// for a byte that is neither a run letter nor `E`/`e`, or
    /// [`DrfProblem::CountTooLarge`] for a count above 4294967295. The
    /// cursor does not move on an error.
    pub fn next_record(&mut self, recording: &DrfRecording) -> Result<DrfStep, RecordingError> {
        if let Some((record, remaining)) = self.run {
            self.run = remaining
                .checked_sub(1)
                .filter(|&n| n > 0)
                .map(|n| (record, n));
            return Ok(DrfStep::Record(record));
        }
        let body = recording.body();
        let Some((index, letter)) = Self::peek_from(body, self.pos) else {
            return Ok(DrfStep::End);
        };
        if matches!(letter, b'E' | b'e') {
            return Ok(DrfStep::End);
        }
        let record = DrfRecord::from_letter(letter).ok_or_else(|| {
            Self::error_at(
                recording,
                index,
                DrfProblem::UnknownRunLetter { byte: letter },
            )
        })?;
        let mut pos = index + 1;
        let mut count: u32 = 0;
        while let Some((index, byte)) = Self::peek_from(body, pos) {
            if !byte.is_ascii_digit() {
                break;
            }
            count = count
                .checked_mul(10)
                .and_then(|count| count.checked_add(u32::from(byte - b'0')))
                .ok_or_else(|| Self::error_at(recording, index, DrfProblem::CountTooLarge))?;
            pos = index + 1;
        }
        self.pos = pos;
        let remaining = count.saturating_sub(1);
        self.run = (remaining > 0).then_some((record, remaining));
        Ok(DrfStep::Record(record))
    }

    /// Consumes the `EOL` that ends a round. The end of the data counts as
    /// `EOL` (the recording was cut short or ended by Exit).
    ///
    /// # Errors
    ///
    /// [`OutOfStep::RepeatsPending`] when the current run is not used up,
    /// [`OutOfStep::NoEol`] when the body does not hold `EOL` here. The
    /// cursor does not move on an error.
    pub fn end_round(&mut self, recording: &DrfRecording) -> Result<(), OutOfStep> {
        if let Some((_, remaining)) = self.run {
            return Err(OutOfStep::RepeatsPending { remaining });
        }
        let body = recording.body();
        let mut pos = self.pos;
        if Self::peek_from(body, pos).is_none() {
            return Ok(());
        }
        let no_eol = OutOfStep::NoEol {
            offset: self.offset(recording),
        };
        for expected in EOL {
            match Self::peek_from(body, pos) {
                Some((index, byte)) if byte == *expected => pos = index + 1,
                _ => return Err(no_eol),
            }
        }
        self.pos = pos;
        Ok(())
    }

    /// Whether a round starts here: optional initials then 8 hex digits.
    /// Used to walk a body without a game; during play the game decides.
    pub fn round_follows(&self, recording: &DrfRecording) -> bool {
        let mut ahead = *self;
        ahead.next_seed(recording).is_ok()
    }

    /// The bytes after the cursor with the dropped bytes removed: after the
    /// last `EOL` this is the trailer (`EOG`, initials), which playback
    /// ignores.
    pub fn rest(&self, recording: &DrfRecording) -> Vec<u8> {
        recording
            .body()
            .get(self.pos..)
            .unwrap_or_default()
            .iter()
            .copied()
            .filter(|&byte| byte >= FIRST_BODY_BYTE)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DRF header with built-in-like maps of `rows` rows.
    fn header_with_rows(rows: &[&str]) -> String {
        let mut text = String::from("DRF\nAJ DOS 19990506\n1\n20000\n");
        for row in rows {
            text.push_str(row);
            text.push('\n');
        }
        text
    }

    /// Replaces map line `index` of `rows`.
    fn set_row<'a>(rows: &mut [&'a str], index: usize, text: &'a str) {
        if let Some(row) = rows.get_mut(index) {
            *row = text;
        }
    }

    fn full_header() -> String {
        header_with_rows(&["S   B     HHHHS"; MAP_LINES])
    }

    fn recording(body: &str) -> Result<DrfRecording, RecordingError> {
        DrfRecording::parse(format!("{}{body}", full_header()).as_bytes())
    }

    const R: DrfRecord = DrfRecord {
        direction: Some(Direction::Right),
        fire: false,
    };
    const UF: DrfRecord = DrfRecord {
        direction: Some(Direction::Up),
        fire: true,
    };
    const L: DrfRecord = DrfRecord {
        direction: Some(Direction::Left),
        fire: false,
    };
    const STOP: DrfRecord = DrfRecord {
        direction: None,
        fire: false,
    };

    /// Records of one round until the end marker, with the error if any.
    fn records(
        rec: &DrfRecording,
        cursor: &mut DrfCursor,
    ) -> Result<Vec<DrfRecord>, RecordingError> {
        let mut out = Vec::new();
        // Bounded so a defect cannot loop forever.
        for _ in 0..10_000 {
            match cursor.next_record(rec)? {
                DrfStep::Record(record) => out.push(record),
                DrfStep::End => return Ok(out),
            }
        }
        Ok(out)
    }

    #[test]
    fn full_width_row() {
        let rec = recording("");
        let row = rec.map(|r| r.maps()[0].rows()[0]);
        assert_eq!(row.as_ref().map(|r| &r[..]), Ok(&b"S   B     HHHHS"[..]));
    }

    #[test]
    fn drf_short_map_row_is_earth() {
        let mut rows = vec!["S   B     HHHHS"; MAP_LINES];
        set_row(&mut rows, 1, "V  CC");
        set_row(&mut rows, 2, "");
        set_row(&mut rows, 3, "SxVh?BC\tH");
        let text = header_with_rows(&rows);
        let rec = DrfRecording::parse(text.as_bytes());
        let plan = rec.map(|r| *r.maps().first().unwrap_or(&LevelLayout::EARTH));
        let rows_of = plan.map(|p| *p.rows());
        assert_eq!(
            rows_of.map(|r| (r[1], r[2], r[3])),
            Ok((
                *b"V  CC          ",
                *b"               ",
                *b"S V  BC H      "
            ))
        );
    }

    #[test]
    fn drf_overlong_map_row_truncated() {
        let mut rows = vec!["S   B     HHHHS"; MAP_LINES];
        set_row(&mut rows, 0, "S   B     HHHHSVVVVV");
        set_row(&mut rows, 79, "CCCCCCCCCCCCCCCBBBBBBBBBB");
        let text = header_with_rows(&rows);
        let rec = DrfRecording::parse(text.as_bytes());
        let first = rec.as_ref().map(|r| r.maps()[0].rows()[0]);
        let last = rec.as_ref().map(|r| r.maps()[7].rows()[9]);
        assert_eq!(first, Ok(*b"S   B     HHHHS"));
        assert_eq!(last, Ok(*b"CCCCCCCCCCCCCCC"));
        // The body starts after the 80th map line.
        assert_eq!(rec.map(|r| r.body().is_empty()), Ok(true));
    }

    #[test]
    fn drf_missing_map_rows_is_header_error() {
        for found in [0_u8, 1, 10, 79] {
            let text = header_with_rows(&vec!["S"; usize::from(found)]);
            assert_eq!(
                DrfRecording::parse(text.as_bytes()),
                Err(RecordingError::BadHeader {
                    line: 5 + u32::from(found),
                    reason: HeaderProblem::MissingMapRows { found },
                }),
                "{found}"
            );
        }
        // 80 lines, the last one unterminated, are complete.
        let mut text = header_with_rows(&vec!["S"; 79]);
        text.push('S');
        assert!(DrfRecording::parse(text.as_bytes()).is_ok());
    }

    #[test]
    fn edrf_is_not_a_drf() {
        assert_eq!(
            DrfRecording::parse(b"eDRF 2\nAJ DOS 19990506\n1\n20000\n"),
            Err(RecordingError::NotARecording)
        );
    }

    #[test]
    fn crlf_file_reads_as_lf() {
        let body = "0001E240\nr12Ul3s\nEOL\n*abc\n";
        let lf = format!("{}{body}", full_header());
        let crlf = lf.replace('\n', "\r\n");
        let a = DrfRecording::parse(lf.as_bytes());
        let b = DrfRecording::parse(crlf.as_bytes());
        assert_eq!(
            a.as_ref().map(|r| (r.header().clone(), *r.maps())),
            b.as_ref().map(|r| (r.header().clone(), *r.maps()))
        );
        let walk = |rec: &DrfRecording| {
            let mut cursor = DrfCursor::new();
            let seed = cursor.next_seed(rec);
            let runs = records(rec, &mut cursor);
            let end = cursor.end_round(rec);
            (seed, runs, end, cursor.rest(rec))
        };
        assert_eq!(a.as_ref().map(walk), b.as_ref().map(walk));
    }

    #[test]
    fn drf_run_length_decoding() {
        let rec = recording("0001E240r12Ul3sEOL");
        let Ok(rec) = rec else {
            return assert!(rec.is_ok(), "{rec:?}");
        };
        let mut cursor = DrfCursor::new();
        assert_eq!(cursor.next_seed(&rec), Ok(0x0001_E240));
        let mut expected = vec![R; 12];
        expected.push(UF);
        expected.extend([L; 3]);
        expected.push(STOP);
        assert_eq!(records(&rec, &mut cursor), Ok(expected));
        assert_eq!(cursor.pending_repeats(), 0);
        assert_eq!(cursor.end_round(&rec), Ok(()));
        assert_eq!(cursor.rest(&rec), Vec::<u8>::new());

        // 0 and 1 both mean once; the end marker is not consumed.
        let rec = recording("00000000r0s1D2EOL");
        let Ok(rec) = rec else {
            return assert!(rec.is_ok(), "{rec:?}");
        };
        let mut cursor = DrfCursor::new();
        assert_eq!(cursor.next_seed(&rec), Ok(0));
        let down_fire = DrfRecord {
            direction: Some(Direction::Down),
            fire: true,
        };
        assert_eq!(
            records(&rec, &mut cursor),
            Ok(vec![R, STOP, down_fire, down_fire])
        );
        assert_eq!(cursor.next_record(&rec), Ok(DrfStep::End));
        assert_eq!(cursor.end_round(&rec), Ok(()));

        // Every letter.
        for (letter, direction) in [
            (b's', None),
            (b'r', Some(Direction::Right)),
            (b'u', Some(Direction::Up)),
            (b'l', Some(Direction::Left)),
            (b'd', Some(Direction::Down)),
        ] {
            assert_eq!(
                DrfRecord::from_letter(letter),
                Some(DrfRecord {
                    direction,
                    fire: false
                })
            );
            assert_eq!(
                DrfRecord::from_letter(letter.to_ascii_uppercase()),
                Some(DrfRecord {
                    direction,
                    fire: true
                })
            );
        }
        assert_eq!(DrfRecord::from_letter(b'x'), None);
        assert_eq!(
            Movement::from(UF),
            Movement {
                direction: Some(Direction::Up),
                fire: true
            }
        );
    }

    #[test]
    fn huge_run_is_not_expanded() {
        let rec = recording("00000000s4294967295EOL");
        let Ok(rec) = rec else {
            return assert!(rec.is_ok(), "{rec:?}");
        };
        let mut cursor = DrfCursor::new();
        assert_eq!(cursor.next_seed(&rec), Ok(0));
        assert_eq!(cursor.next_record(&rec), Ok(DrfStep::Record(STOP)));
        assert_eq!(cursor.pending_repeats(), 4_294_967_294);
        assert_eq!(
            cursor.end_round(&rec),
            Err(OutOfStep::RepeatsPending {
                remaining: 4_294_967_294
            })
        );
        let too_large = recording("00000000s4294967296EOL");
        let Ok(too_large) = too_large else {
            return assert!(too_large.is_ok(), "{too_large:?}");
        };
        let mut cursor = DrfCursor::new();
        assert_eq!(cursor.next_seed(&too_large), Ok(0));
        let offset = too_large.body_offset() + 18;
        assert_eq!(
            cursor.next_record(&too_large),
            Err(RecordingError::BadDrfBody {
                offset,
                reason: DrfProblem::CountTooLarge
            })
        );
    }

    #[test]
    fn drf_body_ignores_control_bytes() {
        let plain = recording("0001E240r12Ul3sEOLEOG");
        let noisy = recording("\r\n00\n01\tE2\r\n40r1\n2U\x01l\x1f3s\nE\r\nO\nL\nE\x00OG\r\n");
        let walk = |rec: &DrfRecording| {
            let mut cursor = DrfCursor::new();
            let seed = cursor.next_seed(rec);
            let runs = records(rec, &mut cursor);
            let end = cursor.end_round(rec);
            (seed, runs, end, cursor.round_follows(rec), cursor.rest(rec))
        };
        let a = plain.as_ref().map(walk);
        assert_eq!(a.as_ref().map(|w| w.4.clone()), Ok(b"EOG".to_vec()));
        assert_eq!(a, noisy.as_ref().map(walk));
    }

    #[test]
    fn drf_initials_before_seed() {
        let rec = recording("*mmm0001E240sEOL*m\nm\nm1A2B3C4DsEOL*abc");
        let Ok(rec) = rec else {
            return assert!(rec.is_ok(), "{rec:?}");
        };
        let mut cursor = DrfCursor::new();
        assert!(cursor.round_follows(&rec));
        assert_eq!(cursor.next_seed(&rec), Ok(0x0001_E240));
        assert_eq!(records(&rec, &mut cursor), Ok(vec![STOP]));
        assert_eq!(cursor.end_round(&rec), Ok(()));
        assert!(cursor.round_follows(&rec));
        assert_eq!(cursor.next_seed(&rec), Ok(0x1A2B_3C4D));
        assert_eq!(records(&rec, &mut cursor), Ok(vec![STOP]));
        assert_eq!(cursor.end_round(&rec), Ok(()));
        // The trailing initials start no round.
        assert!(!cursor.round_follows(&rec));
        assert_eq!(cursor.rest(&rec), b"*abc".to_vec());
        // Seeds of either case.
        let rec = recording("abcdef01");
        let seed = rec.map(|rec| DrfCursor::new().next_seed(&rec));
        assert_eq!(seed, Ok(Ok(0xABCD_EF01)));
    }

    #[test]
    fn drf_bad_seed_reports_offset() {
        let rec = recording("12G45678sEOL");
        let Ok(rec) = rec else {
            return assert!(rec.is_ok(), "{rec:?}");
        };
        let mut cursor = DrfCursor::new();
        assert_eq!(
            cursor.next_seed(&rec),
            Err(RecordingError::BadDrfBody {
                offset: rec.body_offset() + 2,
                reason: DrfProblem::BadSeed { byte: b'G' }
            })
        );
        assert_eq!(cursor, DrfCursor::new());
        let message = cursor.next_seed(&rec).err().map(|e| e.to_string());
        let expected = format!(
            "DRF: bad record at offset {}: seed holds 'G', not a hex digit",
            rec.body_offset() + 2
        );
        assert_eq!(message, Some(expected));

        // Cut off after 5 digits, also with a line end in between.
        for body in ["12345", "12\r\n345\r\n"] {
            let rec = recording(body);
            let Ok(rec) = rec else {
                return assert!(rec.is_ok(), "{rec:?}");
            };
            let end = rec.body_offset() + rec.body().len();
            assert_eq!(
                DrfCursor::new().next_seed(&rec),
                Err(RecordingError::BadDrfBody {
                    offset: end,
                    reason: DrfProblem::SeedCutOff
                }),
                "{body:?}"
            );
        }
        // Initials cut off.
        let rec = recording("*ab");
        let Ok(rec) = rec else {
            return assert!(rec.is_ok(), "{rec:?}");
        };
        assert_eq!(
            DrfCursor::new().next_seed(&rec),
            Err(RecordingError::BadDrfBody {
                offset: rec.body_offset() + 3,
                reason: DrfProblem::SeedCutOff
            })
        );
    }

    #[test]
    fn drf_unknown_run_letter_reports_offset() {
        let rec = recording("00000000r2x5EOL");
        let Ok(rec) = rec else {
            return assert!(rec.is_ok(), "{rec:?}");
        };
        let mut cursor = DrfCursor::new();
        assert_eq!(cursor.next_seed(&rec), Ok(0));
        assert_eq!(cursor.next_record(&rec), Ok(DrfStep::Record(R)));
        assert_eq!(cursor.next_record(&rec), Ok(DrfStep::Record(R)));
        let before = cursor;
        let error = Err(RecordingError::BadDrfBody {
            offset: rec.body_offset() + 10,
            reason: DrfProblem::UnknownRunLetter { byte: b'x' },
        });
        assert_eq!(cursor.next_record(&rec), error);
        assert_eq!(cursor, before);
        assert_eq!(cursor.offset(&rec), rec.body_offset() + 10);
    }

    #[test]
    fn drf_end_of_data_is_end_marker() {
        // The end of the data, `E` and `e` all end the playback.
        for body in [
            "00000000r2",
            "00000000r2EOL",
            "00000000r2EOLEOG",
            "00000000r2e",
            "00000000r2\r\n",
        ] {
            let rec = recording(body);
            let Ok(rec) = rec else {
                return assert!(rec.is_ok(), "{rec:?}");
            };
            let mut cursor = DrfCursor::new();
            assert_eq!(cursor.next_seed(&rec), Ok(0), "{body:?}");
            assert_eq!(records(&rec, &mut cursor), Ok(vec![R, R]), "{body:?}");
            // Reading again stays at the marker.
            assert_eq!(cursor.next_record(&rec), Ok(DrfStep::End), "{body:?}");
            assert_eq!(cursor.next_record(&rec), Ok(DrfStep::End), "{body:?}");
        }
        // Without EOL the round still ends: the stream counts as at EOL.
        let rec = recording("00000000r2");
        let Ok(rec) = rec else {
            return assert!(rec.is_ok(), "{rec:?}");
        };
        let mut cursor = DrfCursor::new();
        assert_eq!(cursor.next_seed(&rec), Ok(0));
        assert_eq!(records(&rec, &mut cursor).map(|r| r.len()), Ok(2));
        assert_eq!(cursor.end_round(&rec), Ok(()));
        assert_eq!(cursor.next_record(&rec), Ok(DrfStep::End));
    }

    #[test]
    fn round_end_out_of_step() {
        let rec = recording("00000000r6EOL");
        let Ok(rec) = rec else {
            return assert!(rec.is_ok(), "{rec:?}");
        };
        let mut cursor = DrfCursor::new();
        assert_eq!(cursor.next_seed(&rec), Ok(0));
        assert_eq!(cursor.next_record(&rec), Ok(DrfStep::Record(R)));
        assert_eq!(
            cursor.end_round(&rec),
            Err(OutOfStep::RepeatsPending { remaining: 5 })
        );
        let rec = recording("00000000r1lEOL");
        let Ok(rec) = rec else {
            return assert!(rec.is_ok(), "{rec:?}");
        };
        let mut cursor = DrfCursor::new();
        assert_eq!(cursor.next_seed(&rec), Ok(0));
        assert_eq!(cursor.next_record(&rec), Ok(DrfStep::Record(R)));
        let offset = rec.body_offset() + 10;
        assert_eq!(cursor.end_round(&rec), Err(OutOfStep::NoEol { offset }));
        let rec = recording("00000000EOX");
        let Ok(rec) = rec else {
            return assert!(rec.is_ok(), "{rec:?}");
        };
        let mut cursor = DrfCursor::new();
        assert_eq!(cursor.next_seed(&rec), Ok(0));
        let offset = rec.body_offset() + 8;
        assert_eq!(cursor.end_round(&rec), Err(OutOfStep::NoEol { offset }));
    }
}
