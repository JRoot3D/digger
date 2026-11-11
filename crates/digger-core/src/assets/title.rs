//! The title picture.

use super::TITLE_STREAM;
use crate::error::AssetError;

/// Bytes in the title picture: 640x400 real pixels, one palette index each
/// (research 03 §9.1).
pub const TITLE_BYTES: usize = 256_000;

/// The decompressed title picture: 640x400 palette indices, row-major, using
/// only the values 0, 4, 7, 8 and 15 (research 03 §9.1).
///
/// Decoding inflates the embedded stream and allocates once; whoever owns the
/// game decodes it at start-up, never during a frame.
#[derive(Clone, PartialEq, Eq)]
pub struct TitlePicture {
    pixels: Box<[u8; TITLE_BYTES]>,
}

impl std::fmt::Debug for TitlePicture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TitlePicture")
            .field("len", &self.pixels.len())
            .finish_non_exhaustive()
    }
}

impl TitlePicture {
    /// Inflates the embedded title stream.
    ///
    /// Fails with [`AssetError::TitleInflate`] when the stream is not a valid
    /// zlib stream and with [`AssetError::TitleLength`] when it does not
    /// inflate to exactly 256000 bytes; no partial picture is produced.
    pub fn decode() -> Result<Self, AssetError> {
        decode_stream(TITLE_STREAM).map(|pixels| Self { pixels })
    }

    /// The picture, 640 bytes per real row, 400 rows.
    pub fn pixels(&self) -> &[u8; TITLE_BYTES] {
        &self.pixels
    }
}

/// Inflates a zlib `stream` that must yield exactly [`TITLE_BYTES`] bytes.
fn decode_stream(stream: &[u8]) -> Result<Box<[u8; TITLE_BYTES]>, AssetError> {
    use miniz_oxide::inflate::{TINFLStatus, decompress_to_vec_zlib_with_limit};

    // One byte of headroom tells "too long" apart from "exactly right".
    let picture = decompress_to_vec_zlib_with_limit(stream, TITLE_BYTES + 1).map_err(|error| {
        if error.status == TINFLStatus::HasMoreOutput {
            AssetError::TitleLength {
                got: error.output.len(),
            }
        } else {
            AssetError::TitleInflate
        }
    })?;
    let got = picture.len();
    picture
        .into_boxed_slice()
        .try_into()
        .map_err(|_| AssetError::TitleLength { got })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: usize = 640;

    fn picture() -> TitlePicture {
        TitlePicture::decode().unwrap_or_else(|error| unreachable!("title decodes: {error}"))
    }

    fn pixel(picture: &TitlePicture, column: usize, row: usize) -> u8 {
        picture
            .pixels()
            .get(row * WIDTH + column)
            .copied()
            .unwrap_or_else(|| unreachable!("({column}, {row}) is inside the picture"))
    }

    #[test]
    fn title_decodes_to_256000_bytes() {
        assert_eq!(TITLE_STREAM.len(), 2772);
        assert_eq!(TITLE_STREAM.get(..2), Some(&[0x78, 0xDA][..]));
        assert_eq!(picture().pixels().len(), 256_000);
    }

    #[test]
    fn title_value_set() {
        let mut seen = [false; 256];
        for &value in picture().pixels() {
            if let Some(slot) = seen.get_mut(usize::from(value)) {
                *slot = true;
            }
        }
        let values: Vec<u8> = (0..=255)
            .filter(|&v| seen.get(usize::from(v)) == Some(&true))
            .collect();
        assert_eq!(values, [0, 4, 7, 8, 15]);
    }

    #[test]
    fn title_landmark_pixels() {
        let picture = picture();
        let rows = picture.pixels().as_chunks::<WIDTH>().0;
        assert_eq!(rows.len(), 400);
        // Real rows 0–34 are black: 22400 bytes.
        assert!(rows.iter().take(35).flatten().all(|&b| b == 0));
        // Frame of palette index 4.
        for row in (35..=38).chain(357..=360) {
            for column in 0..WIDTH {
                assert_eq!(pixel(&picture, column, row), 4, "({column}, {row})");
            }
        }
        for row in 35..=360 {
            for column in (0..=5).chain(320..=327).chain(634..=639) {
                assert_eq!(pixel(&picture, column, row), 4, "({column}, {row})");
            }
        }
        for (column, row) in [(3, 200), (324, 200), (636, 200), (100, 36), (100, 358)] {
            assert_eq!(pixel(&picture, column, row), 4, "({column}, {row})");
        }
        // The copyright artwork in rows 363–398 uses 7, 8 and 15 on black.
        let art: Vec<u8> = rows.iter().skip(363).take(36).flatten().copied().collect();
        assert!(art.iter().all(|&b| matches!(b, 0 | 7 | 8 | 15)));
        for value in [7, 8, 15] {
            assert!(art.contains(&value), "artwork uses {value}");
        }
    }

    #[test]
    fn title_truncated_stream_is_error() {
        let truncated = TITLE_STREAM.get(..1000).unwrap_or_default();
        assert_eq!(decode_stream(truncated), Err(AssetError::TitleInflate));
    }

    #[test]
    fn title_wrong_length_is_error() {
        let short = miniz_oxide::deflate::compress_to_vec_zlib(&[0; 1000], 6);
        assert_eq!(
            decode_stream(&short),
            Err(AssetError::TitleLength { got: 1000 })
        );
        let long = miniz_oxide::deflate::compress_to_vec_zlib(&vec![0; TITLE_BYTES + 10], 6);
        assert!(matches!(
            decode_stream(&long),
            Err(AssetError::TitleLength { got }) if got > TITLE_BYTES
        ));
    }
}
