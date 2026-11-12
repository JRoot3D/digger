//! Errors of the recording codecs (design D2 of
//! `add-recording-and-conformance`).
//!
//! Every failure to read a recording is one [`RecordingError`], and every
//! one that can be placed says where: a line number for the header and the
//! eDRF records, a byte offset for the DRF body ([`Location`]).

use std::fmt;

/// Where in a recording a problem was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Location {
    /// A 1-based line number.
    Line(u32),
    /// A 0-based byte offset from the start of the file.
    Offset(usize),
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Line(line) => write!(f, "line {line}"),
            Self::Offset(offset) => write!(f, "offset {offset}"),
        }
    }
}

/// What is wrong with a recording header (game-recording spec, "Common
/// header lines", "Mode line grammar", "DRF header level maps").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum HeaderProblem {
    /// The file ends before the version line.
    MissingVersionLine,
    /// The file ends before the mode line.
    MissingModeLine,
    /// The file ends before the bonus-score line.
    MissingBonusLine,
    /// The file ends after `found` of the 80 DRF map lines.
    MissingMapRows {
        /// Map lines present.
        found: u8,
    },
    /// The mode line does not match the mode grammar.
    BadModeLine {
        /// The line as found.
        text: String,
    },
    /// The bonus-score line is not a decimal number from 0 to 65535.
    BadBonusScore {
        /// The line as found.
        text: String,
    },
}

impl fmt::Display for HeaderProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVersionLine => f.write_str("the file ends before the version line"),
            Self::MissingModeLine => f.write_str("the file ends before the mode line"),
            Self::MissingBonusLine => f.write_str("the file ends before the bonus score line"),
            Self::MissingMapRows { found } => {
                write!(f, "the file ends after {found} of the 80 level map lines")
            }
            Self::BadModeLine { text } => write!(f, "invalid mode line \"{text}\""),
            Self::BadBonusScore { text } => write!(
                f,
                "invalid bonus score \"{text}\": expected a number from 0 to 65535"
            ),
        }
    }
}

/// What is wrong with a DRF body at the place it was read (game-recording
/// spec, "DRF body grammar", "Malformed input never crashes").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DrfProblem {
    /// A seed holds a byte that is not a hexadecimal digit.
    BadSeed {
        /// The offending byte.
        byte: u8,
    },
    /// The body ends inside, or before, a seed.
    SeedCutOff,
    /// A run starts with a byte that is none of `s r u l d S R U L D`
    /// nor the `E`/`e` of the end marker.
    UnknownRunLetter {
        /// The offending byte.
        byte: u8,
    },
    /// A repeat count above 4294967295.
    CountTooLarge,
}

impl fmt::Display for DrfProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadSeed { byte } => {
                write!(f, "seed holds {}, not a hex digit", ByteText(*byte))
            }
            Self::SeedCutOff => f.write_str("the body ends inside a round seed"),
            Self::UnknownRunLetter { byte } => {
                write!(f, "{} is not a run letter", ByteText(*byte))
            }
            Self::CountTooLarge => f.write_str("repeat count above 4294967295"),
        }
    }
}

/// A byte shown in a message: printable ASCII quoted, anything else as hex.
struct ByteText(u8);

impl fmt::Display for ByteText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_ascii_graphic() || self.0 == b' ' {
            write!(f, "'{}'", char::from(self.0))
        } else {
            write!(f, "byte 0x{:02X}", self.0)
        }
    }
}

/// A recording that cannot be read or played (design D2).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RecordingError {
    /// The first line is neither `eDRF 2` nor starts with `DRF`.
    NotARecording,
    /// The version line dates the recording to 19981125 or earlier
    /// (game-recording spec, "Pre-1998 recordings are rejected").
    UnsupportedVersion {
        /// The version line as found.
        version_line: String,
    },
    /// A header line is missing or malformed.
    BadHeader {
        /// The 1-based line number.
        line: u32,
        /// What is wrong.
        reason: HeaderProblem,
    },
    /// An eDRF line matches no record form.
    BadRecord {
        /// The 1-based line number.
        line: u32,
        /// The line as found.
        text: String,
    },
    /// The DRF body is malformed where playback read it.
    BadDrfBody {
        /// The byte offset from the start of the file.
        offset: usize,
        /// What is wrong.
        reason: DrfProblem,
    },
}

impl RecordingError {
    /// Where the problem was found, when it has a place.
    pub const fn location(&self) -> Option<Location> {
        match self {
            Self::NotARecording => Some(Location::Line(1)),
            Self::UnsupportedVersion { .. } => Some(Location::Line(2)),
            Self::BadHeader { line, .. } | Self::BadRecord { line, .. } => {
                Some(Location::Line(*line))
            }
            Self::BadDrfBody { offset, .. } => Some(Location::Offset(*offset)),
        }
    }
}

impl fmt::Display for RecordingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotARecording => {
                f.write_str("not a recording: the first line is neither \"DRF\" nor \"eDRF 2\"")
            }
            Self::UnsupportedVersion { version_line } => write!(
                f,
                "recording \"{version_line}\" not supported: recorded by a version dated 19981125 or earlier"
            ),
            Self::BadHeader { line, reason } => {
                write!(f, "bad recording header at line {line}: {reason}")
            }
            Self::BadRecord { line, text } => {
                write!(f, "eDRF: bad record at line {line}: \"{text}\"")
            }
            Self::BadDrfBody { offset, reason } => {
                write!(f, "DRF: bad record at offset {offset}: {reason}")
            }
        }
    }
}

impl std::error::Error for RecordingError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages() {
        let cases = [
            (
                RecordingError::NotARecording,
                "not a recording: the first line is neither \"DRF\" nor \"eDRF 2\"",
            ),
            (
                RecordingError::UnsupportedVersion {
                    version_line: "AJ DOS 19981125".to_owned(),
                },
                "recording \"AJ DOS 19981125\" not supported: recorded by a version dated 19981125 or earlier",
            ),
            (
                RecordingError::BadHeader {
                    line: 2,
                    reason: HeaderProblem::MissingVersionLine,
                },
                "bad recording header at line 2: the file ends before the version line",
            ),
            (
                RecordingError::BadHeader {
                    line: 3,
                    reason: HeaderProblem::MissingModeLine,
                },
                "bad recording header at line 3: the file ends before the mode line",
            ),
            (
                RecordingError::BadHeader {
                    line: 4,
                    reason: HeaderProblem::MissingBonusLine,
                },
                "bad recording header at line 4: the file ends before the bonus score line",
            ),
            (
                RecordingError::BadHeader {
                    line: 84,
                    reason: HeaderProblem::MissingMapRows { found: 79 },
                },
                "bad recording header at line 84: the file ends after 79 of the 80 level map lines",
            ),
            (
                RecordingError::BadHeader {
                    line: 3,
                    reason: HeaderProblem::BadModeLine {
                        text: "M3".to_owned(),
                    },
                },
                "bad recording header at line 3: invalid mode line \"M3\"",
            ),
            (
                RecordingError::BadHeader {
                    line: 4,
                    reason: HeaderProblem::BadBonusScore {
                        text: "abc".to_owned(),
                    },
                },
                "bad recording header at line 4: invalid bonus score \"abc\": expected a number from 0 to 65535",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn record_messages() {
        let cases = [
            (
                RecordingError::BadRecord {
                    line: 57,
                    text: "X 1234".to_owned(),
                },
                "eDRF: bad record at line 57: \"X 1234\"",
            ),
            (
                RecordingError::BadDrfBody {
                    offset: 1234,
                    reason: DrfProblem::BadSeed { byte: b'G' },
                },
                "DRF: bad record at offset 1234: seed holds 'G', not a hex digit",
            ),
            (
                RecordingError::BadDrfBody {
                    offset: 9,
                    reason: DrfProblem::BadSeed { byte: 0x80 },
                },
                "DRF: bad record at offset 9: seed holds byte 0x80, not a hex digit",
            ),
            (
                RecordingError::BadDrfBody {
                    offset: 7,
                    reason: DrfProblem::SeedCutOff,
                },
                "DRF: bad record at offset 7: the body ends inside a round seed",
            ),
            (
                RecordingError::BadDrfBody {
                    offset: 8,
                    reason: DrfProblem::UnknownRunLetter { byte: b'x' },
                },
                "DRF: bad record at offset 8: 'x' is not a run letter",
            ),
            (
                RecordingError::BadDrfBody {
                    offset: 5,
                    reason: DrfProblem::CountTooLarge,
                },
                "DRF: bad record at offset 5: repeat count above 4294967295",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn locations() {
        assert_eq!(
            RecordingError::NotARecording.location(),
            Some(Location::Line(1))
        );
        let error = RecordingError::BadRecord {
            line: 57,
            text: String::new(),
        };
        assert_eq!(error.location(), Some(Location::Line(57)));
        assert_eq!(
            error.location().map(|l| l.to_string()),
            Some("line 57".to_owned())
        );
        let error = RecordingError::BadDrfBody {
            offset: 3,
            reason: DrfProblem::SeedCutOff,
        };
        assert_eq!(error.location(), Some(Location::Offset(3)));
        assert_eq!(Location::Offset(3).to_string(), "offset 3");
    }
}
