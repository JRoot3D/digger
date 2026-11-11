//! Title import: the zlib stream `title_gz` of `title_gz.c` (research 03
//! §9.1; design D2, D8).
//!
//! The stream is committed verbatim as `title.zz`. The importer inflates it
//! only to check that it yields exactly the 640×400 picture, and reports the
//! CRC-32 of the inflated bytes so that the golden checksum of the title
//! picture can be compared with a value computed from upstream bytes alone.

use super::ImportError;
use super::ctable::TableFile;

/// The table holding the compressed title picture.
pub(crate) const TABLE: &str = "title_gz";

/// Length of the upstream zlib stream.
pub(crate) const STREAM_LEN: usize = 2772;

/// Length of the inflated picture: 640×400 palette indices.
pub(crate) const PICTURE_LEN: usize = 256_000;

/// The planned `title.zz`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TitlePlan {
    /// The zlib stream, verbatim.
    pub(crate) stream: Vec<u8>,
    /// CRC-32 (IEEE) of the inflated picture.
    pub(crate) picture_crc32: u32,
}

/// Plans `title.zz` from `source`, checking that the stream inflates to the
/// picture.
pub(crate) fn import(source: &TableFile) -> Result<TitlePlan, ImportError> {
    let file = source.file();
    let stream = source.bytes(TABLE)?;
    if stream.len() != STREAM_LEN {
        return Err(ImportError::Length {
            file: file.to_owned(),
            table: TABLE.to_owned(),
            expected: STREAM_LEN,
            got: stream.len(),
        });
    }
    let inflate_error = |message: String| ImportError::TitleInflate {
        file: file.to_owned(),
        table: TABLE.to_owned(),
        message,
    };
    let picture = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(stream, PICTURE_LEN)
        .map_err(|error| {
            if error.status == miniz_oxide::inflate::TINFLStatus::HasMoreOutput {
                inflate_error(format!("inflates to more than {PICTURE_LEN} bytes"))
            } else {
                inflate_error(format!("is not a valid zlib stream ({error})"))
            }
        })?;
    if picture.len() != PICTURE_LEN {
        return Err(inflate_error(format!(
            "inflates to {} bytes, expected {PICTURE_LEN}",
            picture.len()
        )));
    }
    Ok(TitlePlan {
        stream: stream.to_vec(),
        picture_crc32: crc32(&picture),
    })
}

/// CRC-32 with the IEEE polynomial (reflected 0xEDB88320, initial and final
/// value 0xFFFFFFFF), as used by zlib and PNG.
///
/// The importer has its own copy so that the title checksum it prints does
/// not depend on the renderer's implementation in `digger-core`.
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::{PICTURE_LEN, STREAM_LEN, crc32, import};
    use crate::import::ImportError;
    use crate::import::ctable::parse;
    use crate::import::fixture::{c_bytes, title_picture, title_stream};

    fn source(stream: &[u8]) -> String {
        format!(
            "#include <zlib.h>\n#define CTITLELEN {}\n{}",
            stream.len(),
            c_bytes("title_gz", stream).replace("title_gz[]", "title_gz[CTITLELEN]")
        )
    }

    #[test]
    fn crc32_known_vectors() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn title_import_checks_inflated_length() -> Result<(), Box<dyn Error>> {
        // A stream of the right length that inflates to the picture.
        let picture = title_picture(PICTURE_LEN);
        let stream = title_stream(&picture).ok_or("no fixture stream")?;
        let plan = import(&parse("title_gz.c", source(&stream).as_bytes())?)?;
        assert_eq!(plan.stream, stream);
        assert_eq!(plan.picture_crc32, crc32(&picture));

        // One byte short of the picture.
        let stream = title_stream(&title_picture(PICTURE_LEN - 1)).ok_or("no fixture stream")?;
        let error = import(&parse("title_gz.c", source(&stream).as_bytes())?).err();
        assert!(
            matches!(&error, Some(ImportError::TitleInflate { file, table, message })
                if file == "title_gz.c" && table == "title_gz" && message.contains("255999")),
            "{error:?}"
        );

        // One byte more than the picture.
        let stream = title_stream(&title_picture(PICTURE_LEN + 1)).ok_or("no fixture stream")?;
        let error = import(&parse("title_gz.c", source(&stream).as_bytes())?).err();
        assert!(
            matches!(&error, Some(ImportError::TitleInflate { message, .. })
                if message.contains("more than 256000")),
            "{error:?}"
        );
        Ok(())
    }

    #[test]
    fn title_import_rejects_corrupt_stream() -> Result<(), Box<dyn Error>> {
        let mut stream = title_stream(&title_picture(PICTURE_LEN)).ok_or("no fixture stream")?;
        if let Some(last) = stream.last_mut() {
            *last ^= 0xff; // breaks the Adler-32 trailer
        }
        let error = import(&parse("title_gz.c", source(&stream).as_bytes())?).err();
        assert!(
            matches!(&error, Some(ImportError::TitleInflate { message, .. })
                if message.contains("not a valid zlib stream")),
            "{error:?}"
        );
        Ok(())
    }

    #[test]
    fn title_import_checks_stream_length() -> Result<(), Box<dyn Error>> {
        let stream = vec![0x78; STREAM_LEN - 1];
        let error = import(&parse("title_gz.c", source(&stream).as_bytes())?).err();
        assert!(
            matches!(
                &error,
                Some(ImportError::Length {
                    expected: 2772,
                    got: 2771,
                    ..
                })
            ),
            "{error:?}"
        );
        Ok(())
    }
}
