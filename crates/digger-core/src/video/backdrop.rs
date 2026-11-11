//! Full-picture drawing: the level background and the title picture.

use super::framebuffer::Framebuffer;
use crate::assets::{ImageId, TitlePicture};
use crate::geom::Px;

/// One of the eight level plans, 1 to 8, each with its own background tile
/// (research 03 §5.2, §8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LevelPlan {
    plan: u8,
    tile: ImageId,
}

impl LevelPlan {
    /// The plan numbered `plan` (1 to 8), or `None` for any other number.
    pub const fn new(plan: u8) -> Option<Self> {
        match ImageId::background(plan) {
            Some(tile) => Some(Self { plan, tile }),
            None => None,
        }
    }

    /// The plan's number, 1 to 8.
    pub const fn number(self) -> u8 {
        self.plan
    }

    /// The plan's background tile: image number `93 + plan`, a 20x4 logical
    /// pixel opaque tile.
    pub const fn tile(self) -> ImageId {
        self.tile
    }

    /// All eight plans in order.
    pub fn all() -> impl Iterator<Item = Self> {
        (1..=8).filter_map(Self::new)
    }
}

/// First logical row of the background; rows 0–13 belong to the status line.
const BACKGROUND_TOP: i32 = 14;
/// Logical width of a background tile.
const TILE_WIDTH: i32 = 20;
/// Logical height of a background tile.
const TILE_HEIGHT: i32 = 4;

impl Framebuffer {
    /// Tiles the picture below the status line with the background of
    /// `plan` (research 03 §8.2).
    ///
    /// The tile is drawn at x = 0, 20, … 300 and y = 14, 18, … 198: 16
    /// columns by 47 rows, 752 tiles. The last row is clipped to logical rows
    /// 198–199, and logical rows 0–13 are not touched.
    pub fn draw_background(&mut self, plan: LevelPlan) {
        // Quirk R7: tiling starts at y 14, not at a tile boundary of the
        // status line, and the bottom row of tiles hangs over the picture's
        // edge by two logical rows.
        let tile = plan.tile();
        let mut y = BACKGROUND_TOP;
        while y < Self::LOGICAL_HEIGHT {
            let mut x = 0;
            while x < Self::LOGICAL_WIDTH {
                self.blit_masked(tile, Px::new(x, y));
                x += TILE_WIDTH;
            }
            y += TILE_HEIGHT;
        }
    }

    /// Replaces every pixel with the title picture, whatever was there
    /// before. The palette and the intensity are kept (research 03 §4.8,
    /// §9.1).
    pub fn draw_title(&mut self, title: &TitlePicture) {
        self.put_opaque(Px::new(0, 0), Self::WIDTH, title.pixels());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::{Intensity, Palette, TextColor};

    const W: usize = Framebuffer::WIDTH;

    fn plan(n: u8) -> LevelPlan {
        LevelPlan::new(n).unwrap_or_else(|| unreachable!("plan {n} is valid"))
    }

    fn title() -> TitlePicture {
        TitlePicture::decode().unwrap_or_else(|error| unreachable!("title decodes: {error}"))
    }

    fn rows(fb: &Framebuffer) -> &[[u8; W]] {
        fb.pixels().as_chunks::<W>().0
    }

    #[test]
    fn level_plan_rejects_0_and_9() {
        assert_eq!(LevelPlan::new(0), None);
        assert_eq!(LevelPlan::new(9), None);
        assert_eq!(LevelPlan::new(255), None);
        for n in 1..=8 {
            let p = plan(n);
            assert_eq!(p.number(), n);
            assert_eq!(p.tile().number(), 93 + n);
            assert_eq!(p.tile().size().w(), 5);
            assert_eq!(p.tile().size().h(), 4);
        }
        assert_eq!(LevelPlan::all().count(), 8);
    }

    #[test]
    fn background_coverage() {
        for p in LevelPlan::all() {
            let mut fb = Framebuffer::new();
            fb.draw_background(p);
            let rows = rows(&fb);
            assert!(
                rows.iter().take(28).flatten().all(|&b| b == 0),
                "plan {}: rows 0–13 untouched",
                p.number()
            );
            assert!(
                rows.iter().skip(28).flatten().all(|&b| b != 0),
                "plan {}: rows 14–199 covered",
                p.number()
            );
        }
    }

    #[test]
    fn background_plan_selects_tile() {
        let mut fb = Framebuffer::new();
        fb.draw_background(plan(4));
        let tile = plan(4).tile().get();
        let stride = tile.size().real_width();
        assert_eq!(stride, 40);
        for (r, line) in tile.pixels().chunks(stride).enumerate() {
            let row = rows(&fb).get(28 + r).map(|row| row.get(..40));
            assert_eq!(row, Some(Some(line)), "tile row {r}");
        }
        // Every tile, the clipped bottom row included, repeats the image.
        for y in (14..200).step_by(4) {
            for x in (0..320).step_by(20) {
                for (r, line) in tile.pixels().chunks(stride).enumerate() {
                    let real_row = 2 * y + r;
                    if real_row >= 400 {
                        continue;
                    }
                    let got = rows(&fb)
                        .get(real_row)
                        .and_then(|row| row.get(2 * x..2 * x + 40));
                    assert_eq!(got, Some(line), "tile at ({x}, {y}) row {r}");
                }
            }
        }
        let mut seen = [false; 256];
        for &b in fb.pixels() {
            if let Some(slot) = seen.get_mut(usize::from(b)) {
                *slot = true;
            }
        }
        let values: Vec<usize> = (0..256).filter(|&v| seen.get(v) == Some(&true)).collect();
        assert_eq!(values, [0, 2, 6]);
    }

    #[test]
    fn background_preserves_top_line() {
        let mut fb = Framebuffer::new();
        fb.draw_text("SCORE 12345", Px::new(0, 0), TextColor::Three);
        let before: Vec<[u8; W]> = rows(&fb).iter().take(28).copied().collect();
        fb.draw_background(plan(1));
        let after: Vec<[u8; W]> = rows(&fb).iter().take(28).copied().collect();
        assert_eq!(before, after);
        assert!(after.iter().flatten().any(|&b| b != 0));
    }

    #[test]
    fn title_replaces_whole_picture() {
        let title = title();
        let mut fb = Framebuffer::new();
        fb.draw_background(plan(2));
        fb.draw_text("FULL OF TEXT", Px::new(20, 100), TextColor::Two);
        fb.blit_masked(ImageId::BAG_STILL, Px::new(60, 60));
        fb.blit_masked(ImageId::NOBBIN_1, Px::new(-4, 190));
        fb.draw_title(&title);
        assert_eq!(fb.pixels(), title.pixels());
    }

    #[test]
    fn title_keeps_palette() {
        let mut fb = Framebuffer::new();
        fb.set_palette(Palette::One);
        fb.set_intensity(Intensity::High);
        fb.draw_title(&title());
        assert_eq!(fb.palette(), Palette::One);
        assert_eq!(fb.intensity(), Intensity::High);
    }

    #[test]
    fn title_text_fits_in_black_margin() {
        let title = title();
        let mut fb = Framebuffer::new();
        fb.draw_title(&title);
        fb.draw_text("D I G G E R", Px::new(100, 0), TextColor::Three);
        for (i, (&now, &was)) in fb.pixels().iter().zip(title.pixels()).enumerate() {
            let (column, row) = (i % W, i / W);
            if now != was {
                assert!(row < 35, "({column}, {row}) changed below the black rows");
                assert_eq!(was, 0, "({column}, {row}) was black");
            }
        }
        // Rows 35 and below, the frame included, are untouched.
        let below = 35 * W;
        assert_eq!(fb.pixels().get(below..), title.pixels().get(below..));
        assert_ne!(fb.pixels(), title.pixels());
    }
}
