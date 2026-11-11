//! Sprite images: [`ImageId`], [`ImageSize`] and [`Image`].

use super::{SPRITE_INDEX, SPRITES, byte_len};

/// The size of a sprite image.
///
/// `w` is in units of 4 logical pixels (8 real pixels) and `h` in logical
/// pixels (2 real rows), as in the upstream tables; the image and its mask
/// each hold `8w x 2h` bytes (research 03 §5.1). Sizes only come from the
/// embedded images, so no size larger than the largest image (4,15) exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageSize {
    w: u8,
    h: u8,
}

impl ImageSize {
    /// The size from the generated index.
    const fn new(w: u8, h: u8) -> Self {
        Self { w, h }
    }

    /// Width in units of 4 logical pixels.
    pub const fn w(self) -> u8 {
        self.w
    }

    /// Height in logical pixels.
    pub const fn h(self) -> u8 {
        self.h
    }

    /// Width in logical pixels (`4w`).
    pub const fn logical_width(self) -> u32 {
        4 * self.w as u32
    }

    /// Height in logical pixels (`h`).
    pub const fn logical_height(self) -> u32 {
        self.h as u32
    }

    /// Width in real pixels (`8w`), the length of one row of image bytes.
    pub const fn real_width(self) -> usize {
        8 * self.w as usize
    }

    /// Height in real pixels (`2h`), the number of rows of image bytes.
    pub const fn real_height(self) -> usize {
        2 * self.h as usize
    }

    /// Bytes in the image, and in the mask: `8w x 2h`.
    pub const fn byte_len(self) -> usize {
        byte_len(self.w, self.h)
    }
}

/// A sprite image number, 0 to 119.
///
/// The numbers keep their upstream meanings (research 03 §5.2): 0 blank,
/// 1–24 first digger, 25 dead digger, 26–30 gravestone, 31–61 the same for
/// the second digger, 62–65 bag, 66–68 gold, 69–80 monsters, 81 bonus,
/// 82–87 fireball and explosion, 88–93 padding duplicates of 82–87, 94–101
/// background tiles, 102–107 tunnel shapes, 108–109 emerald and its eraser,
/// 110–113 life icons, 114–119 second fireball.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageId(u8);

impl ImageId {
    /// Number of image numbers (research 03 §5.2).
    pub const COUNT: usize = 120;

    /// Blank digger: an opaque black 16x15 block, the initial digger image
    /// (research 03 §5.2, §8.4).
    pub const BLANK: Self = Self(0);
    /// Dead first digger, upside down (research 03 §5.2).
    pub const DIGGER_DEAD: Self = Self(25);

    /// Bag standing still (research 03 §5.2, §8.4).
    pub const BAG_STILL: Self = Self(62);
    /// Bag wobbling to the right (research 03 §5.2).
    pub const BAG_WOBBLE_RIGHT: Self = Self(63);
    /// Bag wobbling to the left (research 03 §5.2).
    pub const BAG_WOBBLE_LEFT: Self = Self(64);
    /// Bag falling (research 03 §5.2).
    pub const BAG_FALLING: Self = Self(65);
    /// Bag breaking, first frame (research 03 §5.2).
    pub const GOLD_1: Self = Self(66);
    /// Bag breaking, second frame (research 03 §5.2).
    pub const GOLD_2: Self = Self(67);
    /// Gold pile, last frame (research 03 §5.2).
    pub const GOLD_3: Self = Self(68);

    /// Nobbin, first animation frame; 70 and 71 follow (research 03 §5.2,
    /// §8.4).
    pub const NOBBIN_1: Self = Self(69);
    /// Nobbin, second animation frame (research 03 §5.2).
    pub const NOBBIN_2: Self = Self(70);
    /// Nobbin, third animation frame (research 03 §5.2).
    pub const NOBBIN_3: Self = Self(71);
    /// Dead nobbin (research 03 §5.2).
    pub const NOBBIN_DEAD: Self = Self(72);
    /// Hobbin facing right, first frame (research 03 §5.2).
    pub const HOBBIN_RIGHT_1: Self = Self(73);
    /// Hobbin facing right, second frame (research 03 §5.2).
    pub const HOBBIN_RIGHT_2: Self = Self(74);
    /// Hobbin facing right, third frame (research 03 §5.2).
    pub const HOBBIN_RIGHT_3: Self = Self(75);
    /// Dead hobbin facing right (research 03 §5.2).
    pub const HOBBIN_RIGHT_DEAD: Self = Self(76);
    /// Hobbin facing left, first frame (research 03 §5.2).
    pub const HOBBIN_LEFT_1: Self = Self(77);
    /// Hobbin facing left, second frame (research 03 §5.2).
    pub const HOBBIN_LEFT_2: Self = Self(78);
    /// Hobbin facing left, third frame (research 03 §5.2).
    pub const HOBBIN_LEFT_3: Self = Self(79);
    /// Dead hobbin facing left; its size is 4,14, one row shorter than the
    /// other creatures (research 03 §5.2, §8.4).
    pub const HOBBIN_LEFT_DEAD: Self = Self(80);

    /// Bonus cherry on an opaque 16x15 block (research 03 §5.2, §8.4).
    pub const BONUS: Self = Self(81);

    /// First digger's fireball, first frame (research 03 §5.2, §8.4).
    pub const FIREBALL_1: Self = Self(82);
    /// First digger's fireball, second frame (research 03 §5.2).
    pub const FIREBALL_2: Self = Self(83);
    /// First digger's fireball, third frame (research 03 §5.2).
    pub const FIREBALL_3: Self = Self(84);
    /// First digger's explosion, first frame (research 03 §5.2, §8.4).
    pub const EXPLOSION_1: Self = Self(85);
    /// First digger's explosion, second frame (research 03 §5.2).
    pub const EXPLOSION_2: Self = Self(86);
    /// First digger's explosion, third frame (research 03 §5.2).
    pub const EXPLOSION_3: Self = Self(87);

    /// Tunnel carved when moving right, 8x18 (research 03 §5.2, §8.3).
    pub const TUNNEL_RIGHT: Self = Self(102);
    /// Tunnel carved when moving up, 24x6 (research 03 §5.2, §8.3).
    pub const TUNNEL_UP: Self = Self(103);
    /// Tunnel carved when moving left, 8x18 (research 03 §5.2, §8.3).
    pub const TUNNEL_LEFT: Self = Self(104);
    /// Tunnel carved when moving down, 24x6 (research 03 §5.2, §8.3).
    pub const TUNNEL_DOWN: Self = Self(105);
    /// "Square" tunnel shape of a falling bag, 24x6 (research 03 §5.2, §8.3).
    pub const TUNNEL_SQUARE: Self = Self(106);
    /// "Furry" tunnel shape of a falling bag, 24x8 (research 03 §5.2, §8.3).
    pub const TUNNEL_FURRY: Self = Self(107);

    /// Emerald, 16x10 (research 03 §5.2, §8.4).
    pub const EMERALD: Self = Self(108);
    /// Emerald eraser: the emerald silhouette in black (research 03 §5.2,
    /// §8.4).
    pub const EMERALD_ERASER: Self = Self(109);

    /// Life icon of player 1 (research 03 §5.2, §8.5).
    pub const LIFE_PLAYER_1: Self = Self(110);
    /// Life icon of player 2 in alternating two-player games (research 03
    /// §5.2, §8.5).
    pub const LIFE_PLAYER_2: Self = Self(111);
    /// Blank life slot, an opaque black block (research 03 §5.2, §8.5).
    pub const LIFE_BLANK: Self = Self(112);
    /// Life icon of the second digger in simultaneous games (research 03
    /// §5.2, §8.5).
    pub const LIFE_SECOND_DIGGER: Self = Self(113);

    /// Image number of the first background tile; plan `p` uses `93 + p`
    /// (research 03 §5.2, §8.2).
    const BACKGROUND_BASE: u8 = 93;
    /// Offset from a first-digger image to the second digger's (research 03
    /// §5.2, drawing offset `+31`).
    const SECOND_DIGGER_OFFSET: u8 = 31;
    /// Offset from a first-digger fireball image to the second digger's
    /// (research 03 §5.2, drawing offset `+32`).
    const SECOND_FIREBALL_OFFSET: u8 = 32;

    /// The image number `n`, or `None` for 120 and above.
    pub const fn new(n: u8) -> Option<Self> {
        if (n as usize) < Self::COUNT {
            Some(Self(n))
        } else {
            None
        }
    }

    /// Every image number in ascending order.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..=119).map(Self)
    }

    /// The number of this image, 0 to 119.
    pub const fn number(self) -> u8 {
        self.0
    }

    /// The background tile for level plan `plan` (1 to 8): image `93 + plan`
    /// (research 03 §5.2, §8.2); `None` for any other plan number.
    pub const fn background(plan: u8) -> Option<Self> {
        if plan >= 1 && plan <= 8 {
            Some(Self(Self::BACKGROUND_BASE + plan))
        } else {
            None
        }
    }

    /// The second digger's counterpart of a first-digger image 0 to 30
    /// (blank, frames, dead digger, gravestones): this image number plus 31
    /// (research 03 §5.2, §8.4). `None` for any other image.
    pub const fn for_second_digger(self) -> Option<Self> {
        if self.0 <= 30 {
            Some(Self(self.0 + Self::SECOND_DIGGER_OFFSET))
        } else {
            None
        }
    }

    /// The second digger's counterpart of a fireball or explosion image 82
    /// to 87: this image number plus 32 (research 03 §5.2, §8.4). `None` for
    /// any other image.
    pub const fn for_second_fireball(self) -> Option<Self> {
        if self.0 >= Self::FIREBALL_1.0 && self.0 <= Self::EXPLOSION_3.0 {
            Some(Self(self.0 + Self::SECOND_FIREBALL_OFFSET))
        } else {
            None
        }
    }

    /// The size of this image (research 03 §5.2).
    pub const fn size(self) -> ImageSize {
        let entry = self.entry();
        ImageSize::new(entry.w, entry.h)
    }

    /// The image and mask data of this image number.
    pub fn get(self) -> Image<'static> {
        let entry = self.entry();
        let len = byte_len(entry.w, entry.h);
        let image = entry.image as usize;
        let mask = entry.mask as usize;
        #[expect(
            clippy::indexing_slicing,
            reason = "the const assertion in assets/mod.rs proves every index entry lies inside sprites.bin"
        )]
        let (pixels, mask) = (&SPRITES[image..image + len], &SPRITES[mask..mask + len]);
        Image {
            size: ImageSize::new(entry.w, entry.h),
            pixels,
            mask,
        }
    }

    const fn entry(self) -> super::SpriteEntry {
        #[expect(
            clippy::indexing_slicing,
            reason = "an ImageId is below 120 by construction and SPRITE_INDEX has 120 entries"
        )]
        SPRITE_INDEX[self.0 as usize]
    }
}

/// The data of one sprite image: its size, its image bytes and its mask
/// bytes, each `8w x 2h` bytes, one byte per real pixel, row-major.
///
/// Image bytes are palette indices, or 255 for transparent; mask bytes are 0
/// (replace) or 255 (keep) (research 03 §4.5, §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Image<'a> {
    size: ImageSize,
    pixels: &'a [u8],
    mask: &'a [u8],
}

impl<'a> Image<'a> {
    /// The image size.
    pub const fn size(&self) -> ImageSize {
        self.size
    }

    /// The image bytes, `size().byte_len()` of them.
    pub const fn pixels(&self) -> &'a [u8] {
        self.pixels
    }

    /// The mask bytes, `size().byte_len()` of them.
    pub const fn mask(&self) -> &'a [u8] {
        self.mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> ImageId {
        ImageId::new(n).unwrap_or_else(|| unreachable!("test image number {n} is valid"))
    }

    #[test]
    fn image_id_rejects_120() {
        assert_eq!(ImageId::new(119).map(ImageId::number), Some(119));
        assert_eq!(ImageId::new(120), None);
        assert_eq!(ImageId::new(255), None);
    }

    #[test]
    fn image_count_is_120() {
        let numbers: Vec<u8> = ImageId::all().map(ImageId::number).collect();
        assert_eq!(numbers.len(), ImageId::COUNT);
        assert_eq!(numbers, (0..120).collect::<Vec<u8>>());
    }

    #[test]
    fn image_byte_size_matches_dimensions() {
        for image in ImageId::all() {
            let data = image.get();
            let size = data.size();
            assert_eq!(size, image.size());
            let expected = 8 * usize::from(size.w()) * 2 * usize::from(size.h());
            assert_eq!(size.byte_len(), expected, "image {}", image.number());
            assert_eq!(data.pixels().len(), expected, "image {}", image.number());
            assert_eq!(data.mask().len(), expected, "mask {}", image.number());
            assert_eq!(size.real_width() * size.real_height(), expected);
        }
        let one = id(1).size();
        assert_eq!((one.w(), one.h()), (4, 15));
        assert_eq!(one.byte_len(), 960);
        assert_eq!((one.real_height(), one.real_width()), (30, 32));
    }

    /// The size table of the spec "Image dimensions by number".
    #[test]
    fn image_sizes_by_number() {
        for image in ImageId::all() {
            let expected = match image.number() {
                0..=79 | 81 => (4, 15),
                80 => (4, 14),
                82..=93 | 114..=119 => (2, 8),
                94..=101 => (5, 4),
                102 | 104 => (2, 18),
                103 | 105 | 106 => (6, 6),
                107 => (6, 8),
                108 | 109 => (4, 10),
                110..=113 => (4, 12),
                n => unreachable!("image number {n} out of range"),
            };
            let size = image.size();
            assert_eq!((size.w(), size.h()), expected, "image {}", image.number());
        }
        let creature = id(69).size();
        assert_eq!(
            (creature.logical_width(), creature.logical_height()),
            (16, 15)
        );
        assert_eq!((creature.real_width(), creature.real_height()), (32, 30));
        assert_eq!(ImageId::HOBBIN_LEFT_DEAD.size().byte_len(), 896);
        assert_eq!(id(82).size().byte_len(), 256);
        assert_eq!(id(94).size().byte_len(), 320);
        assert_eq!(id(102).size().byte_len(), 576);
        assert_eq!(id(103).size().byte_len(), 576);
        assert_eq!(id(107).size().byte_len(), 768);
    }

    #[test]
    fn named_constants_match_upstream_numbers() {
        let named = [
            (ImageId::BLANK, 0),
            (ImageId::DIGGER_DEAD, 25),
            (ImageId::BAG_STILL, 62),
            (ImageId::BAG_WOBBLE_RIGHT, 63),
            (ImageId::BAG_WOBBLE_LEFT, 64),
            (ImageId::BAG_FALLING, 65),
            (ImageId::GOLD_1, 66),
            (ImageId::GOLD_2, 67),
            (ImageId::GOLD_3, 68),
            (ImageId::NOBBIN_1, 69),
            (ImageId::NOBBIN_2, 70),
            (ImageId::NOBBIN_3, 71),
            (ImageId::NOBBIN_DEAD, 72),
            (ImageId::HOBBIN_RIGHT_1, 73),
            (ImageId::HOBBIN_RIGHT_2, 74),
            (ImageId::HOBBIN_RIGHT_3, 75),
            (ImageId::HOBBIN_RIGHT_DEAD, 76),
            (ImageId::HOBBIN_LEFT_1, 77),
            (ImageId::HOBBIN_LEFT_2, 78),
            (ImageId::HOBBIN_LEFT_3, 79),
            (ImageId::HOBBIN_LEFT_DEAD, 80),
            (ImageId::BONUS, 81),
            (ImageId::FIREBALL_1, 82),
            (ImageId::FIREBALL_2, 83),
            (ImageId::FIREBALL_3, 84),
            (ImageId::EXPLOSION_1, 85),
            (ImageId::EXPLOSION_2, 86),
            (ImageId::EXPLOSION_3, 87),
            (ImageId::TUNNEL_RIGHT, 102),
            (ImageId::TUNNEL_UP, 103),
            (ImageId::TUNNEL_LEFT, 104),
            (ImageId::TUNNEL_DOWN, 105),
            (ImageId::TUNNEL_SQUARE, 106),
            (ImageId::TUNNEL_FURRY, 107),
            (ImageId::EMERALD, 108),
            (ImageId::EMERALD_ERASER, 109),
            (ImageId::LIFE_PLAYER_1, 110),
            (ImageId::LIFE_PLAYER_2, 111),
            (ImageId::LIFE_BLANK, 112),
            (ImageId::LIFE_SECOND_DIGGER, 113),
        ];
        for (image, number) in named {
            assert_eq!(image.number(), number);
        }
        for plan in 1..=8 {
            assert_eq!(
                ImageId::background(plan).map(ImageId::number),
                Some(93 + plan)
            );
        }
        assert_eq!(ImageId::background(0), None);
        assert_eq!(ImageId::background(9), None);
    }

    #[test]
    fn second_digger_offset_is_31() {
        for n in 0..=30 {
            let second = id(n).for_second_digger();
            assert_eq!(second.map(ImageId::number), Some(n + 31), "image {n}");
            assert_eq!(second.map(ImageId::size), Some(id(n).size()));
        }
        for n in 31..120 {
            assert_eq!(id(n).for_second_digger(), None, "image {n}");
        }
    }

    #[test]
    fn second_fireball_offset_is_32() {
        for n in 82..=87 {
            let second = id(n).for_second_fireball();
            assert_eq!(second.map(ImageId::number), Some(n + 32), "image {n}");
            assert_eq!(second.map(ImageId::size), Some(id(n).size()));
        }
        for n in (0..82).chain(88..120) {
            assert_eq!(id(n).for_second_fireball(), None, "image {n}");
        }
    }

    /// Quirk A1: 57–61 duplicate 26–30 and 88–93 duplicate 82–87, so that the
    /// +31 and +32 offsets hold.
    #[test]
    fn duplicate_image_numbers() {
        for (copy, original) in (57..=61).zip(26..=30).chain((88..=93).zip(82..=87)) {
            let (a, b) = (id(copy).get(), id(original).get());
            assert_eq!(a.size(), b.size(), "images {copy} and {original}");
            assert_eq!(a.pixels(), b.pixels(), "images {copy} and {original}");
            assert_eq!(a.mask(), b.mask(), "masks {copy} and {original}");
        }
    }
}
