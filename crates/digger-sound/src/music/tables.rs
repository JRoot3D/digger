//! Decoding of `assets/music.bin` into [`TuneTables`] (design D11; research
//! 04 §3.1).
//!
//! The blob is written by `cargo xtask import-assets` from the reference
//! game's tune lists and pinned by `assets/manifest.txt`. All integers are
//! little-endian:
//!
//! ```text
//! magic "DGMU", version u8 = 1, tune count u8 = 4
//! per tune, in the order normal, bonus, dirge, battle:
//!   note count u16, then note count x (divisor u16, duration u16)
//! ```
//!
//! The blob stores every note as the reference game does, the battle tune
//! untransposed; decoding transposes the battle tune's pitched notes up a
//! major second. Decoding happens once, when an engine is created and never
//! on the audio path; afterwards the tables are only read.

use core::fmt;

use crate::command::Tune;
use crate::units::Divisor;

/// The committed blob, embedded at build time.
static MUSIC_BIN: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/music.bin"
));

/// The blob's first four bytes.
const MAGIC: [u8; 4] = *b"DGMU";
/// The only blob format version this decoder reads.
const VERSION: u8 = 1;
/// The number of tunes in a blob.
const TUNE_COUNT: u8 = 4;
/// The tunes in blob order.
const BLOB_ORDER: [Tune; 4] = [Tune::Main, Tune::Bonus, Tune::Dirge, Tune::LastLife];

/// One note of a tune: a pitch and how long it lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Note {
    /// The pitch as a timer divisor; [`Note::REST`] for a rest.
    pub divisor: Divisor,
    /// The duration in tune units (at least 1); the ticks per unit depend on
    /// the tune and tempo.
    pub units: u16,
}

impl Note {
    /// The divisor that marks a rest (32000, `0x7D00`).
    pub const REST: Divisor = Divisor(0x7D00);

    /// Whether the note is a rest.
    pub const fn is_rest(self) -> bool {
        self.divisor.0 == Self::REST.0
    }
}

/// Transposes a battle-tune pitch up a major second:
/// `(divisor x 891 + 500) / 1000` with integer division. Rests are kept.
pub fn transpose_battle(divisor: Divisor) -> Divisor {
    if divisor == Note::REST {
        return divisor;
    }
    let scaled = (u32::from(divisor.0) * 891 + 500) / 1000;
    // 65535 x 891 / 1000 is below 65535, so the fallback is never taken.
    Divisor(u16::try_from(scaled).unwrap_or(u16::MAX))
}

/// Why `music.bin` could not be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TableError {
    /// The blob does not start with `DGMU`.
    BadMagic,
    /// The blob has a format version this decoder does not read.
    UnsupportedVersion {
        /// The version byte found.
        version: u8,
    },
    /// The blob does not hold exactly the four tunes.
    WrongTuneCount {
        /// The tune count found.
        count: u8,
    },
    /// The blob ends in the middle of its header or of a tune.
    Truncated {
        /// The number of bytes the blob would need at least.
        needed: usize,
        /// The blob's length.
        len: usize,
    },
    /// The blob holds bytes after its last tune.
    TrailingBytes {
        /// How many bytes follow the last tune.
        extra: usize,
    },
    /// A tune has no notes.
    EmptyTune {
        /// The tune concerned.
        tune: Tune,
    },
    /// A note lasts 0 units.
    ZeroUnitNote {
        /// The tune concerned.
        tune: Tune,
        /// The note's position in the tune, from 0.
        index: usize,
    },
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => f.write_str("music tables: the blob does not start with \"DGMU\""),
            Self::UnsupportedVersion { version } => write!(
                f,
                "music tables: format version {version} is not supported (expected {VERSION})"
            ),
            Self::WrongTuneCount { count } => write!(
                f,
                "music tables: the blob holds {count} tunes, expected {TUNE_COUNT}"
            ),
            Self::Truncated { needed, len } => write!(
                f,
                "music tables: the blob is truncated ({len} bytes, at least {needed} needed)"
            ),
            Self::TrailingBytes { extra } => write!(
                f,
                "music tables: {extra} unexpected bytes after the last tune"
            ),
            Self::EmptyTune { tune } => write!(f, "music tables: tune {tune:?} has no notes"),
            Self::ZeroUnitNote { tune, index } => write!(
                f,
                "music tables: note {index} of tune {tune:?} lasts 0 units"
            ),
        }
    }
}

impl std::error::Error for TableError {}

/// The notes of the four tunes, decoded once and then only read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuneTables {
    main: Box<[Note]>,
    bonus: Box<[Note]>,
    dirge: Box<[Note]>,
    last_life: Box<[Note]>,
}

/// A little-endian reader over the blob that reports truncation.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], TableError> {
        let end = self.pos.saturating_add(N);
        let taken = self
            .bytes
            .get(self.pos..end)
            .and_then(|slice| <[u8; N]>::try_from(slice).ok())
            .ok_or(TableError::Truncated {
                needed: end,
                len: self.bytes.len(),
            })?;
        self.pos = end;
        Ok(taken)
    }

    fn u8(&mut self) -> Result<u8, TableError> {
        self.take::<1>().map(|[byte]| byte)
    }

    fn u16(&mut self) -> Result<u16, TableError> {
        self.take::<2>().map(u16::from_le_bytes)
    }
}

impl TuneTables {
    /// Decodes the tables embedded in this crate (the committed
    /// `assets/music.bin`).
    pub fn embedded() -> Result<Self, TableError> {
        Self::decode(MUSIC_BIN)
    }

    /// Decodes a `music.bin` blob.
    pub fn decode(bytes: &[u8]) -> Result<Self, TableError> {
        let mut reader = Reader { bytes, pos: 0 };
        if reader.take::<4>()? != MAGIC {
            return Err(TableError::BadMagic);
        }
        let version = reader.u8()?;
        if version != VERSION {
            return Err(TableError::UnsupportedVersion { version });
        }
        let count = reader.u8()?;
        if count != TUNE_COUNT {
            return Err(TableError::WrongTuneCount { count });
        }
        let [main, bonus, dirge, last_life] = BLOB_ORDER;
        let tables = Self {
            main: Self::tune(&mut reader, main)?,
            bonus: Self::tune(&mut reader, bonus)?,
            dirge: Self::tune(&mut reader, dirge)?,
            last_life: Self::tune(&mut reader, last_life)?,
        };
        let extra = bytes.len().saturating_sub(reader.pos);
        if extra != 0 {
            return Err(TableError::TrailingBytes { extra });
        }
        Ok(tables)
    }

    /// Reads one tune, transposing the battle tune.
    fn tune(reader: &mut Reader<'_>, tune: Tune) -> Result<Box<[Note]>, TableError> {
        let count = reader.u16()?;
        if count == 0 {
            return Err(TableError::EmptyTune { tune });
        }
        (0..usize::from(count))
            .map(|index| {
                let divisor = Divisor(reader.u16()?);
                let units = reader.u16()?;
                if units == 0 {
                    return Err(TableError::ZeroUnitNote { tune, index });
                }
                let divisor = if tune == Tune::LastLife {
                    transpose_battle(divisor)
                } else {
                    divisor
                };
                Ok(Note { divisor, units })
            })
            .collect()
    }

    /// The notes of `tune`, in playing order; never empty. The battle tune
    /// played on the last life ([`Tune::LastLife`]) is already transposed.
    pub fn notes(&self, tune: Tune) -> &[Note] {
        match tune {
            Tune::Main => &self.main,
            Tune::Bonus => &self.bonus,
            Tune::Dirge => &self.dirge,
            Tune::LastLife => &self.last_life,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A blob holding `tunes` (raw `(divisor, units)` pairs) under the given
    /// header values.
    fn blob(magic: [u8; 4], version: u8, tunes: &[&[(u16, u16)]]) -> Vec<u8> {
        let mut bytes = magic.to_vec();
        bytes.push(version);
        bytes.push(u8::try_from(tunes.len()).unwrap_or(u8::MAX));
        for notes in tunes {
            bytes.extend(u16::try_from(notes.len()).unwrap_or(u16::MAX).to_le_bytes());
            for (divisor, units) in *notes {
                bytes.extend(divisor.to_le_bytes());
                bytes.extend(units.to_le_bytes());
            }
        }
        bytes
    }

    const ONE: &[(u16, u16)] = &[(2280, 2)];

    fn valid() -> Vec<u8> {
        blob(
            *b"DGMU",
            1,
            &[ONE, ONE, &[(0x7D00, 16)], &[(2280, 4), (0x7D00, 2)]],
        )
    }

    #[test]
    fn decodes_embedded_tables() -> Result<(), TableError> {
        let tables = TuneTables::embedded()?;
        assert_eq!(tables.notes(Tune::Main).len(), 145);
        assert_eq!(tables.notes(Tune::Bonus).len(), 160);
        let dirge = tables.notes(Tune::Dirge);
        assert_eq!(dirge.len(), 24);
        let (head, tail) = dirge.split_at(12);
        let units: u32 = head.iter().map(|note| u32::from(note.units)).sum();
        assert_eq!(units, 50);
        assert!(tail.iter().all(|note| note.is_rest()));
        // The battle tune's length is measured by the importer (design D11).
        assert_eq!(tables.notes(Tune::LastLife).len(), 364);
        for tune in BLOB_ORDER {
            assert!(tables.notes(tune).iter().all(|note| note.units > 0));
        }
        Ok(())
    }

    #[test]
    fn battle_transposition() -> Result<(), TableError> {
        assert_eq!(transpose_battle(Divisor(2280)), Divisor(2031));
        assert_eq!(Divisor(2031).hertz().get(), 587);
        assert_eq!(transpose_battle(Divisor(32000)), Divisor(32000));
        assert_eq!(transpose_battle(Divisor(u16::MAX)), Divisor(58392));
        // Decoding applies it to the battle tune's pitched notes only.
        let tables = TuneTables::decode(&valid())?;
        let note = |divisor, units| Note {
            divisor: Divisor(divisor),
            units,
        };
        assert_eq!(
            tables.notes(Tune::LastLife),
            [note(2031, 4), note(32000, 2)]
        );
        assert_eq!(tables.notes(Tune::Main), [note(2280, 2)]);
        assert_eq!(tables.notes(Tune::Bonus), [note(2280, 2)]);
        assert_eq!(tables.notes(Tune::Dirge), [note(32000, 16)]);
        Ok(())
    }

    #[test]
    fn bad_magic_is_rejected() {
        let bytes = blob(*b"DGMV", 1, &[ONE, ONE, ONE, ONE]);
        assert_eq!(TuneTables::decode(&bytes), Err(TableError::BadMagic));
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let bytes = blob(*b"DGMU", 2, &[ONE, ONE, ONE, ONE]);
        assert_eq!(
            TuneTables::decode(&bytes),
            Err(TableError::UnsupportedVersion { version: 2 })
        );
    }

    #[test]
    fn wrong_tune_count_is_rejected() {
        let bytes = blob(*b"DGMU", 1, &[ONE, ONE, ONE]);
        assert_eq!(
            TuneTables::decode(&bytes),
            Err(TableError::WrongTuneCount { count: 3 })
        );
    }

    #[test]
    fn truncated_blob_is_rejected() {
        let bytes = valid();
        for len in 0..bytes.len() {
            let result = TuneTables::decode(bytes.get(..len).unwrap_or_default());
            assert!(
                matches!(result, Err(TableError::Truncated { needed, len: got })
                    if got == len && needed > len),
                "length {len}: {result:?}"
            );
        }
        assert!(TuneTables::decode(&bytes).is_ok());
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = valid();
        bytes.extend([0, 0, 0]);
        assert_eq!(
            TuneTables::decode(&bytes),
            Err(TableError::TrailingBytes { extra: 3 })
        );
    }

    #[test]
    fn empty_tune_is_rejected() {
        let bytes = blob(*b"DGMU", 1, &[ONE, ONE, &[], ONE]);
        assert_eq!(
            TuneTables::decode(&bytes),
            Err(TableError::EmptyTune { tune: Tune::Dirge })
        );
    }

    #[test]
    fn zero_unit_note_is_rejected() {
        let bytes = blob(*b"DGMU", 1, &[ONE, &[(2280, 2), (2280, 0)], ONE, ONE]);
        assert_eq!(
            TuneTables::decode(&bytes),
            Err(TableError::ZeroUnitNote {
                tune: Tune::Bonus,
                index: 1
            })
        );
    }

    #[test]
    fn errors_describe_themselves() {
        let message = TableError::ZeroUnitNote {
            tune: Tune::Bonus,
            index: 1,
        }
        .to_string();
        assert!(
            message.contains("note 1") && message.contains("Bonus"),
            "{message}"
        );
        assert!(TableError::BadMagic.to_string().contains("DGMU"));
    }
}
