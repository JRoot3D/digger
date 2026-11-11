//! Font import: the 40 VGA glyph arrays of `alpha.c` and the 95-entry
//! character table `ascii2vga` (research 03 §6; design D2).
//!
//! `font.bin` holds the glyphs in the fixed order A–Z, 0–9, `.`, `:`, `_`,
//! space. The character table is turned into a glyph number (an index into
//! that order) for each character code 32–126, or `None`. The CGA glyphs and
//! `ascii2cga` in the same file are never looked at.

use super::ImportError;
use super::ctable::TableFile;

/// The pointer table mapping character codes 32–126 to glyph arrays.
pub(crate) const CHAR_TABLE: &str = "ascii2vga";

/// Entries in the character table: codes 32 to 126.
pub(crate) const CHAR_COUNT: usize = 95;

/// Bytes per glyph: 24×24 real pixels.
pub(crate) const GLYPH_BYTES: usize = 576;

/// The upstream glyph arrays, in the order they are stored in `font.bin`.
pub(crate) const GLYPH_NAMES: [&str; 40] = [
    "vgaletA",
    "vgaletB",
    "vgaletC",
    "vgaletD",
    "vgaletE",
    "vgaletF",
    "vgaletG",
    "vgaletH",
    "vgaletI",
    "vgaletJ",
    "vgaletK",
    "vgaletL",
    "vgaletM",
    "vgaletN",
    "vgaletO",
    "vgaletP",
    "vgaletQ",
    "vgaletR",
    "vgaletS",
    "vgaletT",
    "vgaletU",
    "vgaletV",
    "vgaletW",
    "vgaletX",
    "vgaletY",
    "vgaletZ",
    "vganum0",
    "vganum1",
    "vganum2",
    "vganum3",
    "vganum4",
    "vganum5",
    "vganum6",
    "vganum7",
    "vganum8",
    "vganum9",
    "vgasymdot",
    "vgasymcolon",
    "vgasymline",
    "vgasymspace",
];

/// The planned `font.bin` and its character map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FontPlan {
    /// 40 glyphs of 576 bytes.
    pub(crate) blob: Vec<u8>,
    /// For each code 32–126, the glyph number in `blob`, if any.
    pub(crate) glyph_index: Vec<Option<u8>>,
}

/// Plans `font.bin` from the glyph arrays and character table of `source`.
pub(crate) fn import(source: &TableFile) -> Result<FontPlan, ImportError> {
    let file = source.file();
    let mut blob = Vec::with_capacity(GLYPH_NAMES.len() * GLYPH_BYTES);
    for name in GLYPH_NAMES {
        let glyph = source.bytes(name)?;
        if glyph.len() != GLYPH_BYTES {
            return Err(ImportError::Length {
                file: file.to_owned(),
                table: name.to_owned(),
                expected: GLYPH_BYTES,
                got: glyph.len(),
            });
        }
        blob.extend_from_slice(glyph);
    }

    let chars = source.pointers(CHAR_TABLE)?;
    if chars.len() != CHAR_COUNT {
        return Err(ImportError::Count {
            file: file.to_owned(),
            table: CHAR_TABLE.to_owned(),
            expected: CHAR_COUNT,
            got: chars.len(),
        });
    }
    let line = source.get(CHAR_TABLE).map_or(1, |table| table.line);
    let glyph_index = chars
        .iter()
        .map(|element| {
            let Some(symbol) = element else {
                return Ok(None);
            };
            GLYPH_NAMES
                .iter()
                .position(|name| name == symbol)
                .and_then(|glyph| u8::try_from(glyph).ok())
                .map(Some)
                .ok_or_else(|| ImportError::UnknownSymbol {
                    file: file.to_owned(),
                    line,
                    table: CHAR_TABLE.to_owned(),
                    symbol: symbol.clone(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FontPlan { blob, glyph_index })
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::{GLYPH_BYTES, GLYPH_NAMES, import};
    use crate::import::ImportError;
    use crate::import::ctable::parse;
    use crate::import::fixture::{alpha_c, glyph_bytes};

    fn code(c: char) -> usize {
        (c as usize) - 32
    }

    #[test]
    fn font_import_fixture() -> Result<(), Box<dyn Error>> {
        let plan = import(&parse("alpha.c", alpha_c(GLYPH_BYTES).as_bytes())?)?;
        assert_eq!(plan.blob.len(), 23_040);
        for ((glyph, chunk), name) in plan.blob.chunks(GLYPH_BYTES).enumerate().zip(GLYPH_NAMES) {
            assert_eq!(chunk, glyph_bytes(glyph), "glyph {name}");
        }
        assert_eq!(plan.glyph_index.len(), 95);
        let index = |c| plan.glyph_index.get(code(c)).copied().flatten();
        assert_eq!(index('A'), Some(0));
        assert_eq!(index('Z'), Some(25));
        assert_eq!(index('0'), Some(26));
        assert_eq!(index('9'), Some(35));
        assert_eq!(index('.'), Some(36));
        assert_eq!(index(':'), Some(37));
        assert_eq!(index('_'), Some(38));
        assert_eq!(index(' '), Some(39));
        assert_eq!(index('-'), None);
        assert_eq!(plan.glyph_index.iter().flatten().count(), 66);
        Ok(())
    }

    #[test]
    fn font_lowercase_maps_to_uppercase_glyph() -> Result<(), Box<dyn Error>> {
        let plan = import(&parse("alpha.c", alpha_c(GLYPH_BYTES).as_bytes())?)?;
        for (lower, upper) in ('a'..='z').zip('A'..='Z') {
            let lower = plan.glyph_index.get(code(lower)).copied().flatten();
            assert!(lower.is_some());
            assert_eq!(lower, plan.glyph_index.get(code(upper)).copied().flatten());
        }
        Ok(())
    }

    #[test]
    fn font_wrong_glyph_size_is_error() -> Result<(), Box<dyn Error>> {
        let error = import(&parse("alpha.c", alpha_c(575).as_bytes())?).err();
        assert!(
            matches!(&error, Some(ImportError::Length { file, table, expected: 576, got: 575 })
                if file == "alpha.c" && table == "vgaletA"),
            "{error:?}"
        );
        Ok(())
    }

    #[test]
    fn font_char_table_naming_other_array_is_error() -> Result<(), Box<dyn Error>> {
        let text = alpha_c(GLYPH_BYTES).replace(
            "const uint8_t * const ascii2vga[0x5f]={\n  vgasymspace,",
            "const uint8_t * const ascii2vga[0x5f]={\n  cgaletA,",
        );
        let error = import(&parse("alpha.c", text.as_bytes())?).err();
        assert!(
            matches!(&error, Some(ImportError::UnknownSymbol { table, symbol, .. })
                if table == "ascii2vga" && symbol == "cgaletA"),
            "{error:?}"
        );
        Ok(())
    }
}
