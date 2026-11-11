//! The picture: 640x400 palette indices, addressed in logical coordinates.

use std::ops::{BitAnd, BitOr, BitOrAssign};

use super::crc::crc32;
use super::palette::{Intensity, Palette, Rgb, colors};
use super::saved::SavedRect;
use crate::assets::{ImageId, ImageSize};
use crate::geom::Px;

/// Real columns of the picture.
const WIDTH: usize = 640;
/// Real rows of the picture.
const HEIGHT: usize = 400;
/// Pixels in the picture.
const BYTES: usize = WIDTH * HEIGHT;

/// The game's picture: 640x400 palette indices, row-major, together with the
/// selected [`Palette`] and [`Intensity`] (research 03 §2–§4).
///
/// Every drawing operation takes logical coordinates: logical (x, y) is real
/// column 2x and real row 2y. Operations clip to the picture, so no
/// coordinate can make one fail. The picture is game state, not only output:
/// the [`probe`](Self::probe) reads it back, so its contents are exact
/// whether or not anything is ever displayed.
#[derive(Clone, PartialEq, Eq)]
pub struct Framebuffer {
    pixels: Box<[u8; BYTES]>,
    palette: Palette,
    intensity: Intensity,
}

impl Framebuffer {
    /// Real columns: 640.
    pub const WIDTH: usize = WIDTH;
    /// Real rows: 400.
    pub const HEIGHT: usize = HEIGHT;
    /// Pixels in the picture: 256000.
    pub const BYTES: usize = BYTES;
    /// Logical columns: 320.
    pub const LOGICAL_WIDTH: i32 = 320;
    /// Logical rows: 200.
    pub const LOGICAL_HEIGHT: i32 = 200;

    /// A picture with every pixel at index 0, palette 0 and normal intensity
    /// (research 03 §3, §4.1).
    pub fn new() -> Self {
        Self {
            pixels: zeroed_pixels(),
            palette: Palette::Zero,
            intensity: Intensity::Normal,
        }
    }

    /// Sets every pixel to index 0. The palette and the intensity are kept
    /// (research 03 §4.2).
    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }

    /// The 256000 palette indices, 640 per real row, 400 rows. This is what a
    /// presenter expands through [`colors`](Self::colors).
    pub fn pixels(&self) -> &[u8; BYTES] {
        &self.pixels
    }

    /// The pixels, mutable, for setting up test pictures.
    #[cfg(test)]
    pub(super) fn pixels_mut(&mut self) -> &mut [u8; BYTES] {
        &mut self.pixels
    }

    /// The selected palette.
    pub const fn palette(&self) -> Palette {
        self.palette
    }

    /// The selected intensity.
    pub const fn intensity(&self) -> Intensity {
        self.intensity
    }

    /// Selects `palette` at normal intensity. No pixel changes.
    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
        // Quirk R3: selecting a palette also resets the intensity
        // (research 03 §3, §13 item 10).
        self.intensity = Intensity::Normal;
    }

    /// Selects `intensity`, keeping the selected palette. No pixel changes.
    pub fn set_intensity(&mut self, intensity: Intensity) {
        self.intensity = intensity;
    }

    /// The 16 colours the picture's indices currently stand for.
    pub const fn colors(&self) -> &'static [Rgb; 16] {
        colors(self.palette, self.intensity)
    }

    /// The CRC-32 of the 256000 indices in row-major order; the palette and
    /// intensity are not part of it (research 03 §12.3).
    pub fn crc32(&self) -> u32 {
        crc32(self.pixels.as_slice())
    }

    /// Draws `image` at logical `at`, masked, clipped to the picture; `at.x`
    /// is used as given (research 03 §4.5).
    ///
    /// Each image byte is combined with the mask byte at the same position:
    /// an image byte of 255 leaves the pixel unchanged, any other byte sets
    /// the pixel to `(pixel AND mask) OR image`.
    pub fn blit_masked(&mut self, image: ImageId, at: Px) {
        let image = image.get();
        self.blit_parts(at, image.size().real_width(), image.pixels(), image.mask());
    }

    /// The masked blit of [`blit_masked`](Self::blit_masked) over raw image
    /// and mask bytes, `real_width` bytes per row.
    pub(super) fn blit_parts(&mut self, at: Px, real_width: usize, pixels: &[u8], mask: &[u8]) {
        let real_height = pixels.len().checked_div(real_width).unwrap_or(0);
        for span in clip(at, real_width, real_height) {
            let (Some(dst), Some(src), Some(keep)) = (
                self.pixels.get_mut(span.dst..span.dst + span.len),
                pixels.get(span.src..span.src + span.len),
                mask.get(span.src..span.src + span.len),
            ) else {
                continue;
            };
            for ((pixel, &value), &keep) in dst.iter_mut().zip(src).zip(keep) {
                // Quirk R4: the image byte 255 is a transparency key of its
                // own, in addition to the mask (research 03 §4.5, §13 item 9).
                if value != 0xFF {
                    *pixel = (*pixel & keep) | value;
                }
            }
        }
    }

    /// Writes `source`, `real_width` bytes per row, opaquely at logical `at`,
    /// clipped to the picture.
    pub(crate) fn put_opaque(&mut self, at: Px, real_width: usize, source: &[u8]) {
        let real_height = source.len().checked_div(real_width).unwrap_or(0);
        for span in clip(at, real_width, real_height) {
            if let (Some(dst), Some(src)) = (
                self.pixels.get_mut(span.dst..span.dst + span.len),
                source.get(span.src..span.src + span.len),
            ) {
                dst.copy_from_slice(src);
            }
        }
    }

    /// Copies the `8w x 2h` real pixels of a rectangle of `size` at logical
    /// `at`. Parts outside the picture read as index 0 (research 03 §4.3).
    pub fn capture(&self, at: Px, size: ImageSize) -> SavedRect {
        let mut saved = SavedRect::zeroed(size);
        let target = saved.pixels_mut();
        for span in clip(at, size.real_width(), size.real_height()) {
            if let (Some(dst), Some(src)) = (
                target.get_mut(span.src..span.src + span.len),
                self.pixels.get(span.dst..span.dst + span.len),
            ) {
                dst.copy_from_slice(src);
            }
        }
        saved
    }

    /// Writes every pixel of `saved`, index 0 included, at logical `at`,
    /// using the size `saved` was captured with, clipped to the picture
    /// (research 03 §4.4, §13 item 6).
    pub fn restore(&mut self, at: Px, saved: &SavedRect) {
        self.put_opaque(at, saved.size().real_width(), saved.pixels());
    }

    /// Reads the 8x2 block of real pixels at real (2x, 2y), as the fireball
    /// does to find walls (research 03 §4.6).
    ///
    /// Bit `0x80 >> i` is set when real column `i` of the block is non-zero
    /// in either row, and the result is masked with `0xEE`. Beyond logical
    /// x 319 or y 199 the result is `0xFF`, unmasked. `at.x` is not aligned.
    pub fn probe(&self, at: Px) -> ProbeBits {
        // Quirk R1: an unaligned 8x2 block, both rows ORed, masked with 0xEE;
        // an unmasked 0xFF beyond the right or bottom edge.
        if at.x >= Self::LOGICAL_WIDTH || at.y >= Self::LOGICAL_HEIGHT {
            return ProbeBits::OUTSIDE;
        }
        let row_len = i64::from(Self::LOGICAL_WIDTH) * 2;
        let mut bits = 0u8;
        for row in 0..2 {
            let start = (2 * i64::from(at.y) + row) * row_len + 2 * i64::from(at.x);
            for column in 0..8u8 {
                if self.pixel_at_offset(start + i64::from(column)) != 0 {
                    bits |= 0x80 >> column;
                }
            }
        }
        ProbeBits(bits & ProbeBits::ALL.0)
    }

    /// The pixel at a linear offset into the row-major picture.
    ///
    /// Quirk R2: the probe addresses the picture linearly, so a block that
    /// runs past a row's end continues on the next row; offsets before the
    /// start or past the end of the picture read as 0 (undefined behaviour
    /// upstream, given a defined result here; research 03 §4.6).
    fn pixel_at_offset(&self, offset: i64) -> u8 {
        usize::try_from(offset)
            .ok()
            .and_then(|offset| self.pixels.get(offset))
            .copied()
            .unwrap_or(0)
    }
}

/// A zeroed picture, allocated on the heap directly rather than built on the
/// stack and moved.
fn zeroed_pixels() -> Box<[u8; BYTES]> {
    match vec![0; BYTES].into_boxed_slice().try_into() {
        Ok(pixels) => pixels,
        Err(_) => unreachable!("a vector of BYTES elements converts to [u8; BYTES]"),
    }
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Framebuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Framebuffer")
            .field("palette", &self.palette)
            .field("intensity", &self.intensity)
            .field("crc32", &format_args!("{:#010x}", self.crc32()))
            .finish_non_exhaustive()
    }
}

/// The bits a [`Framebuffer::probe`] returns: bit `0x80 >> i` stands for
/// real column `i` of the probed 8x2 block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ProbeBits(u8);

impl ProbeBits {
    /// No bit set: the block is black.
    pub const NONE: Self = Self(0);
    /// Every bit a probe inside the picture can set: `0xEE`, columns 3 and 7
    /// never contribute.
    pub const ALL: Self = Self(0xEE);
    /// The result of a probe beyond logical x 319 or y 199: `0xFF`.
    pub const OUTSIDE: Self = Self(0xFF);
    /// The bits the fireball tests when moving up: `0xC0`, real columns 0
    /// and 1 (research 03 §4.6).
    pub const UP: Self = Self(0xC0);
    /// The bits the fireball tests when moving down: `0x03`; inside the
    /// picture only `0x02`, real column 6, can be set (research 03 §4.6).
    pub const DOWN: Self = Self(0x03);

    /// The bits as a byte.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// The probe result `bits`.
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// Whether no bit is set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether `self` and `other` have a bit in common.
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

impl BitOr for ProbeBits {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitOrAssign for ProbeBits {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl BitAnd for ProbeBits {
    type Output = Self;

    fn bitand(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

/// One row of a clipped transfer: `len` bytes starting at offset `dst` in
/// the picture and at offset `src` in the source rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    dst: usize,
    src: usize,
    len: usize,
}

/// The rows of a `real_width x real_height` rectangle at logical `at` that
/// lie inside the picture, each cut to its in-picture columns. Empty when the
/// rectangle lies entirely outside.
fn clip(at: Px, real_width: usize, real_height: usize) -> impl Iterator<Item = Span> {
    clip_bounds(at, real_width, real_height)
        .into_iter()
        .flat_map(move |bounds| {
            (bounds.top..bounds.bottom).map(move |row| Span {
                dst: row * WIDTH + bounds.left,
                src: (row - bounds.top + bounds.src_y) * real_width + bounds.src_x,
                len: bounds.right - bounds.left,
            })
        })
}

/// The in-picture part of a rectangle, in real picture coordinates
/// (`left..right`, `top..bottom`), and where it starts in the source.
#[derive(Debug, Clone, Copy)]
struct Bounds {
    left: usize,
    right: usize,
    top: usize,
    bottom: usize,
    /// Source column of picture column `left`.
    src_x: usize,
    /// Source row of picture row `top`.
    src_y: usize,
}

fn clip_bounds(at: Px, real_width: usize, real_height: usize) -> Option<Bounds> {
    let x = 2 * i64::from(at.x);
    let y = 2 * i64::from(at.y);
    let width = i64::try_from(real_width).ok()?;
    let height = i64::try_from(real_height).ok()?;
    let left = x.max(0);
    let right = (x + width).min(i64::try_from(WIDTH).ok()?);
    let top = y.max(0);
    let bottom = (y + height).min(i64::try_from(HEIGHT).ok()?);
    if left >= right || top >= bottom {
        return None;
    }
    Some(Bounds {
        left: usize::try_from(left).ok()?,
        right: usize::try_from(right).ok()?,
        top: usize::try_from(top).ok()?,
        bottom: usize::try_from(bottom).ok()?,
        src_x: usize::try_from(left - x).ok()?,
        src_y: usize::try_from(top - y).ok()?,
    })
}
