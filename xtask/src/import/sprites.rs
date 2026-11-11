//! Sprite import: the VGA image and mask arrays of `vgagrafx.c` and their
//! 240-entry index table `vgatable` (research 03 §5.1, §5.2; design D2).
//!
//! Upstream stores no image sizes, so the importer carries the fixed
//! size-by-number table and checks every array against it:
//!
//! - an array used with one size only (the image-specific arrays) must hold
//!   exactly `8w × 2h` bytes;
//! - an array used with several sizes (the shared all-zero block) must hold
//!   at least the largest of them, and each image number takes the first
//!   `8w × 2h` bytes of it.
//!
//! The planned `sprites.bin` holds each distinct array content once, in order
//! of first reference by image number (image before mask).

use std::collections::BTreeMap;

use super::ImportError;
use super::ctable::TableFile;

/// The pointer table naming the image and mask array of every image number.
pub(crate) const INDEX_TABLE: &str = "vgatable";

/// Number of image numbers the game uses.
pub(crate) const IMAGE_COUNT: usize = 120;

/// The size of an image: `w` in units of 4 logical pixels, `h` in logical
/// pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ImageSize {
    pub(crate) w: u8,
    pub(crate) h: u8,
}

impl ImageSize {
    const fn new(w: u8, h: u8) -> Self {
        Self { w, h }
    }

    /// Bytes of an image or mask of this size: `8w × 2h`, one byte per real
    /// pixel.
    pub(crate) const fn bytes(self) -> usize {
        8 * self.w as usize * 2 * self.h as usize
    }
}

/// The fixed size of image number `n` (research 03 §5.2), or `None` for a
/// number of 120 or more.
pub(crate) const fn upstream_size(n: usize) -> Option<ImageSize> {
    let size = match n {
        0..=79 | 81 => ImageSize::new(4, 15),
        80 => ImageSize::new(4, 14),
        82..=93 | 114..=119 => ImageSize::new(2, 8),
        94..=101 => ImageSize::new(5, 4),
        102 | 104 => ImageSize::new(2, 18),
        103 | 105 | 106 => ImageSize::new(6, 6),
        107 => ImageSize::new(6, 8),
        108 | 109 => ImageSize::new(4, 10),
        110..=113 => ImageSize::new(4, 12),
        _ => return None,
    };
    Some(size)
}

/// The sizes of all 120 image numbers, in order.
pub(crate) fn upstream_sizes() -> Vec<ImageSize> {
    (0..IMAGE_COUNT).filter_map(upstream_size).collect()
}

/// Where one image number's data lies in `sprites.bin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpriteEntry {
    /// Byte offset of the image.
    pub(crate) image: u32,
    /// Byte offset of the mask.
    pub(crate) mask: u32,
    /// The image's size; image and mask each span `size.bytes()` bytes.
    pub(crate) size: ImageSize,
}

/// The planned `sprites.bin` and its index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpritePlan {
    /// The de-duplicated arrays, concatenated.
    pub(crate) blob: Vec<u8>,
    /// One entry per image number.
    pub(crate) entries: Vec<SpriteEntry>,
}

/// Plans `sprites.bin` from the arrays of `source`, for image numbers whose
/// sizes are `sizes` (the upstream import passes [`upstream_sizes`]).
pub(crate) fn import(source: &TableFile, sizes: &[ImageSize]) -> Result<SpritePlan, ImportError> {
    let file = source.file();
    let pointers = source.pointers(INDEX_TABLE)?;
    let expected = 2 * sizes.len();
    if pointers.len() != expected {
        return Err(ImportError::Count {
            file: file.to_owned(),
            table: INDEX_TABLE.to_owned(),
            expected,
            got: pointers.len(),
        });
    }

    // The array names of every image number, as (image, mask) pairs.
    let mut pairs = Vec::with_capacity(sizes.len());
    for (number, [image, mask]) in pointers.as_chunks::<2>().0.iter().enumerate() {
        let name = |element: &Option<String>, what: &str| {
            element.clone().ok_or_else(|| ImportError::Syntax {
                file: file.to_owned(),
                line: source.get(INDEX_TABLE).map_or(1, |table| table.line),
                message: format!("`{INDEX_TABLE}` has no {what} for image number {number}"),
            })
        };
        pairs.push((name(image, "image")?, name(mask, "mask")?));
    }

    // Every array in order of first reference, with the sizes it is used at.
    let mut order: Vec<&str> = Vec::new();
    let mut uses: BTreeMap<&str, Vec<ImageSize>> = BTreeMap::new();
    for ((image, mask), size) in pairs.iter().zip(sizes) {
        for name in [image.as_str(), mask.as_str()] {
            let sizes_of = uses.entry(name).or_default();
            if sizes_of.is_empty() {
                order.push(name);
            }
            if !sizes_of.contains(size) {
                sizes_of.push(*size);
            }
        }
    }

    // Validate lengths and lay out the blob, de-duplicating by content.
    let mut blob = Vec::new();
    let mut offset_of_name: BTreeMap<&str, u32> = BTreeMap::new();
    let mut offset_of_content: BTreeMap<&[u8], u32> = BTreeMap::new();
    for name in order {
        let data = source.bytes(name)?;
        let sizes_used = uses.get(name).map_or(&[][..], Vec::as_slice);
        let required = sizes_used
            .iter()
            .map(|size| size.bytes())
            .max()
            .unwrap_or(0);
        let fits = if sizes_used.len() == 1 {
            data.len() == required
        } else {
            data.len() >= required
        };
        if !fits {
            return Err(ImportError::Length {
                file: file.to_owned(),
                table: name.to_owned(),
                expected: required,
                got: data.len(),
            });
        }
        let offset = if let Some(&offset) = offset_of_content.get(data) {
            offset
        } else {
            let offset = u32::try_from(blob.len()).map_err(|_| ImportError::Length {
                file: file.to_owned(),
                table: name.to_owned(),
                expected: u32::MAX as usize,
                got: blob.len(),
            })?;
            blob.extend_from_slice(data);
            offset_of_content.insert(data, offset);
            offset
        };
        offset_of_name.insert(name, offset);
    }

    let entries = pairs
        .iter()
        .zip(sizes)
        .map(|((image, mask), &size)| SpriteEntry {
            image: offset_of_name.get(image.as_str()).copied().unwrap_or(0),
            mask: offset_of_name.get(mask.as_str()).copied().unwrap_or(0),
            size,
        })
        .collect();
    Ok(SpritePlan { blob, entries })
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::{ImageSize, SpriteEntry, import, upstream_size, upstream_sizes};
    use crate::import::ImportError;
    use crate::import::ctable::parse;
    use crate::import::fixture::c_bytes;

    const SIZES: [ImageSize; 3] = [
        ImageSize::new(4, 15),
        ImageSize::new(2, 8),
        ImageSize::new(5, 4),
    ];

    fn fire() -> Vec<u8> {
        (0..256u16)
            .map(|i| {
                [0xff, 4, 6, 14]
                    .get(usize::from(i % 4))
                    .copied()
                    .unwrap_or(0)
            })
            .collect()
    }

    fn fire_mask() -> Vec<u8> {
        (0..256u16)
            .map(|i| if i % 4 == 0 { 0xff } else { 0 })
            .collect()
    }

    fn back() -> Vec<u8> {
        (0..320u16)
            .map(|i| if i % 3 == 0 { 4 } else { 6 })
            .collect()
    }

    /// Three image numbers: a blank 4,15 block whose mask is a second,
    /// identical all-zero array; a 2,8 fireball; a 5,4 tile that borrows the
    /// shared 960-byte zero array as its mask.
    fn fixture(zero_len: usize) -> String {
        let mut text = String::from("#include \"def.h\"\n\n");
        text += &c_bytes("vgazero480", &vec![0; zero_len]);
        text += &c_bytes("vgaediggermask", &[0; 960]);
        text += &c_bytes("vgafire1", &fire());
        text += &c_bytes("vgafire1mask", &fire_mask());
        text += &c_bytes("vgaback1", &back());
        text += "\nconst uint8_t *vgatable[]={\n\
                 \x20 vgazero480, vgaediggermask, /* 0 */\n\
                 \x20 vgafire1, vgafire1mask,\n\
                 \x20 vgaback1, vgazero480};\n";
        text
    }

    #[test]
    fn upstream_size_table() {
        assert_eq!(upstream_sizes().len(), 120);
        assert_eq!(upstream_size(80), Some(ImageSize::new(4, 14)));
        assert_eq!(upstream_size(81), Some(ImageSize::new(4, 15)));
        assert_eq!(upstream_size(107).map(ImageSize::bytes), Some(768));
        assert_eq!(upstream_size(120), None);
    }

    #[test]
    fn sprites_import_fixture() -> Result<(), Box<dyn Error>> {
        let source = parse("vgagrafx.c", fixture(960).as_bytes())?;
        let plan = import(&source, &SIZES)?;
        // The two all-zero arrays collapse into one.
        let mut expected = vec![0; 960];
        expected.extend(fire());
        expected.extend(fire_mask());
        expected.extend(back());
        assert_eq!(plan.blob, expected);
        assert_eq!(
            plan.entries,
            [
                SpriteEntry {
                    image: 0,
                    mask: 0,
                    size: SIZES[0]
                },
                SpriteEntry {
                    image: 960,
                    mask: 1216,
                    size: SIZES[1]
                },
                SpriteEntry {
                    image: 1472,
                    mask: 0,
                    size: SIZES[2]
                },
            ]
        );
        Ok(())
    }

    #[test]
    fn sprites_wrong_length_is_error() -> Result<(), Box<dyn Error>> {
        // An image-specific array must be exact: 959 bytes for a 4,15 image.
        let text = fixture(960).replace(
            &c_bytes("vgaediggermask", &[0; 960]),
            &c_bytes("vgaediggermask", &[0; 959]),
        );
        let error = import(&parse("vgagrafx.c", text.as_bytes())?, &SIZES).err();
        assert!(
            matches!(&error, Some(ImportError::Length { file, table, expected: 960, got: 959 })
                if file == "vgagrafx.c" && table == "vgaediggermask"),
            "{error:?}"
        );
        let message = error.map(|e| e.to_string()).unwrap_or_default();
        assert!(
            message.contains("vgaediggermask") && message.contains("960"),
            "{message}"
        );

        // ... and so must one that is one byte too long.
        let text = fixture(960).replace(
            &c_bytes("vgafire1", &fire()),
            &c_bytes("vgafire1", &[fire(), vec![0]].concat()),
        );
        let error = import(&parse("vgagrafx.c", text.as_bytes())?, &SIZES).err();
        assert!(
            matches!(&error, Some(ImportError::Length { table, expected: 256, got: 257, .. })
                if table == "vgafire1"),
            "{error:?}"
        );

        // A shared array must hold at least its largest use ...
        let error = import(&parse("vgagrafx.c", fixture(959).as_bytes())?, &SIZES).err();
        assert!(
            matches!(&error, Some(ImportError::Length { table, expected: 960, got: 959, .. })
                if table == "vgazero480"),
            "{error:?}"
        );
        // ... and may be longer.
        let plan = import(&parse("vgagrafx.c", fixture(1000).as_bytes())?, &SIZES)?;
        assert_eq!(plan.blob.len(), 1000 + 256 + 256 + 320 + 960);
        Ok(())
    }

    #[test]
    fn sprites_wrong_entry_count_is_error() -> Result<(), Box<dyn Error>> {
        let source = parse("vgagrafx.c", fixture(960).as_bytes())?;
        let error = import(&source, &SIZES[..2]).err();
        assert!(
            matches!(&error, Some(ImportError::Count { table, expected: 4, got: 6, .. })
                if table == "vgatable"),
            "{error:?}"
        );
        Ok(())
    }

    #[test]
    fn sprites_null_entry_is_error() -> Result<(), Box<dyn Error>> {
        let text = fixture(960).replace("vgaback1, vgazero480}", "vgaback1, NULL}");
        let error = import(&parse("vgagrafx.c", text.as_bytes())?, &SIZES).err();
        assert!(
            matches!(&error, Some(ImportError::Syntax { message, .. })
                if message.contains("mask for image number 2")),
            "{error:?}"
        );
        Ok(())
    }
}
