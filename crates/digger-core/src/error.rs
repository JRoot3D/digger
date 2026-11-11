//! Errors of `digger-core`.
//!
//! Almost every operation is infallible by typed arguments. The fallible
//! ones are decoding the embedded title picture, validating a game
//! configuration and reading level data; each has its own error type, and
//! [`Error`] gathers them.

use std::fmt;

use crate::sim::{ConfigError, LevelError};

/// A failure to decode embedded asset data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// The title stream is not a valid zlib stream.
    TitleInflate,
    /// The title stream inflated to `got` bytes instead of 256000; when the
    /// stream is longer, `got` is the length at which inflation stopped.
    TitleLength {
        /// Bytes produced.
        got: usize,
    },
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TitleInflate => {
                f.write_str("the title picture stream is not a valid zlib stream")
            }
            Self::TitleLength { got } => write!(
                f,
                "the title picture inflated to {got} bytes instead of 256000"
            ),
        }
    }
}

impl std::error::Error for AssetError {}

/// Any error reported by `digger-core`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Embedded asset data could not be decoded.
    Asset(AssetError),
    /// A game configuration cannot be played.
    Config(ConfigError),
    /// Level data cannot be used.
    Level(LevelError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Asset(error) => error.fmt(f),
            Self::Config(error) => error.fmt(f),
            Self::Level(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Asset(error) => Some(error),
            Self::Config(error) => Some(error),
            Self::Level(error) => Some(error),
        }
    }
}

impl From<AssetError> for Error {
    fn from(error: AssetError) -> Self {
        Self::Asset(error)
    }
}

impl From<ConfigError> for Error {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<LevelError> for Error {
    fn from(error: LevelError) -> Self {
        Self::Level(error)
    }
}
