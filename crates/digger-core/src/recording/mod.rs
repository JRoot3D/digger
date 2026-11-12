//! Reading and writing upstream's recording formats, DRF and eDRF
//! (game-recording spec; design D1–D3 and D9 of
//! `add-recording-and-conformance`).
//!
//! The codecs are total functions over bytes: every input yields a value
//! or a typed error, and nothing here panics. They take `&[u8]` and produce
//! `String`; no file handle crosses the crate boundary, in keeping with the
//! crate's no-I/O rule.
//!
//! The module holds (some parts are added by later task groups):
//!
//! - [`Recording`] (DRF or eDRF) and [`parse`], with the shared types;
//! - `error`: `RecordingError` and `Location`;
//! - `header`: the first line, the version line, the mode line and the
//!   bonus score;
//! - `drf`: `DrfRecording` and its lazy body cursor `DrfCursor`;
//! - `edrf`: `EdrfRecording`, its records and run-length control runs;
//! - `naming`: upstream's default file name for a saved recording.
//!
//! The format grammars are documented in `docs/recording-formats.md`.

mod drf;
mod edrf;
mod error;
mod header;
mod naming;

pub use drf::{DRF_PLANS, DrfCursor, DrfRecord, DrfRecording, DrfStep, OutOfStep};
pub use edrf::{
    ControlRuns, ControlValue, EDRF_SLOTS, EdrfLevel, EdrfRecording, EdrfRecords, EndRecord,
    InputRecord, LevelMap, LevelRecord, Record, Run,
};

pub use error::{DrfProblem, HeaderProblem, Location, RecordingError};
pub(crate) use header::{DRF_FIRST_LINE_TEXT, EDRF_FIRST_LINE_TEXT};
pub use header::{
    Header, LAST_UNSUPPORTED_DATE, RecordingKind, WRITER_VERSION_LINE, detect_kind, mode_line_for,
};
pub use naming::{DEFAULT_TWO_PLAYER_NAME, default_file_name, kind_for_name};

/// A recording in either format. Both are boxed: the DRF header maps
/// and the eDRF queues make them a few hundred bytes to a kilobyte.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Recording {
    /// A DRF recording.
    Drf(Box<DrfRecording>),
    /// An eDRF recording.
    Edrf(Box<EdrfRecording>),
}

impl Recording {
    /// The header lines.
    pub const fn header(&self) -> &Header {
        match self {
            Self::Drf(recording) => recording.header(),
            Self::Edrf(recording) => recording.header(),
        }
    }

    /// The recording's format.
    pub const fn kind(&self) -> RecordingKind {
        match self {
            Self::Drf(_) => RecordingKind::Drf,
            Self::Edrf(_) => RecordingKind::Edrf,
        }
    }
}

/// Reads a recording of either format, chosen by its first line.
///
/// # Errors
///
/// Those of [`DrfRecording::parse`] or [`EdrfRecording::parse`].
pub fn parse(bytes: &[u8]) -> Result<Recording, RecordingError> {
    match detect_kind(bytes)? {
        RecordingKind::Drf => DrfRecording::parse(bytes).map(|r| Recording::Drf(Box::new(r))),
        RecordingKind::Edrf => EdrfRecording::parse(bytes).map(|r| Recording::Edrf(Box::new(r))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_selects_the_format() {
        let edrf = parse(b"eDRF 2\nMS MPL 20190212\nM2\n20000\nR 00000000\n");
        assert_eq!(edrf.as_ref().map(Recording::kind), Ok(RecordingKind::Edrf));
        let mode = edrf.map(|r| r.header().mode_line().to_owned());
        assert_eq!(mode.as_deref(), Ok("M2"));
        let mut drf = String::from("DRF\nAJ DOS 19990506\n1\n20000\n");
        drf.push_str(&"\n".repeat(80));
        assert_eq!(
            parse(drf.as_bytes()).map(|r| r.kind()),
            Ok(RecordingKind::Drf)
        );
        assert_eq!(parse(b"PNG\n"), Err(RecordingError::NotARecording));
    }
}
