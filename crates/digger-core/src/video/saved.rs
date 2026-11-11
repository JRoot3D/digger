//! Saved rectangles: copies of picture areas, restored opaquely.

use crate::assets::ImageSize;

/// A copy of a rectangle of the picture, taken by
/// [`Framebuffer::capture`](super::Framebuffer::capture) and written back by
/// [`Framebuffer::restore`](super::Framebuffer::restore).
///
/// The copy carries its own size, and a restore always writes exactly that
/// many pixels, whatever size a sprite has by then (research 03 §4.4, §13
/// item 6). Storage is inline and sized for the largest image, so capturing
/// never allocates.
#[derive(Clone, PartialEq, Eq)]
pub struct SavedRect {
    size: ImageSize,
    pixels: [u8; ImageSize::MAX_BYTES],
}

impl SavedRect {
    /// A copy of `size` whose pixels are all 0.
    pub(super) const fn zeroed(size: ImageSize) -> Self {
        Self {
            size,
            pixels: [0; ImageSize::MAX_BYTES],
        }
    }

    /// The size the copy was captured with.
    pub const fn size(&self) -> ImageSize {
        self.size
    }

    /// The saved palette indices, `size().real_width()` per row,
    /// `size().real_height()` rows.
    pub fn pixels(&self) -> &[u8] {
        self.pixels
            .get(..self.size.byte_len())
            .unwrap_or(&self.pixels)
    }

    /// The saved pixels, mutable.
    pub(super) fn pixels_mut(&mut self) -> &mut [u8] {
        let len = self.size.byte_len();
        let (pixels, _) = self.pixels.split_at_mut(len.min(ImageSize::MAX_BYTES));
        pixels
    }
}

impl std::fmt::Debug for SavedRect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SavedRect")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}
