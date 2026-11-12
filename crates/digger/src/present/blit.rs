//! Palette indices through a 16-entry colour table into XRGB rows with black
//! borders (design D5).
//!
//! The output is one `u32` per window pixel, `0x00RRGGBB`, row after row,
//! as softbuffer takes it. Pixels are replicated, never smoothed: each
//! window pixel of the viewport shows the picture pixel the layout's column
//! and row maps name, and everything outside the viewport is black.

use digger_core::video::{Framebuffer, Rgb};

use super::scale::{Layout, PICTURE_WIDTH};

/// Black in the output format.
pub(crate) const BLACK: u32 = 0;

/// The 16 colours of a palette in the output format, `0x00RRGGBB`.
pub(crate) fn palette_table(colors: &[Rgb; 16]) -> [u32; 16] {
    colors.map(|c| (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b))
}

/// Draws the 640×400 picture `pixels` through `table` into `out`, a window
/// of the layout's size, with black around the viewport.
///
/// `out` holds `width × height` pixels; if it is shorter, only the rows
/// that fit are drawn. Only the low four bits of an index are used.
pub(crate) fn blit(
    pixels: &[u8; Framebuffer::BYTES],
    table: &[u32; 16],
    layout: &Layout,
    out: &mut [u32],
) {
    out.fill(BLACK);
    let width = to_usize(layout.width());
    let view = layout.viewport();
    let (x, y, w) = (to_usize(view.x), to_usize(view.y), to_usize(view.w));
    if width == 0 || w == 0 {
        return;
    }
    let picture_width = to_usize(PICTURE_WIDTH);
    let cols = layout.col_map();
    // The picture row the previous window row shows, and where it starts.
    let mut previous: Option<(u16, usize)> = None;
    for (dy, &sy) in layout.row_map().iter().enumerate() {
        let start = (y + dy) * width + x;
        let Some(dst) = out.get_mut(start..start + w) else {
            break;
        };
        match previous {
            Some((prev_sy, prev_start)) if prev_sy == sy => {
                out.copy_within(prev_start..prev_start + w, start);
            }
            _ => {
                let src_start = usize::from(sy) * picture_width;
                let src = pixels
                    .get(src_start..src_start + picture_width)
                    .unwrap_or_default();
                for (d, &sx) in dst.iter_mut().zip(cols) {
                    *d = src
                        .get(usize::from(sx))
                        .and_then(|&index| table.get(usize::from(index & 0x0F)))
                        .copied()
                        .unwrap_or(BLACK);
                }
            }
        }
        previous = Some((sy, start));
    }
}

/// A window coordinate as an index; window sizes fit in `usize` on every
/// supported platform.
fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use digger_core::video::{Intensity, Palette, colors};

    use super::*;
    use crate::config::ScaleMode;

    /// A picture of one index, on the heap.
    fn filled(index: u8) -> Box<[u8; Framebuffer::BYTES]> {
        vec![index; Framebuffer::BYTES]
            .into_boxed_slice()
            .try_into()
            .unwrap_or_else(|_| unreachable!("the vector has the picture's size"))
    }

    /// A test pattern in which neighbouring pixels differ: index
    /// `(x + 3y) mod 16`.
    fn pattern() -> Box<[u8; Framebuffer::BYTES]> {
        let mut pixels = filled(0);
        for (i, p) in pixels.iter_mut().enumerate() {
            let (x, y) = (i % 640, i / 640);
            *p = u8::try_from((x + 3 * y) % 16).unwrap_or(0);
        }
        pixels
    }

    fn table(palette: Palette, intensity: Intensity) -> [u32; 16] {
        palette_table(colors(palette, intensity))
    }

    fn draw(pixels: &[u8; Framebuffer::BYTES], table: &[u32; 16], layout: &Layout) -> Vec<u32> {
        let len = to_usize(layout.width()) * to_usize(layout.height());
        // Start from garbage, so that the borders must be written.
        let mut out = vec![0xDEAD_BEEF; len];
        blit(pixels, table, layout, &mut out);
        out
    }

    #[test]
    fn palette_table_is_xrgb() {
        let colours = [Rgb::new(0x12, 0x34, 0x56); 16];
        assert_eq!(palette_table(&colours), [0x0012_3456; 16]);
        let normal = table(Palette::Zero, Intensity::Normal);
        assert_eq!(normal[0], BLACK);
        assert!(normal.iter().all(|&c| c >> 24 == 0));
    }

    #[test]
    fn blit_scale_1() {
        let pixels = pattern();
        let table = table(Palette::Zero, Intensity::Normal);
        let layout = Layout::new(640, 400, ScaleMode::Integer);
        let out = draw(&pixels, &table, &layout);
        for (i, (&o, &p)) in out.iter().zip(pixels.iter()).enumerate() {
            assert_eq!(o, table[usize::from(p)], "pixel {i}");
        }
    }

    #[test]
    fn blit_scale_2_replicates() {
        let pixels = pattern();
        let table = table(Palette::Zero, Intensity::Normal);
        let layout = Layout::new(1280, 800, ScaleMode::Integer);
        let out = draw(&pixels, &table, &layout);
        for dy in 0..800 {
            for dx in 0..1280 {
                let p = pixels[(dy / 2) * 640 + dx / 2];
                assert_eq!(out[dy * 1280 + dx], table[usize::from(p)], "({dx}, {dy})");
            }
        }
    }

    #[test]
    fn blit_non_integer_follows_the_maps() {
        let pixels = pattern();
        let table = table(Palette::Zero, Intensity::High);
        for (w, h, mode) in [
            (1000, 700, ScaleMode::Fit),
            (1366, 768, ScaleMode::Stretch),
            (500, 400, ScaleMode::Integer),
        ] {
            let layout = Layout::new(w, h, mode);
            let out = draw(&pixels, &table, &layout);
            let v = layout.viewport();
            let width = to_usize(w);
            for (dy, &sy) in layout.row_map().iter().enumerate() {
                for (dx, &sx) in layout.col_map().iter().enumerate() {
                    let p = pixels[usize::from(sy) * 640 + usize::from(sx)];
                    let at = (to_usize(v.y) + dy) * width + to_usize(v.x) + dx;
                    assert_eq!(out[at], table[usize::from(p)], "{w}x{h} ({dx}, {dy})");
                }
            }
        }
    }

    #[test]
    fn blit_borders_black() {
        // Index 15 is white: every picture pixel is visibly not black.
        let pixels = filled(15);
        let table = table(Palette::Zero, Intensity::Normal);
        assert_ne!(table[15], BLACK);
        for (w, h, mode) in [
            (1920, 1080, ScaleMode::Integer),
            (1920, 1080, ScaleMode::Fit),
            (1300, 1300, ScaleMode::Integer),
            (500, 400, ScaleMode::Integer),
        ] {
            let layout = Layout::new(w, h, mode);
            let out = draw(&pixels, &table, &layout);
            let v = layout.viewport();
            let mut black = 0;
            for (i, &o) in out.iter().enumerate() {
                let (x, y) = (i % to_usize(w), i / to_usize(w));
                let inside = (to_usize(v.x)..to_usize(v.x + v.w)).contains(&x)
                    && (to_usize(v.y)..to_usize(v.y + v.h)).contains(&y);
                if inside {
                    assert_eq!(o, table[15], "{w}x{h} {mode} ({x}, {y})");
                } else {
                    assert_eq!(o, BLACK, "{w}x{h} {mode} ({x}, {y})");
                    black += 1;
                }
            }
            assert_eq!(black, w * h - v.w * v.h);
        }
        // 1920×1080 integer: 320 columns each side, 140 rows above and
        // below.
        let layout = Layout::new(1920, 1080, ScaleMode::Integer);
        let out = draw(&pixels, &table, &layout);
        assert_eq!(out[140 * 1920 + 319], BLACK);
        assert_eq!(out[140 * 1920 + 320], table[15]);
        assert_eq!(out[139 * 1920 + 960], BLACK);
        assert_eq!(out[939 * 1920 + 1599], table[15]);
        assert_eq!(out[940 * 1920 + 1599], BLACK);
    }

    #[test]
    fn blit_palette_switch() {
        let pixels = pattern();
        let layout = Layout::new(1280, 800, ScaleMode::Integer);
        let zero = table(Palette::Zero, Intensity::Normal);
        let mut out = draw(&pixels, &zero, &layout);
        for (palette, intensity) in [
            (Palette::One, Intensity::Normal),
            (Palette::Zero, Intensity::High),
            (Palette::One, Intensity::High),
        ] {
            let next = table(palette, intensity);
            assert_ne!(next, zero);
            // Same buffer, same pixels: every pixel takes the new colours.
            blit(&pixels, &next, &layout, &mut out);
            for dy in 0..800 {
                for dx in 0..1280 {
                    let p = pixels[(dy / 2) * 640 + dx / 2];
                    assert_eq!(out[dy * 1280 + dx], next[usize::from(p)]);
                }
            }
        }
    }

    #[test]
    fn blit_tolerates_short_and_empty_buffers() {
        let pixels = pattern();
        let table = table(Palette::Zero, Intensity::Normal);
        let layout = Layout::new(640, 400, ScaleMode::Integer);
        let mut short = vec![1_u32; 640 * 10 + 5];
        blit(&pixels, &table, &layout, &mut short);
        assert_eq!(
            short[640 * 9 + 639],
            table[usize::from(pixels[640 * 9 + 639])]
        );
        assert_eq!(short[640 * 10], BLACK);
        let empty = Layout::new(0, 0, ScaleMode::Integer);
        blit(&pixels, &table, &empty, &mut []);
        // An index above 15 uses its low four bits.
        let mut high = filled(0x1F);
        high[0] = 0xF2;
        let out = draw(&high, &table, &layout);
        assert_eq!(out[0], table[2]);
        assert_eq!(out[1], table[15]);
    }
}
