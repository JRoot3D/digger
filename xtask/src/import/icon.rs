//! Icon import: the 64×64 `Icon` table of `icon.c` (research 03 §10;
//! design D2).
//!
//! Upstream stores one 32-bit ARGB word per pixel (alpha in the top byte,
//! then red, green, blue). `icon.rgba` stores the same pixels as the bytes
//! R, G, B, A, row-major.

use super::ImportError;
use super::ctable::TableFile;

/// The table holding the icon.
pub(crate) const TABLE: &str = "Icon";

/// Pixels in the 64×64 icon.
pub(crate) const PIXELS: usize = 64 * 64;

/// Plans `icon.rgba` from `source`.
pub(crate) fn import(source: &TableFile) -> Result<Vec<u8>, ImportError> {
    let words = source.words(TABLE)?;
    if words.len() != PIXELS {
        return Err(ImportError::Count {
            file: source.file().to_owned(),
            table: TABLE.to_owned(),
            expected: PIXELS,
            got: words.len(),
        });
    }
    Ok(words.iter().flat_map(|&argb| argb_to_rgba(argb)).collect())
}

/// One ARGB word as R, G, B, A bytes.
fn argb_to_rgba(argb: u32) -> [u8; 4] {
    let [a, r, g, b] = argb.to_be_bytes();
    [r, g, b, a]
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::{PIXELS, import};
    use crate::import::ImportError;
    use crate::import::ctable::parse;
    use crate::import::fixture::c_words;

    #[test]
    fn icon_channel_order() -> Result<(), Box<dyn Error>> {
        let mut words = vec![0u32; PIXELS];
        if let Some(first) = words.first_mut() {
            *first = 0xFF11_2233;
        }
        if let Some(last) = words.last_mut() {
            *last = 0x8044_5566;
        }
        let rgba = import(&parse("icon.c", c_words("Icon", &words).as_bytes())?)?;
        assert_eq!(rgba.len(), 16_384);
        assert_eq!(rgba.get(..4), Some(&[0x11, 0x22, 0x33, 0xFF][..]));
        assert_eq!(rgba.get(4..8), Some(&[0, 0, 0, 0][..]));
        assert_eq!(rgba.get(16_380..), Some(&[0x44, 0x55, 0x66, 0x80][..]));
        Ok(())
    }

    #[test]
    fn icon_wrong_pixel_count_is_error() -> Result<(), Box<dyn Error>> {
        let words = vec![0u32; PIXELS - 1];
        let error = import(&parse("icon.c", c_words("Icon", &words).as_bytes())?).err();
        assert!(
            matches!(&error, Some(ImportError::Count { table, expected: 4096, got: 4095, .. })
                if table == "Icon"),
            "{error:?}"
        );
        Ok(())
    }
}
