//! Logical coordinates.
//!
//! Game logic addresses the picture as 320x200 logical pixels; every logical
//! pixel covers 2x2 real pixels of the 640x400 picture (research 03 §2). A
//! [`Px`] is a logical point and a [`Rect`] a logical rectangle. Coordinates
//! are `i32` so that positions left of or above the picture are expressible;
//! every drawing operation clips them.

use crate::assets::ImageSize;

/// A point in logical coordinates: `x` grows to the right, `y` downwards,
/// (0, 0) is the top-left logical pixel of the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Px {
    /// Logical column.
    pub x: i32,
    /// Logical row.
    pub y: i32,
}

impl Px {
    /// The point (`x`, `y`).
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// The same point with `x` rounded down to a multiple of 4
    /// (see [`align_x`]); `y` is unchanged.
    #[must_use]
    pub const fn aligned(self) -> Self {
        Self {
            x: align_x(self.x),
            y: self.y,
        }
    }
}

/// Rounds `x` down to a multiple of 4, towards negative infinity for
/// negative values: 115 becomes 112 and -3 becomes -4.
///
/// Quirk S1: the sprite layer aligns x this way for place, move and static
/// drawing, but text, the pixel probe and the protected area of a static
/// draw use x as given (research 03 §2, §13 item 8).
pub const fn align_x(x: i32) -> i32 {
    // Clearing the two low bits of a two's-complement number rounds towards
    // negative infinity.
    x & !3
}

/// A rectangle in logical pixels: its top-left corner and its width and
/// height, both at least 1 for a non-empty rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    origin: Px,
    width: i32,
    height: i32,
}

impl Rect {
    /// The rectangle with top-left corner `origin`, `width` logical columns
    /// and `height` logical rows.
    pub const fn new(origin: Px, width: i32, height: i32) -> Self {
        Self {
            origin,
            width,
            height,
        }
    }

    /// The rectangle an image of `size` covers when drawn at `origin`:
    /// `4w` logical columns by `h` logical rows.
    pub const fn of_image(origin: Px, size: ImageSize) -> Self {
        // An image is at most 6 units (24 logical pixels) wide and 18 rows
        // high, so both values fit an i32 exactly.
        Self::new(origin, 4 * size.w() as i32, size.h() as i32)
    }

    /// The top-left corner.
    pub const fn origin(self) -> Px {
        self.origin
    }

    /// Width in logical pixels.
    pub const fn width(self) -> i32 {
        self.width
    }

    /// Height in logical pixels.
    pub const fn height(self) -> i32 {
        self.height
    }

    /// The leftmost logical column.
    pub const fn left(self) -> i32 {
        self.origin.x
    }

    /// The topmost logical row.
    pub const fn top(self) -> i32 {
        self.origin.y
    }

    /// The rightmost logical column, inclusive: `left + width - 1`.
    pub const fn right(self) -> i32 {
        self.origin.x + self.width - 1
    }

    /// The bottom logical row, inclusive: `top + height - 1`.
    pub const fn bottom(self) -> i32 {
        self.origin.y + self.height - 1
    }

    /// Whether the two rectangles share at least one logical pixel: each
    /// left edge lies at or left of the other's right edge and each top edge
    /// at or above the other's bottom edge (research 03 §7.3).
    pub const fn overlaps(self, other: Self) -> bool {
        self.left() <= other.right()
            && other.left() <= self.right()
            && self.top() <= other.bottom()
            && other.top() <= self.bottom()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::ImageId;

    #[test]
    fn align_x_rounds_toward_negative_infinity() {
        let cases = [
            (0, 0),
            (1, 0),
            (3, 0),
            (4, 4),
            (115, 112),
            (292, 292),
            (319, 316),
            (-1, -4),
            (-3, -4),
            (-4, -4),
            (-5, -8),
            (i32::MIN, i32::MIN),
            (i32::MAX, i32::MAX - 3),
        ];
        for (x, aligned) in cases {
            assert_eq!(align_x(x), aligned, "align_x({x})");
            assert_eq!(align_x(x) % 4, 0, "align_x({x}) is a multiple of 4");
        }
        assert_eq!(Px::new(115, 37).aligned(), Px::new(112, 37));
        assert_eq!(Px::new(-3, -7).aligned(), Px::new(-4, -7));
    }

    #[test]
    fn rect_edges_are_inclusive() {
        let rect = Rect::of_image(Px::new(12, 18), ImageId::BAG_STILL.size());
        assert_eq!((rect.width(), rect.height()), (16, 15));
        assert_eq!((rect.left(), rect.right()), (12, 27));
        assert_eq!((rect.top(), rect.bottom()), (18, 32));
        assert_eq!(rect.origin(), Px::new(12, 18));
    }

    #[test]
    fn rect_overlap_shares_a_pixel() {
        let a = Rect::new(Px::new(100, 50), 16, 15);
        assert!(a.overlaps(a));
        assert!(a.overlaps(Rect::new(Px::new(115, 64), 1, 1)));
        assert!(!a.overlaps(Rect::new(Px::new(116, 50), 16, 15)));
        assert!(!a.overlaps(Rect::new(Px::new(100, 65), 16, 15)));
        assert!(!a.overlaps(Rect::new(Px::new(84, 50), 16, 15)));
        assert!(a.overlaps(Rect::new(Px::new(85, 36), 16, 15)));
    }
}
