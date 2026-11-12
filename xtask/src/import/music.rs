//! The four tunes of `sound_backend.c`, read as data into `assets/music.bin`
//! (add-sound-engine design D11; research 04 §3.1).
//!
//! Upstream keeps each tune as a `static const int16_t` initialiser list of
//! `divisor, duration` pairs closed by the end marker `0x7d64`. Only those
//! four lists are read, as integer tokens through [`ctable::parse_only`];
//! the rest of the file is skipped unread. The end marker is checked and
//! dropped (the note count replaces it), and every note is kept exactly as
//! upstream stores it: rests (divisor `0x7d00`) stay in place and the battle
//! tune is not transposed.
//!
//! Blob format, all integers little-endian:
//!
//! ```text
//! magic "DGMU", version u8 = 1, tune count u8 = 4
//! per tune, in the order normal, bonus, dirge, battle:
//!   note count u16, then note count x (divisor u16, duration u16)
//! ```

use super::ImportError;
use super::ctable::{self, TableFile};

/// The upstream source file holding the tunes.
pub(crate) const MUSIC_FILE: &str = "sound_backend.c";

/// The blob's first four bytes.
pub(crate) const MAGIC: [u8; 4] = *b"DGMU";
/// The blob format version.
pub(crate) const VERSION: u8 = 1;
/// The value closing every upstream tune list.
pub(crate) const END_MARKER: u16 = 0x7d64;
/// The divisor of a rest.
pub(crate) const REST: u16 = 0x7d00;

/// Number of notes of the normal tune.
pub(crate) const NORMAL_NOTES: usize = 145;
/// Number of notes of the bonus tune.
pub(crate) const BONUS_NOTES: usize = 160;
/// Number of notes of the dirge.
pub(crate) const DIRGE_NOTES: usize = 24;
/// The dirge's sounding part: its first 12 notes, 50 units long in all.
const DIRGE_HEAD: usize = 12;
const DIRGE_HEAD_UNITS: u32 = 50;

/// One tune of the blob, in blob order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TuneSpec {
    /// The name the tune is printed under.
    pub(crate) label: &'static str,
    /// The upstream initialiser list.
    pub(crate) table: &'static str,
    /// The required note count, or `None` where it is only measured.
    pub(crate) notes: Option<usize>,
}

/// The four tunes, in blob order.
pub(crate) const TUNES: [TuneSpec; 4] = [
    TuneSpec {
        label: "normal",
        table: "backgjingle",
        notes: Some(NORMAL_NOTES),
    },
    TuneSpec {
        label: "bonus",
        table: "bonusjingle",
        notes: Some(BONUS_NOTES),
    },
    TuneSpec {
        label: "dirge",
        table: "dirge",
        notes: Some(DIRGE_NOTES),
    },
    TuneSpec {
        label: "battle",
        table: "battlejingle",
        notes: None,
    },
];

/// One note as upstream stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Note {
    pub(crate) divisor: u16,
    pub(crate) duration: u16,
}

/// The result of the music import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Music {
    /// The contents of `music.bin`.
    pub(crate) blob: Vec<u8>,
    /// The note count of each tune, in blob order.
    pub(crate) counts: [usize; 4],
}

fn tune_error(file: &TableFile, table: &str, message: impl Into<String>) -> ImportError {
    ImportError::Tune {
        file: file.file().to_owned(),
        table: table.to_owned(),
        message: message.into(),
    }
}

/// Parses the four tune lists, and nothing else, from `text`.
pub(crate) fn parse(text: &[u8]) -> Result<TableFile, ImportError> {
    let names = TUNES.map(|tune| tune.table);
    ctable::parse_only(MUSIC_FILE, text, &names)
}

/// The notes of the list `table`: its values read as `divisor, duration`
/// pairs, with the closing end marker checked and removed.
pub(crate) fn notes(file: &TableFile, table: &str) -> Result<Vec<Note>, ImportError> {
    let values = file.halves(table)?;
    let Some((&END_MARKER, pairs)) = values.split_last() else {
        return Err(tune_error(
            file,
            table,
            format!("does not end with the end marker 0x{END_MARKER:04x}"),
        ));
    };
    if pairs.len() % 2 != 0 {
        return Err(tune_error(
            file,
            table,
            format!(
                "has {} values before the end marker, not an even number of \
                 divisor and duration pairs",
                pairs.len()
            ),
        ));
    }
    let notes: Vec<Note> = pairs
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[divisor, duration]| Note { divisor, duration })
        .collect();
    if let Some(index) = notes.iter().position(|note| note.divisor == END_MARKER) {
        return Err(tune_error(
            file,
            table,
            format!("note {index} is an end marker before the end of the list"),
        ));
    }
    Ok(notes)
}

/// Checks the shape of one tune (design D11).
fn check_shape(file: &TableFile, spec: TuneSpec, notes: &[Note]) -> Result<(), ImportError> {
    if notes.is_empty() {
        return Err(tune_error(file, spec.table, "has no notes"));
    }
    if let Some(expected) = spec.notes
        && notes.len() != expected
    {
        return Err(ImportError::Count {
            file: file.file().to_owned(),
            table: spec.table.to_owned(),
            expected,
            got: notes.len(),
        });
    }
    if let Some(index) = notes.iter().position(|note| note.duration == 0) {
        return Err(tune_error(
            file,
            spec.table,
            format!("note {index} has a duration of 0"),
        ));
    }
    if spec.table == "dirge" {
        let (head, tail) = notes.split_at(DIRGE_HEAD.min(notes.len()));
        let units: u32 = head.iter().map(|note| u32::from(note.duration)).sum();
        if units != DIRGE_HEAD_UNITS {
            return Err(tune_error(
                file,
                spec.table,
                format!(
                    "the first {DIRGE_HEAD} notes last {units} units, expected {DIRGE_HEAD_UNITS}"
                ),
            ));
        }
        if let Some(offset) = tail.iter().position(|note| note.divisor != REST) {
            return Err(tune_error(
                file,
                spec.table,
                format!(
                    "note {} is not a rest; the last {} notes must be rests",
                    DIRGE_HEAD + offset,
                    tail.len()
                ),
            ));
        }
    }
    Ok(())
}

/// Encodes the tunes, in blob order, as `music.bin`.
pub(crate) fn encode(tunes: &[Vec<Note>]) -> Option<Vec<u8>> {
    let mut blob = MAGIC.to_vec();
    blob.push(VERSION);
    blob.push(u8::try_from(tunes.len()).ok()?);
    for notes in tunes {
        blob.extend(u16::try_from(notes.len()).ok()?.to_le_bytes());
        for note in notes {
            blob.extend(note.divisor.to_le_bytes());
            blob.extend(note.duration.to_le_bytes());
        }
    }
    Some(blob)
}

/// Reads, checks and encodes the four tunes of a parsed `sound_backend.c`.
pub(crate) fn import(file: &TableFile) -> Result<Music, ImportError> {
    let mut tunes = Vec::with_capacity(TUNES.len());
    let mut counts = [0; 4];
    for (spec, count) in TUNES.iter().zip(&mut counts) {
        let notes = notes(file, spec.table)?;
        check_shape(file, *spec, &notes)?;
        *count = notes.len();
        tunes.push(notes);
    }
    let blob = encode(&tunes).ok_or_else(|| {
        tune_error(
            file,
            "battlejingle",
            "has more notes than the format can hold",
        )
    })?;
    Ok(Music { blob, counts })
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::{BONUS_NOTES, DIRGE_NOTES, NORMAL_NOTES, Note, import, notes, parse};
    use crate::import::ImportError;
    use crate::import::fixture::sound_backend_c;

    /// A fragment in the shape of upstream: tune lists among code and other
    /// initialised declarations the importer must skip.
    const FRAGMENT: &str = "#include \"def.h\"\n\
        static bool soundfireflag[FIREBALLS]={false,false},sff[FIREBALLS];\n\
        static const int16_t emfreqs[3]={0x8e8,0x7f0,0x712};\n\
        static const int16_t dirge[]={\n  0x7d00, 2,0x11d1, 6,\n  0x7d64};\n\
        void soundupdate(void)\n{\n  if (dirge[musicp]==0x7d64) musicp=0;\n}\n\
        static const int16_t odd[]={0x11d1, 2, 4, 0x7d64};\n\
        static const int16_t open[]={0x11d1, 2};\n\
        static const int16_t early[]={0x7d64, 2, 0x7d64};\n";

    fn tune(table: &str) -> Result<Vec<Note>, ImportError> {
        let file = crate::import::ctable::parse_only(
            super::MUSIC_FILE,
            FRAGMENT.as_bytes(),
            &["dirge", "odd", "open", "early"],
        )?;
        notes(&file, table)
    }

    #[test]
    fn parses_pairs_and_end_marker() -> Result<(), Box<dyn Error>> {
        assert_eq!(
            tune("dirge")?,
            [
                Note {
                    divisor: 0x7d00,
                    duration: 2
                },
                Note {
                    divisor: 0x11d1,
                    duration: 6
                }
            ]
        );
        Ok(())
    }

    #[test]
    fn rejects_missing_end_marker() {
        let error = tune("open").err();
        assert!(
            matches!(&error, Some(ImportError::Tune { file, table, .. })
                if file == "sound_backend.c" && table == "open"),
            "{error:?}"
        );
        let message = error.map(|e| e.to_string()).unwrap_or_default();
        assert!(message.contains("end marker 0x7d64"), "{message}");
        let error = tune("early").err();
        assert!(
            matches!(&error, Some(ImportError::Tune { table, message, .. })
                if table == "early" && message.contains("note 0")),
            "{error:?}"
        );
    }

    #[test]
    fn rejects_odd_length() {
        let error = tune("odd").err();
        assert!(
            matches!(&error, Some(ImportError::Tune { table, message, .. })
                if table == "odd" && message.contains("3 values")),
            "{error:?}"
        );
    }

    #[test]
    fn fixture_tunes_have_the_upstream_shape() -> Result<(), Box<dyn Error>> {
        let music = import(&parse(sound_backend_c(&[]).as_bytes())?)?;
        assert_eq!(music.counts[..3], [NORMAL_NOTES, BONUS_NOTES, DIRGE_NOTES]);
        let notes: usize = music.counts.iter().sum();
        assert_eq!(music.blob.len(), 6 + 4 * 2 + notes * 4);
        assert_eq!(music.blob.get(..6), Some(&b"DGMU\x01\x04"[..]));
        let normal = u16::try_from(NORMAL_NOTES)?.to_le_bytes();
        assert_eq!(music.blob.get(6..8), Some(&normal[..]));
        Ok(())
    }

    /// The fixture with one tune list replaced by `values`.
    fn import_with(table: &'static str, values: &[u16]) -> Result<super::Music, ImportError> {
        import(&parse(
            sound_backend_c(&[(table, values.to_vec())]).as_bytes(),
        )?)
    }

    #[test]
    fn rejects_wrong_note_counts() {
        let error = import_with("bonusjingle", &[0x11d1, 2, 0x7d64]).err();
        assert!(
            matches!(&error, Some(ImportError::Count { table, expected: 160, got: 1, .. })
                if table == "bonusjingle"),
            "{error:?}"
        );
        let error = import_with("battlejingle", &[0x7d64]).err();
        assert!(
            matches!(&error, Some(ImportError::Tune { table, message, .. })
                if table == "battlejingle" && message.contains("no notes")),
            "{error:?}"
        );
    }

    /// A dirge of 24 notes: the given head, then 12 rests of 16 units.
    fn dirge(head: &[(u16, u16)], tail_divisor: u16) -> Vec<u16> {
        let mut values: Vec<u16> = head.iter().flat_map(|&(d, u)| [d, u]).collect();
        for _ in 0..12 {
            values.extend([tail_divisor, 16]);
        }
        values.push(0x7d64);
        values
    }

    #[test]
    fn rejects_bad_dirge_shape() {
        let good: Vec<(u16, u16)> = std::iter::once((0x7d00, 6))
            .chain(std::iter::repeat_n((0x11d1, 4), 11))
            .collect();
        assert!(import_with("dirge", &dirge(&good, 0x7d00)).is_ok());

        let mut long = good.clone();
        if let Some(note) = long.last_mut() {
            note.1 = 5;
        }
        let error = import_with("dirge", &dirge(&long, 0x7d00)).err();
        assert!(
            matches!(&error, Some(ImportError::Tune { message, .. })
                if message.contains("last 51 units")),
            "{error:?}"
        );
        let error = import_with("dirge", &dirge(&good, 0x11d1)).err();
        assert!(
            matches!(&error, Some(ImportError::Tune { message, .. })
                if message.contains("note 12 is not a rest")),
            "{error:?}"
        );
        let mut zero = good;
        if let Some(note) = zero.first_mut() {
            *note = (0x11d1, 0);
        }
        let error = import_with("dirge", &dirge(&zero, 0x7d00)).err();
        assert!(
            matches!(&error, Some(ImportError::Tune { message, .. })
                if message.contains("note 0 has a duration of 0")),
            "{error:?}"
        );
    }
}
