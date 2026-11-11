//! Unit tests of the picture: one per scenario of the rendering spec that
//! this module covers.

use super::*;
use crate::assets::{Image, ImageId};
use crate::geom::Px;

const W: usize = Framebuffer::WIDTH;
const H: usize = Framebuffer::HEIGHT;

fn id(n: u8) -> ImageId {
    ImageId::new(n).unwrap_or_else(|| unreachable!("test image number {n} is valid"))
}

/// A picture whose every pixel is a non-zero index that varies with both
/// coordinates.
fn patterned() -> Framebuffer {
    let mut fb = Framebuffer::new();
    for (i, pixel) in fb.pixels_mut().iter_mut().enumerate() {
        let (column, row) = (i % W, i / W);
        *pixel = u8::try_from(1 + (column * 7 + row * 3) % 15)
            .unwrap_or_else(|_| unreachable!("the value is below 16"));
    }
    fb
}

/// A picture of one index everywhere.
fn filled(value: u8) -> Framebuffer {
    let mut fb = Framebuffer::new();
    fb.pixels_mut().fill(value);
    fb
}

fn pixel(fb: &Framebuffer, column: usize, row: usize) -> u8 {
    fb.pixels()
        .get(row * W + column)
        .copied()
        .unwrap_or_else(|| unreachable!("({column}, {row}) is inside the picture"))
}

fn set_pixel(fb: &mut Framebuffer, column: usize, row: usize, value: u8) {
    if let Some(slot) = fb.pixels_mut().get_mut(row * W + column) {
        *slot = value;
    } else {
        unreachable!("({column}, {row}) is inside the picture");
    }
}

/// The picture after a masked draw, computed pixel by pixel from real
/// coordinates, independently of the clipping code.
fn oracle_blit(before: &Framebuffer, image: Image<'_>, at: Px) -> Vec<u8> {
    let mut after = before.pixels().to_vec();
    let size = image.size();
    let stride = size.real_width();
    for (i, (&value, &keep)) in image.pixels().iter().zip(image.mask()).enumerate() {
        let x = 2 * i64::from(at.x) + i64::try_from(i % stride).unwrap_or(i64::MAX);
        let y = 2 * i64::from(at.y) + i64::try_from(i / stride).unwrap_or(i64::MAX);
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            continue;
        };
        if x >= W || y >= H || value == 0xFF {
            continue;
        }
        if let Some(slot) = after.get_mut(y * W + x) {
            *slot = (*slot & keep) | value;
        }
    }
    after
}

/// Real pixels that differ between two pictures, as (column, row).
fn changed(before: &[u8], after: &[u8]) -> Vec<(usize, usize)> {
    before
        .iter()
        .zip(after)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| (i % W, i / W))
        .collect()
}

/// Parses a spec scenario's colour list, "(r,g,b), (r,g,b), …".
fn parse_colours(text: &str) -> Vec<Rgb> {
    text.split("), (")
        .map(|entry| {
            let channels: Vec<u8> = entry
                .trim_matches(|c| c == '(' || c == ')')
                .split(',')
                .map(|n| {
                    n.trim()
                        .parse()
                        .unwrap_or_else(|_| unreachable!("bad colour {entry}"))
                })
                .collect();
            match channels.as_slice() {
                &[r, g, b] => Rgb::new(r, g, b),
                _ => unreachable!("bad colour {entry}"),
            }
        })
        .collect()
}

// 5.1 Framebuffer geometry, initial state and clearing.

#[test]
fn new_framebuffer_is_black_palette_zero() {
    let fb = Framebuffer::new();
    assert_eq!(fb.pixels().len(), 256_000);
    assert_eq!(Framebuffer::BYTES, 400 * 640);
    assert!(fb.pixels().iter().all(|&p| p == 0));
    assert_eq!(fb.palette(), Palette::Zero);
    assert_eq!(fb.intensity(), Intensity::Normal);
    assert_eq!(fb.colors(), colors(Palette::Zero, Intensity::Normal));
    assert_eq!(Framebuffer::default(), fb);
}

#[test]
fn clear_keeps_palette() {
    let mut fb = Framebuffer::new();
    fb.set_palette(Palette::One);
    fb.blit_masked(ImageId::BAG_STILL, Px::new(100, 50));
    assert!(fb.pixels().iter().any(|&p| p != 0));
    fb.clear();
    assert!(fb.pixels().iter().all(|&p| p == 0));
    assert_eq!(fb.palette(), Palette::One);
    assert_eq!(fb.intensity(), Intensity::Normal);

    fb.set_intensity(Intensity::High);
    fb.clear();
    assert_eq!(fb.intensity(), Intensity::High);
}

// 5.2 Palettes and intensity.

#[test]
fn palette_tables_match_spec() {
    // The colour lists of the four "Palette colour values" scenarios,
    // verbatim.
    let spec = [
        (
            Palette::Zero,
            Intensity::Normal,
            "(0,0,0), (0,0,128), (0,128,0), (0,128,128), (128,0,0), (128,0,128), (128,64,0), (128,128,128), (64,64,64), (0,0,255), (0,255,0), (0,255,255), (255,0,0), (255,0,255), (255,255,0), (255,255,255)",
        ),
        (
            Palette::Zero,
            Intensity::High,
            "(0,0,0), (0,0,255), (0,255,0), (0,255,255), (255,0,0), (255,0,255), (255,128,0), (196,196,196), (128,128,128), (128,128,255), (128,255,128), (128,255,255), (255,128,128), (255,128,255), (255,255,128), (255,255,255)",
        ),
        (
            Palette::One,
            Intensity::Normal,
            "(0,0,0), (0,128,0), (128,0,0), (128,64,0), (0,0,128), (0,128,128), (128,0,128), (128,128,128), (64,64,64), (0,255,0), (255,0,0), (255,255,0), (0,0,255), (0,255,255), (255,0,255), (255,255,255)",
        ),
        (
            Palette::One,
            Intensity::High,
            "(0,0,0), (0,255,0), (255,0,0), (255,128,0), (0,0,255), (0,255,255), (255,0,255), (196,196,196), (128,128,128), (128,255,128), (255,128,128), (255,255,128), (128,128,255), (128,255,255), (255,128,255), (255,255,255)",
        ),
    ];
    for (palette, intensity, text) in spec {
        let expected = parse_colours(text);
        assert_eq!(expected.len(), 16);
        assert_eq!(
            colors(palette, intensity).as_slice(),
            expected.as_slice(),
            "{palette:?} {intensity:?}"
        );
        let table = colors(palette, intensity);
        assert_eq!(table.first(), Some(&Rgb::new(0, 0, 0)));
        assert_eq!(table.last(), Some(&Rgb::new(255, 255, 255)));
    }
    assert_eq!(Palette::from_number(0), Some(Palette::Zero));
    assert_eq!(Palette::from_number(1), Some(Palette::One));
    assert_eq!(Palette::from_number(2), None);
    assert_eq!(Palette::One.number(), 1);
}

#[test]
fn palette_select_resets_intensity() {
    let mut fb = Framebuffer::new();
    fb.set_intensity(Intensity::High);
    assert_eq!(fb.colors(), colors(Palette::Zero, Intensity::High));
    fb.set_palette(Palette::Zero);
    assert_eq!(fb.intensity(), Intensity::Normal);
    assert_eq!(fb.colors(), colors(Palette::Zero, Intensity::Normal));

    fb.set_intensity(Intensity::High);
    fb.set_palette(Palette::One);
    assert_eq!(fb.colors(), colors(Palette::One, Intensity::Normal));
}

#[test]
fn intensity_keeps_palette() {
    let mut fb = Framebuffer::new();
    fb.set_palette(Palette::One);
    fb.set_intensity(Intensity::High);
    assert_eq!(fb.palette(), Palette::One);
    assert_eq!(fb.colors(), colors(Palette::One, Intensity::High));
    fb.set_intensity(Intensity::Normal);
    assert_eq!(fb.colors(), colors(Palette::One, Intensity::Normal));
}

#[test]
fn palette_change_leaves_pixels() {
    let mut fb = patterned();
    let before = fb.crc32();
    let pixels = fb.pixels().to_vec();
    fb.set_palette(Palette::One);
    fb.set_intensity(Intensity::High);
    fb.set_palette(Palette::Zero);
    fb.set_intensity(Intensity::High);
    assert_eq!(fb.crc32(), before);
    assert_eq!(fb.pixels().as_slice(), pixels.as_slice());
}

#[test]
fn whole_frame_follows_palette() {
    let mut fb = filled(10);
    let expand = |fb: &Framebuffer| -> Vec<Rgb> {
        fb.pixels()
            .iter()
            .map(|&p| {
                fb.colors()
                    .get(usize::from(p))
                    .copied()
                    .unwrap_or_else(|| unreachable!("index {p} is below 16"))
            })
            .collect()
    };
    assert!(expand(&fb).iter().all(|&c| c == Rgb::new(0, 255, 0)));
    fb.set_palette(Palette::One);
    assert!(expand(&fb).iter().all(|&c| c == Rgb::new(255, 0, 0)));
}

// 5.3 Frame checksum.

#[test]
fn crc32_of_cleared_frame() {
    // CRC-32 of 256000 zero bytes, computed independently with zlib.
    const ZEROS_256000: u32 = 0x69FF_78A6;
    let mut fb = patterned();
    fb.clear();
    assert_eq!(fb.crc32(), ZEROS_256000);
    assert_eq!(Framebuffer::new().crc32(), ZEROS_256000);
    assert_eq!(crc32(&vec![0; 256_000]), ZEROS_256000);
}

#[test]
fn crc32_sensitive_to_one_pixel() {
    let mut fb = Framebuffer::new();
    let blank = fb.crc32();
    set_pixel(&mut fb, 639, 399, 1);
    assert_ne!(fb.crc32(), blank);
    set_pixel(&mut fb, 639, 399, 0);
    assert_eq!(fb.crc32(), blank);
}

// 5.4 Masked drawing and clipping.

#[test]
fn blit_rule_truth_table() {
    // (image byte, mask byte, destination before, destination after)
    let table = [
        (0xFF, 0x00, 6, 6),    // transparent pixel
        (0xFF, 0xFF, 6, 6),    // transparent under a keep mask
        (14, 0x00, 6, 14),     // opaque coloured pixel
        (0, 0x00, 6, 0),       // black outline pixel
        (0, 0xFF, 6, 6),       // zero image under a keep mask
        (3, 0xFF, 4, 7),       // the full rule: (6 AND 255) OR 3
        (8, 0x0F, 0x37, 0x0F), // (0x37 AND 0x0F) OR 8
    ];
    for (value, keep, before, after) in table {
        let mut fb = filled(before);
        // A 1x1 unit image: 8 real columns, 1 real row, all the same byte.
        fb.blit_parts(Px::new(10, 10), 8, &[value; 8], &[keep; 8]);
        for column in 20..28 {
            assert_eq!(
                pixel(&fb, column, 20),
                after,
                "image {value:#04x}, mask {keep:#04x}, destination {before:#04x}"
            );
        }
        assert_eq!(pixel(&fb, 19, 20), before);
        assert_eq!(pixel(&fb, 28, 20), before);
        assert_eq!(pixel(&fb, 20, 21), before);
    }
}

#[test]
fn blit_maps_logical_to_real() {
    let image = ImageId::BAG_STILL;
    assert_eq!((image.size().w(), image.size().h()), (4, 15));
    let before = patterned();
    let mut fb = before.clone();
    let at = Px::new(12, 18);
    fb.blit_masked(image, at);
    assert_eq!(
        fb.pixels().as_slice(),
        oracle_blit(&before, image.get(), at)
    );
    let changed = changed(before.pixels(), fb.pixels());
    assert_ne!(changed.len(), 0);
    for (column, row) in changed {
        assert!((24..=55).contains(&column), "column {column}");
        assert!((36..=65).contains(&row), "row {row}");
    }
    // The image's first opaque byte lands at its real offset from (24, 36).
    let data = image.get();
    let stride = data.size().real_width();
    let (i, &value) = data
        .pixels()
        .iter()
        .enumerate()
        .find(|&(i, &v)| v != 0xFF && data.mask().get(i) == Some(&0))
        .unwrap_or_else(|| unreachable!("the bag has opaque pixels"));
    assert_eq!(pixel(&fb, 24 + i % stride, 36 + i / stride), value);
}

#[test]
fn blit_x_is_not_aligned() {
    let before = Framebuffer::new();
    let mut fb = before.clone();
    fb.blit_masked(ImageId::BONUS, Px::new(13, 7));
    assert_eq!(
        fb.pixels().as_slice(),
        oracle_blit(&before, ImageId::BONUS.get(), Px::new(13, 7))
    );
    let columns: Vec<usize> = changed(before.pixels(), fb.pixels())
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    assert!(columns.iter().all(|&c| (26..58).contains(&c)));
}

#[test]
fn blit_clips_bottom_edge() {
    let image = ImageId::TUNNEL_SQUARE;
    assert_eq!((image.size().w(), image.size().h()), (6, 6));
    let before = patterned();
    let mut fb = before.clone();
    let at = Px::new(8, 195);
    fb.blit_masked(image, at);
    assert_eq!(
        fb.pixels().as_slice(),
        oracle_blit(&before, image.get(), at)
    );
    let rows: Vec<usize> = changed(before.pixels(), fb.pixels())
        .into_iter()
        .map(|(_, r)| r)
        .collect();
    assert_ne!(rows.len(), 0);
    assert!(rows.iter().all(|&r| (390..400).contains(&r)));
    // Logical rows 195–199 of the shape are drawn: the shape's real rows 0–9.
    assert!(rows.contains(&390) && rows.contains(&399));
}

#[test]
fn blit_clips_left_edge() {
    let image = id(1);
    assert_eq!((image.size().w(), image.size().h()), (4, 15));
    let before = patterned();
    let mut fb = before.clone();
    let at = Px::new(-8, 20);
    fb.blit_masked(image, at);
    assert_eq!(
        fb.pixels().as_slice(),
        oracle_blit(&before, image.get(), at)
    );
    for (column, row) in changed(before.pixels(), fb.pixels()) {
        assert!(column < 16, "column {column}");
        assert!((40..70).contains(&row), "row {row}");
    }
    // Real column 0 shows image column 16.
    let data = image.get();
    for row in 0..30 {
        let i = row * 32 + 16;
        let (Some(&value), Some(&keep)) = (data.pixels().get(i), data.mask().get(i)) else {
            unreachable!("inside the image");
        };
        let old = pixel(&before, 0, 40 + row);
        let expected = if value == 0xFF {
            old
        } else {
            (old & keep) | value
        };
        assert_eq!(pixel(&fb, 0, 40 + row), expected, "row {row}");
    }
}

#[test]
fn blit_clips_top_and_right_edges() {
    for at in [
        Px::new(310, -5),
        Px::new(318, 100),
        Px::new(-12, -14),
        Px::new(316, 195),
    ] {
        for image in [
            ImageId::BAG_STILL,
            ImageId::TUNNEL_RIGHT,
            ImageId::TUNNEL_FURRY,
        ] {
            let before = patterned();
            let mut fb = before.clone();
            fb.blit_masked(image, at);
            assert_eq!(
                fb.pixels().as_slice(),
                oracle_blit(&before, image.get(), at),
                "image {} at {at:?}",
                image.number()
            );
        }
    }
}

#[test]
fn blit_fully_outside_is_noop() {
    let before = patterned();
    for at in [
        Px::new(400, 300),
        Px::new(-40, -40),
        Px::new(320, 0),
        Px::new(0, 200),
        Px::new(-16, 0),
        Px::new(0, -15),
        Px::new(i32::MAX, i32::MAX),
        Px::new(i32::MIN, i32::MIN),
    ] {
        let mut fb = before.clone();
        fb.blit_masked(ImageId::BAG_STILL, at);
        assert_eq!(fb, before, "{at:?}");
    }
}

// 5.5 Rectangle capture and restore.

#[test]
fn capture_in_bounds() {
    let fb = patterned();
    let size = ImageId::BAG_STILL.size();
    let saved = fb.capture(Px::new(12, 18), size);
    assert_eq!(saved.size(), size);
    assert_eq!(saved.pixels().len(), 32 * 30);
    for (i, &value) in saved.pixels().iter().enumerate() {
        assert_eq!(value, pixel(&fb, 24 + i % 32, 36 + i / 32), "byte {i}");
    }
}

#[test]
fn capture_restore_round_trip() {
    let mut fb = patterned();
    fb.blit_masked(ImageId::EMERALD, Px::new(52, 39));
    let original = fb.clone();
    for at in [Px::new(100, 60), Px::new(-6, -3), Px::new(310, 194)] {
        let saved = fb.capture(at, ImageId::BAG_STILL.size());
        assert_eq!(fb, original, "capture changes nothing");
        fb.blit_masked(ImageId::BAG_FALLING, at);
        fb.blit_masked(ImageId::FIREBALL_1, Px::new(at.x + 4, at.y + 3));
        fb.restore(at, &saved);
        assert_eq!(fb, original, "round trip at {at:?}");
    }
}

#[test]
fn capture_zero_fills_outside() {
    let fb = filled(9);
    let size = ImageId::TUNNEL_SQUARE.size();
    let saved = fb.capture(Px::new(8, 195), size);
    assert_eq!(saved.pixels().len(), 48 * 12);
    for (i, &value) in saved.pixels().iter().enumerate() {
        let row = i / 48;
        assert_eq!(value, if row < 10 { 9 } else { 0 }, "byte {i}");
    }

    let fb = patterned();
    let saved = fb.capture(Px::new(-2, -1), ImageId::BAG_STILL.size());
    for (i, &value) in saved.pixels().iter().enumerate() {
        let (column, row) = (i % 32, i / 32);
        let expected = if column < 4 || row < 2 {
            0
        } else {
            pixel(&fb, column - 4, row - 2)
        };
        assert_eq!(value, expected, "byte {i}");
    }

    let outside = fb.capture(Px::new(500, 500), ImageId::BAG_STILL.size());
    assert!(outside.pixels().iter().all(|&p| p == 0));
}

#[test]
fn restore_is_opaque() {
    let blank = Framebuffer::new();
    let saved = blank.capture(Px::new(40, 40), ImageId::BAG_STILL.size());
    let mut fb = filled(7);
    fb.restore(Px::new(40, 40), &saved);
    for row in 0..H {
        for column in 0..W {
            let inside = (80..112).contains(&column) && (80..110).contains(&row);
            assert_eq!(pixel(&fb, column, row), if inside { 0 } else { 7 });
        }
    }
}

#[test]
fn restore_uses_saved_size() {
    let source = patterned();
    let saved = source.capture(Px::new(20, 30), ImageId::BAG_STILL.size());
    let before = filled(5);
    let mut fb = before.clone();
    // Whatever is drawn at the target, exactly 32x30 real pixels are written.
    fb.blit_masked(ImageId::BONUS, Px::new(60, 60));
    let drawn = fb.clone();
    fb.restore(Px::new(60, 60), &saved);
    for row in 0..H {
        for column in 0..W {
            let inside = (120..152).contains(&column) && (120..150).contains(&row);
            let expected = if inside {
                pixel(&source, column - 120 + 40, row - 120 + 60)
            } else {
                pixel(&drawn, column, row)
            };
            assert_eq!(pixel(&fb, column, row), expected, "({column}, {row})");
        }
    }

    // A restore across the edges is clipped and writes the in-bounds part.
    let mut fb = before.clone();
    fb.restore(Px::new(310, 190), &saved);
    for (column, row) in changed(before.pixels(), fb.pixels()) {
        assert!(column >= 620 && row >= 380);
        assert_eq!(
            pixel(&fb, column, row),
            pixel(&source, column - 620 + 40, row - 380 + 60)
        );
    }
    let mut fb = before.clone();
    fb.restore(Px::new(-100, -100), &saved);
    assert_eq!(fb, before);
}

// 5.6 Pixel probe.

/// A picture whose only non-zero pixels are the given real (column, row)s.
fn with_pixels(points: &[(usize, usize)]) -> Framebuffer {
    let mut fb = Framebuffer::new();
    for &(column, row) in points {
        set_pixel(&mut fb, column, row, 4);
    }
    fb
}

#[test]
fn probe_vectors() {
    let at = Px::new(50, 30);
    let (x, y) = (100, 60);
    assert_eq!(Framebuffer::new().probe(at), ProbeBits::NONE);
    assert_eq!(filled(1).probe(at).bits(), 0xEE);
    // Either row counts.
    assert_eq!(with_pixels(&[(x, y + 1)]).probe(at).bits(), 0x80);
    // Masked columns.
    assert_eq!(
        with_pixels(&[(x + 3, y), (x + 7, y + 1)]).probe(at).bits(),
        0x00
    );
    // Each column on its own, in either row.
    for column in 0..8 {
        for row in 0..2 {
            let expected = (0x80u8 >> column) & 0xEE;
            assert_eq!(
                with_pixels(&[(x + column, y + row)]).probe(at).bits(),
                expected,
                "column {column}, row {row}"
            );
        }
    }
    // Pixels just outside the block do not count.
    let around = with_pixels(&[(x - 1, y), (x + 8, y), (x, y - 1), (x, y + 2)]);
    assert_eq!(around.probe(at), ProbeBits::NONE);
    // x is not aligned: probing x = 51 reads real columns 102–109.
    assert_eq!(
        with_pixels(&[(102, 60)]).probe(Px::new(51, 30)).bits(),
        0x80
    );
    // The probe sees sprites drawn on the picture.
    let mut fb = Framebuffer::new();
    fb.blit_masked(ImageId::BAG_STILL, Px::new(48, 28));
    let seen = (48..64).any(|x| (28..43).any(|y| !fb.probe(Px::new(x, y)).is_empty()));
    assert!(seen);
    assert!(fb.probe(Px::new(70, 50)).is_empty());
}

#[test]
fn probe_beyond_right_or_bottom_is_ff() {
    for fb in [Framebuffer::new(), filled(3)] {
        for at in [
            Px::new(320, 50),
            Px::new(10, 200),
            Px::new(320, 200),
            Px::new(1000, -5),
            Px::new(i32::MAX, 0),
            Px::new(0, i32::MAX),
        ] {
            assert_eq!(fb.probe(at), ProbeBits::OUTSIDE, "{at:?}");
            assert_eq!(fb.probe(at).bits(), 0xFF);
        }
        assert_ne!(fb.probe(Px::new(319, 199)), ProbeBits::OUTSIDE);
    }
}

#[test]
fn probe_row_wrap() {
    let fb = with_pixels(&[(0, 22)]);
    assert_eq!(fb.probe(Px::new(318, 10)).bits(), 0x08);
    // From x = 317 the block's columns 6 and 7 wrap; column 6 is seen.
    assert_eq!(fb.probe(Px::new(317, 10)).bits(), 0x02);
    // A block that ends at the row's end does not wrap.
    assert_eq!(fb.probe(Px::new(316, 10)).bits(), 0x00);
    // From x = 319 real columns 0 and 1 of the next row are bits 0x20 and
    // 0x10; the second is masked off by 0xEE.
    let fb = with_pixels(&[(0, 21), (1, 22)]);
    assert_eq!(fb.probe(Px::new(319, 10)).bits(), 0x20);
}

#[test]
fn probe_outside_buffer_reads_zero() {
    let points: Vec<(usize, usize)> = (636..640).map(|column| (column, 399)).collect();
    let fb = with_pixels(&points);
    assert_eq!(fb.probe(Px::new(318, 199)).bits(), 0xE0);
    assert_eq!(fb.probe(Px::new(319, 199)).bits(), 0xC0);
    assert_eq!(filled(1).probe(Px::new(319, 199)).bits(), 0xEE);
}

#[test]
fn probe_negative_coordinates() {
    let fb = with_pixels(&[(0, 0)]);
    assert_eq!(fb.probe(Px::new(-1, 0)).bits(), 0x20);
    assert_eq!(fb.probe(Px::new(0, -1)).bits(), 0x00);
    assert_eq!(fb.probe(Px::new(i32::MIN, i32::MIN)).bits(), 0x00);
    assert_eq!(filled(1).probe(Px::new(0, -1)).bits(), 0x00);
    // Negative x on a lower row reads the end of the previous row.
    let fb = with_pixels(&[(639, 19)]);
    assert_eq!(fb.probe(Px::new(-1, 10)).bits(), 0x40);
}

#[test]
fn probe_bits_combine() {
    let mut bits = ProbeBits::from_bits(0x80);
    bits |= ProbeBits::from_bits(0x02);
    assert_eq!(bits.bits(), 0x82);
    assert!(bits.intersects(ProbeBits::UP));
    assert!(bits.intersects(ProbeBits::DOWN));
    assert_eq!((bits & ProbeBits::DOWN).bits(), 0x02);
    assert_eq!((ProbeBits::NONE | ProbeBits::ALL), ProbeBits::ALL);
    assert!(!ProbeBits::ALL.intersects(ProbeBits::from_bits(0x11)));
}

#[test]
fn saved_rect_debug_is_short() {
    let saved = Framebuffer::new().capture(Px::new(0, 0), ImageId::BAG_STILL.size());
    assert!(format!("{saved:?}").len() < 100);
    assert!(format!("{:?}", Framebuffer::new()).contains("0x69ff78a6"));
}
