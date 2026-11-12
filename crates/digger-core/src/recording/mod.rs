//! Reading and writing upstream's recording formats, DRF and eDRF
//! (game-recording spec; design D1–D3 and D9 of
//! `add-recording-and-conformance`).
//!
//! The codecs are total functions over bytes: every input yields a value
//! or a typed error, and nothing here panics. They take `&[u8]` and produce
//! `String`; no file handle crosses the crate boundary, in keeping with the
//! crate's no-I/O rule.
//!
//! The module will hold, as later task groups add them:
//!
//! - `Recording` (DRF or eDRF) and `parse(bytes)`, with the shared types;
//! - `error`: `RecordingError` and `Location`;
//! - `header`: the first line, the version line, the mode line and the
//!   bonus score;
//! - `drf`: `DrfRecording` and its lazy body cursor `DrfCursor`;
//! - `edrf`: `EdrfRecording`, its records and run-length control runs;
//! - `naming`: upstream's default file name for a saved recording.
//!
//! The format grammars are documented in `docs/recording-formats.md`.

mod drf;
mod error;
mod header;

pub use drf::{DRF_PLANS, DrfCursor, DrfRecord, DrfRecording, DrfStep, OutOfStep};

pub use error::{DrfProblem, HeaderProblem, Location, RecordingError};
pub use header::{Header, LAST_UNSUPPORTED_DATE, RecordingKind, detect_kind};
