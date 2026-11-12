//! The four header lines both recording formats share: the first line that
//! names the kind, the version line, the mode line and the bonus score
//! (game-recording spec, "Recording kind is detected from the first line",
//! "Common header lines", "Mode line grammar", "Pre-1998 recordings are
//! rejected"; research 05 §4.1, §5.2).
//!
//! Lines end in LF or CR LF; a last line without a line end is still a
//! header line.

use super::error::{HeaderProblem, RecordingError};
use crate::sim::{GameMode, StartLevel};

/// The first line of an eDRF recording.
const EDRF_FIRST_LINE: &[u8] = b"eDRF 2";
/// The first three bytes of a DRF recording.
const DRF_MAGIC: &[u8] = b"DRF";
/// Byte offset of the date in the version line (`XX YYY YYYYMMDD`).
const VERSION_DATE_OFFSET: usize = 7;
/// Recordings dated this or earlier are refused: they were made by
/// versions whose monster behaviour the port does not reproduce (research
/// 05 §4.1, the `kludge` flag; porting plan, "Dropped feature").
pub const LAST_UNSUPPORTED_DATE: i64 = 19_981_125;
/// The longest gauntlet a mode line may ask for, in seconds.
const MAX_MODE_GAUNTLET: u32 = 3599;
/// The highest start level a mode line may ask for.
const MAX_MODE_START_LEVEL: u32 = 1000;

/// Line numbers of the header lines.
const VERSION_LINE: u32 = 2;
const MODE_LINE: u32 = 3;
const BONUS_LINE: u32 = 4;

/// Which of the two formats a recording is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordingKind {
    /// Upstream's original text format.
    Drf,
    /// The extended format, `eDRF 2`.
    Edrf,
}

/// One line of a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Line<'a> {
    /// The line without its line end.
    pub(crate) text: &'a [u8],
    /// The 1-based line number.
    pub(crate) number: u32,
    /// Whether a line end followed it.
    pub(crate) terminated: bool,
}

/// Splits bytes into lines ending in LF or CR LF.
#[derive(Debug, Clone)]
pub(crate) struct LineReader<'a> {
    data: &'a [u8],
    pos: usize,
    line: u32,
}

impl<'a> LineReader<'a> {
    /// A reader at the start of `data`.
    pub(crate) const fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            line: 0,
        }
    }

    /// The byte offset of the next unread line.
    pub(crate) const fn position(&self) -> usize {
        self.pos
    }

    /// The number of the last line read (0 before the first).
    pub(crate) const fn line_number(&self) -> u32 {
        self.line
    }

    /// The bytes not yet read.
    pub(crate) fn rest(&self) -> &'a [u8] {
        self.data.get(self.pos..).unwrap_or_default()
    }

    /// The next line, or `None` at the end of the data.
    pub(crate) fn next_line(&mut self) -> Option<Line<'a>> {
        let rest = self.rest();
        if rest.is_empty() {
            return None;
        }
        self.line = self.line.saturating_add(1);
        let (raw, terminated, consumed) = match rest.iter().position(|&b| b == b'\n') {
            Some(end) => (rest.get(..end).unwrap_or_default(), true, end + 1),
            None => (rest, false, rest.len()),
        };
        self.pos += consumed;
        let text = raw.strip_suffix(b"\r").unwrap_or(raw);
        Some(Line {
            text,
            number: self.line,
            terminated,
        })
    }
}

/// The kind named by the first line, which is given without its line end.
fn kind_of_first_line(line: &[u8]) -> Result<RecordingKind, RecordingError> {
    if line == EDRF_FIRST_LINE {
        Ok(RecordingKind::Edrf)
    } else if line.starts_with(DRF_MAGIC) {
        Ok(RecordingKind::Drf)
    } else {
        Err(RecordingError::NotARecording)
    }
}

/// The kind of the recording in `bytes`, decided from its first line alone.
///
/// # Errors
///
/// [`RecordingError::NotARecording`] when the first line is neither exactly
/// `eDRF 2` nor starts with `DRF`, or the data is empty.
pub fn detect_kind(bytes: &[u8]) -> Result<RecordingKind, RecordingError> {
    let line = LineReader::new(bytes)
        .next_line()
        .ok_or(RecordingError::NotARecording)?;
    kind_of_first_line(line.text)
}

/// The date of a version line: the decimal number starting at byte offset
/// 7, read as C's `atol` reads it (leading white space, an optional sign,
/// then digits; no digits read as 0). Values too large saturate.
pub(crate) fn version_date(line: &[u8]) -> i64 {
    let text = line.get(VERSION_DATE_OFFSET..).unwrap_or_default();
    let text = text.trim_ascii_start();
    let (negative, digits) = match text.split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, text),
    };
    let magnitude = digits
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .fold(0_i64, |value, digit| {
            value
                .saturating_mul(10)
                .saturating_add(i64::from(digit - b'0'))
        });
    if negative { -magnitude } else { magnitude }
}

/// Whether a version line dates its recording after 19981125.
pub(crate) fn version_supported(line: &[u8]) -> bool {
    version_date(line) > LAST_UNSUPPORTED_DATE
}

/// A run of at least one decimal digit at the start of `text`, with the
/// rest. `None` when there is no digit or the value exceeds `u32`.
fn leading_number(text: &[u8]) -> Option<(u32, &[u8])> {
    let digits = text.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let (number, rest) = text.split_at_checked(digits)?;
    let value = number.iter().try_fold(0_u32, |value, digit| {
        value.checked_mul(10)?.checked_add(u32::from(digit - b'0'))
    })?;
    Some((value, rest))
}

/// The game mode and start level of a mode line, or `None` when the line
/// does not match
/// `("1" | "2" | ["M" digit] ["G" digits]) ["U"] ["I" digits]`
/// with 1 or 2 diggers, a gauntlet of 1 to 3599 seconds and a start level
/// of 1 to 1000.
pub(crate) fn parse_mode_line(line: &[u8]) -> Option<(GameMode, StartLevel)> {
    let mut players = 1;
    let mut diggers = 1;
    let mut gauntlet = None;
    let mut rest = line;
    match rest.split_first() {
        Some((b'1', tail)) => rest = tail,
        Some((b'2', tail)) => {
            players = 2;
            rest = tail;
        }
        _ => {
            if let Some(tail) = rest.strip_prefix(b"M") {
                let (digit, tail) = tail.split_first()?;
                diggers = match digit {
                    b'1' => 1,
                    b'2' => 2,
                    _ => return None,
                };
                rest = tail;
            }
            if let Some(tail) = rest.strip_prefix(b"G") {
                let (seconds, tail) = leading_number(tail)?;
                if !(1..=MAX_MODE_GAUNTLET).contains(&seconds) {
                    return None;
                }
                gauntlet = Some(seconds);
                rest = tail;
            }
        }
    }
    // `U` is accepted and has no effect (game-recording spec).
    if let Some(tail) = rest.strip_prefix(b"U") {
        rest = tail;
    }
    let mut start_level = StartLevel::FIRST;
    if let Some(tail) = rest.strip_prefix(b"I") {
        let (level, tail) = leading_number(tail)?;
        if !(1..=MAX_MODE_START_LEVEL).contains(&level) {
            return None;
        }
        start_level = StartLevel::new(i64::from(level)).ok()?;
        rest = tail;
    }
    if !rest.is_empty() {
        return None;
    }
    let mode = GameMode::from_parts(players, diggers, gauntlet).ok()?;
    Some((mode, start_level))
}

/// The bonus-life interval of a bonus line: one decimal number from 0 to
/// 65535 and nothing else.
pub(crate) fn parse_bonus_line(line: &[u8]) -> Option<u16> {
    match leading_number(line)? {
        (value, []) => u16::try_from(value).ok(),
        _ => None,
    }
}

/// The header lines common to DRF and eDRF.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Header {
    kind: RecordingKind,
    version_line: String,
    mode_line: String,
    mode: GameMode,
    start_level: StartLevel,
    bonus_score: u16,
}

impl Header {
    /// Reads the four header lines at the start of `bytes`.
    ///
    /// # Errors
    ///
    /// [`RecordingError::NotARecording`] for an unknown first line,
    /// [`RecordingError::UnsupportedVersion`] for a version line dated
    /// 19981125 or earlier (or with no date), and
    /// [`RecordingError::BadHeader`] for a missing line, a bad mode line or
    /// a bad bonus score.
    pub fn parse(bytes: &[u8]) -> Result<Self, RecordingError> {
        Self::read(&mut LineReader::new(bytes))
    }

    /// Reads the four header lines from `lines`, leaving it after the bonus
    /// line.
    pub(crate) fn read(lines: &mut LineReader<'_>) -> Result<Self, RecordingError> {
        let first = lines.next_line().ok_or(RecordingError::NotARecording)?;
        let kind = kind_of_first_line(first.text)?;

        let version = lines.next_line().ok_or(RecordingError::BadHeader {
            line: VERSION_LINE,
            reason: HeaderProblem::MissingVersionLine,
        })?;
        let version_line = String::from_utf8_lossy(version.text).into_owned();
        if !version_supported(version.text) {
            return Err(RecordingError::UnsupportedVersion { version_line });
        }

        let mode = lines.next_line().ok_or(RecordingError::BadHeader {
            line: MODE_LINE,
            reason: HeaderProblem::MissingModeLine,
        })?;
        let mode_line = String::from_utf8_lossy(mode.text).into_owned();
        let Some((game_mode, start_level)) = parse_mode_line(mode.text) else {
            return Err(RecordingError::BadHeader {
                line: MODE_LINE,
                reason: HeaderProblem::BadModeLine { text: mode_line },
            });
        };

        let bonus = lines.next_line().ok_or(RecordingError::BadHeader {
            line: BONUS_LINE,
            reason: HeaderProblem::MissingBonusLine,
        })?;
        let Some(bonus_score) = parse_bonus_line(bonus.text) else {
            return Err(RecordingError::BadHeader {
                line: BONUS_LINE,
                reason: HeaderProblem::BadBonusScore {
                    text: String::from_utf8_lossy(bonus.text).into_owned(),
                },
            });
        };

        Ok(Self {
            kind,
            version_line,
            mode_line,
            mode: game_mode,
            start_level,
            bonus_score,
        })
    }

    /// The recording's format.
    pub const fn kind(&self) -> RecordingKind {
        self.kind
    }

    /// The version line as written (bytes that are not UTF-8 replaced).
    pub fn version_line(&self) -> &str {
        &self.version_line
    }

    /// The mode line as written.
    pub fn mode_line(&self) -> &str {
        &self.mode_line
    }

    /// The game mode the mode line asks for.
    pub const fn mode(&self) -> GameMode {
        self.mode
    }

    /// The start level the mode line asks for (1 when not given).
    pub const fn start_level(&self) -> StartLevel {
        self.start_level
    }

    /// The score interval at which an extra life is awarded.
    pub const fn bonus_score(&self) -> u16 {
        self.bonus_score
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::GauntletSeconds;

    fn header(first: &str, version: &str, mode: &str, bonus: &str) -> String {
        format!("{first}\n{version}\n{mode}\n{bonus}\n")
    }

    fn level(n: i64) -> StartLevel {
        StartLevel::new(n).unwrap_or(StartLevel::FIRST)
    }

    #[test]
    fn drf_first_line() {
        let text = header("DRF", "AJ DOS 19990506", "1", "20000");
        assert_eq!(detect_kind(text.as_bytes()), Ok(RecordingKind::Drf));
        let parsed = Header::parse(text.as_bytes()).map(|h| h.kind());
        assert_eq!(parsed, Ok(RecordingKind::Drf));
        // Only the first three bytes are checked.
        assert_eq!(detect_kind(b"DRFXYZ\n"), Ok(RecordingKind::Drf));
    }

    #[test]
    fn edrf_first_line() {
        let text = header("eDRF 2", "MS MPL 20190212", "1", "20000");
        assert_eq!(detect_kind(text.as_bytes()), Ok(RecordingKind::Edrf));
        let parsed = Header::parse(text.as_bytes()).map(|h| h.kind());
        assert_eq!(parsed, Ok(RecordingKind::Edrf));
        assert_eq!(detect_kind(b"eDRF 2\r\n"), Ok(RecordingKind::Edrf));
    }

    #[test]
    fn unknown_first_line() {
        for bytes in [
            &b"eDRF 3\nAJ DOS 19990506\n1\n20000\n"[..],
            b"PNG\n",
            b"\x89PNG\r\n",
            b"",
            b"eDRF 2 \n",
            b"DR\n",
            b"\nDRF\n",
        ] {
            assert_eq!(
                detect_kind(bytes),
                Err(RecordingError::NotARecording),
                "{bytes:?}"
            );
            assert_eq!(
                Header::parse(bytes),
                Err(RecordingError::NotARecording),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn crlf_header() {
        let lf = header("DRF", "MS MPL 20190212", "M2G300I5", "20000");
        let crlf = lf.replace('\n', "\r\n");
        let a = Header::parse(lf.as_bytes());
        let b = Header::parse(crlf.as_bytes());
        assert!(a.is_ok());
        assert_eq!(a, b);
        let mut lines = LineReader::new(crlf.as_bytes());
        let mut ends = Vec::new();
        while let Some(line) = lines.next_line() {
            ends.push((line.number, line.text.to_vec(), line.terminated));
        }
        assert_eq!(ends.len(), 4);
        assert_eq!(ends.first(), Some(&(1, b"DRF".to_vec(), true)));
        assert_eq!(lines.position(), crlf.len());
    }

    #[test]
    fn unterminated_header_line_is_read() {
        let parsed = Header::parse(b"DRF\nAJ DOS 19990506\n1\n20000");
        assert_eq!(parsed.map(|h| h.bonus_score()), Ok(20000));
        let mut lines = LineReader::new(b"a\nb");
        assert_eq!(lines.next_line().map(|l| l.terminated), Some(true));
        assert_eq!(
            lines.next_line().map(|l| (l.text, l.terminated)),
            Some((&b"b"[..], false))
        );
        assert_eq!(lines.next_line(), None);
        assert_eq!(lines.line_number(), 2);
    }

    #[test]
    fn typical_header() {
        let text = header("DRF", "POK SDL 20110912", "1", "20000");
        let parsed = Header::parse(text.as_bytes()).map(|h| {
            (
                h.version_line().to_owned(),
                h.mode_line().to_owned(),
                h.mode(),
                h.start_level(),
                h.bonus_score(),
            )
        });
        assert_eq!(
            parsed,
            Ok((
                "POK SDL 20110912".to_owned(),
                "1".to_owned(),
                GameMode::OnePlayer,
                StartLevel::FIRST,
                20000
            ))
        );
    }

    #[test]
    fn bonus_score_range() {
        for (text, value) in [("0", 0), ("65535", 65535), ("20000", 20000), ("007", 7)] {
            assert_eq!(parse_bonus_line(text.as_bytes()), Some(value), "{text}");
        }
        for text in [
            "70000",
            "-1",
            "abc",
            "",
            "65536",
            "1 ",
            " 1",
            "+5",
            "99999999999",
        ] {
            assert_eq!(parse_bonus_line(text.as_bytes()), None, "{text}");
            let file = header("DRF", "AJ DOS 19990506", "1", text);
            assert_eq!(
                Header::parse(file.as_bytes()),
                Err(RecordingError::BadHeader {
                    line: 4,
                    reason: HeaderProblem::BadBonusScore {
                        text: text.to_owned()
                    },
                }),
                "{text}"
            );
        }
    }

    #[test]
    fn file_ends_inside_the_header() {
        let cases: [(&[u8], u32, HeaderProblem); 4] = [
            (b"DRF\n", 2, HeaderProblem::MissingVersionLine),
            (b"DRF\nAJ DOS 19990506\n", 3, HeaderProblem::MissingModeLine),
            (
                b"eDRF 2\r\nAJ DOS 19990506\r\n",
                3,
                HeaderProblem::MissingModeLine,
            ),
            (
                b"DRF\nAJ DOS 19990506\n1\n",
                4,
                HeaderProblem::MissingBonusLine,
            ),
        ];
        for (bytes, line, reason) in cases {
            assert_eq!(
                Header::parse(bytes),
                Err(RecordingError::BadHeader { line, reason }),
                "{bytes:?}"
            );
        }
    }

    fn unsupported(version: &str) -> Result<Header, RecordingError> {
        Err(RecordingError::UnsupportedVersion {
            version_line: version.to_owned(),
        })
    }

    #[test]
    fn rejects_19981125() {
        for version in ["AJ DOS 19981125", "AJ DOS 19980704", "AJ DOS -19990506"] {
            let file = header("DRF", version, "1", "20000");
            let parsed = Header::parse(file.as_bytes());
            assert_eq!(parsed, unsupported(version), "{version}");
            let message = parsed.err().map(|e| e.to_string()).unwrap_or_default();
            assert!(message.contains(version), "{message}");
            assert!(
                message.contains("not supported: recorded by a version dated 19981125 or earlier"),
                "{message}"
            );
        }
        // The check comes before the rest of the header is read.
        assert_eq!(
            Header::parse(b"eDRF 2\nAJ DOS 19981125\n"),
            unsupported("AJ DOS 19981125")
        );
    }

    #[test]
    fn accepts_19990130() {
        for version in [
            "AJ DOS 19990130",
            "AJ DOS 19981126",
            "TD WIN 20020310",
            "RS DIG 20261111",
            "XX YYY  19990130 trailing text",
        ] {
            let file = header("DRF", version, "1", "20000");
            let parsed = Header::parse(file.as_bytes()).map(|h| h.version_line().to_owned());
            assert_eq!(parsed, Ok(version.to_owned()), "{version}");
        }
        assert_eq!(version_date(b"AJ DOS 19990130"), 19_990_130);
        assert_eq!(version_date(b"AJ DOS +19990130x"), 19_990_130);
        assert_eq!(version_date(b"AJ DOS 99999999999999999999999"), i64::MAX);
    }

    #[test]
    fn rejects_version_without_date() {
        for version in ["DIGGER", "", "AJ DOS", "AJ DOS ", "AJ DOS x19990130"] {
            assert_eq!(version_date(version.as_bytes()), 0, "{version:?}");
            let file = header("DRF", version, "1", "20000");
            assert_eq!(
                Header::parse(file.as_bytes()),
                unsupported(version),
                "{version:?}"
            );
        }
    }

    #[test]
    fn accepted_mode_lines() {
        let g = |s| GauntletSeconds::new(s);
        let cases = [
            ("1", GameMode::OnePlayer, 1),
            ("2", GameMode::TwoPlayers, 1),
            ("M2", GameMode::TwoDiggers, 1),
            ("M1", GameMode::OnePlayer, 1),
            ("G120", GameMode::Gauntlet { time: g(120) }, 1),
            ("M2G300I5", GameMode::GauntletTwoDiggers { time: g(300) }, 5),
            ("2UI3", GameMode::TwoPlayers, 3),
            ("", GameMode::OnePlayer, 1),
            ("1I5", GameMode::OnePlayer, 5),
            ("M2G90", GameMode::GauntletTwoDiggers { time: g(90) }, 1),
            ("G1", GameMode::Gauntlet { time: g(1) }, 1),
            ("G3599", GameMode::Gauntlet { time: g(3599) }, 1),
            ("I1000", GameMode::OnePlayer, 1000),
            ("U", GameMode::OnePlayer, 1),
            ("1U", GameMode::OnePlayer, 1),
            ("M2U", GameMode::TwoDiggers, 1),
            ("G0120", GameMode::Gauntlet { time: g(120) }, 1),
        ];
        for (line, mode, start) in cases {
            assert_eq!(
                parse_mode_line(line.as_bytes()),
                Some((mode, level(start))),
                "{line:?}"
            );
            let file = header("DRF", "AJ DOS 19990506", line, "20000");
            let parsed = Header::parse(file.as_bytes())
                .map(|h| (h.mode(), h.start_level(), h.mode_line().to_owned()));
            assert_eq!(
                parsed,
                Ok((mode, level(start), line.to_owned())),
                "{line:?}"
            );
        }
    }

    #[test]
    fn rejected_mode_lines() {
        for line in [
            "3",
            "M3",
            "G0",
            "G4000",
            "I0",
            "I1001",
            "1X",
            "M",
            "M0",
            "MG120",
            "G",
            "I",
            "12",
            "1M2",
            "2M2",
            "UU",
            "IU",
            "1 ",
            " 1",
            "M22",
            "G99999999999",
            "2G120",
            "UI",
            "I5U",
        ] {
            assert_eq!(parse_mode_line(line.as_bytes()), None, "{line:?}");
            let file = header("DRF", "AJ DOS 19990506", line, "20000");
            assert_eq!(
                Header::parse(file.as_bytes()),
                Err(RecordingError::BadHeader {
                    line: 3,
                    reason: HeaderProblem::BadModeLine {
                        text: line.to_owned()
                    },
                }),
                "{line:?}"
            );
        }
    }
}
