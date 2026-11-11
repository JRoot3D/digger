//! Drawing the playfield: background, tunnel openings, carving shapes,
//! emeralds and the progressive level draw (design D1).
//!
//! The picture is game state, not only output: the fireball finds earth by
//! reading pixels back, and sprites collide with what was drawn. Everything
//! here therefore draws exactly whether or not anyone looks at the picture.
//!
//! # Drawing order of a full draw
//!
//! A full draw happens for a new level and whenever the turn passes to the
//! other player (research 03 §8.2, research 01 §3.2). Its order is fixed,
//! because later draws protect and re-save the sprites the earlier ones put
//! on the picture:
//!
//! 1. The picture is cleared to black, status line included.
//! 2. The caller resets every sprite to its initial image and disables it
//!    (this changes no pixel, quirk S6, so it may equally come first).
//! 3. The field in play is normalised for the redraw (quirk Q05): every
//!    cell that is not untouched loses all its vertical slices if any is
//!    missing and all its horizontal slices if any is missing. This is a
//!    state change and happens once, when the draw starts.
//! 4. The palette is reset to palette 0 at normal intensity.
//! 5. The background tile of the level's layout number is tiled over rows
//!    14..=199 (rows 0..=13 stay black for the status line).
//! 6. Every opened cell (one that is not untouched) is drawn, column by
//!    column from the left and top to bottom within a column: the vertical
//!    opening, then the horizontal opening, then the join to the right
//!    neighbour, then the join to the lower neighbour.
//! 7. The bags are drawn (by the caller, through the hook of
//!    [`LevelDraw::frame`]).
//! 8. Every emerald of the current player is drawn, column by column.
//!
//! Steps 1–5 take no frame. Steps 6–8 are revealed progressively, three
//! items per frame, where every opened cell and every emerald is one item
//! and the bags are not counted: level 1 (33 opened cells, 30 emeralds)
//! takes 21 frames. A draw with no item at all still takes one frame. No
//! game tick elapses during the draw, and nothing it reads changes, so the
//! state afterwards is the same as if it had been drawn at once.
//!
//! The scores, lives and top-line text are drawn afterwards by the round
//! start (status line, design D3).

use crate::assets::ImageId;
use crate::geom::{Px, Rect};
use crate::video::{Framebuffer, Intensity, LevelPlan, Palette};

use super::field::{EmeraldMap, Field};
use super::geom::{COLUMNS, CellPos, Direction, ROWS};
use super::gfx::Gfx;
use super::ids::PlayerId;

/// Items revealed per frame by the progressive level draw (design D3).
pub(crate) const DRAW_ITEMS_PER_FRAME: u8 = 3;

/// Vertical offset of an emerald below its cell's top-left corner
/// (research 01 §5.1, research 03 §8.4).
const EMERALD_DY: i32 = 3;

/// Number of cells in the grid, as a column-major index bound.
const CELLS: u8 = COLUMNS * ROWS;

/// One of the six black shapes carved into the earth (research 03 §8.3).
///
/// Each is drawn at an offset from the carving entity's position and is a
/// mask-only image: where its mask is clear the picture turns black, and
/// elsewhere it is left alone. The shapes have rounded or toothed edges, so
/// a shape does not blacken its whole rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Carve {
    /// Right cap, 8×18 at (x+16, y−1): carved when moving right.
    Right,
    /// Left cap, 8×18 at (x−8, y−1): carved when moving left.
    Left,
    /// Top cap, 24×6 at (x−4, y−6): carved when moving up.
    Top,
    /// Bottom cap, 24×6 at (x−4, y+15): carved when moving down.
    Bottom,
    /// Toothed 24×6 shape at (x−4, y+17): a bag's first fall step.
    Square,
    /// Toothed 24×8 shape at (x−4, y+15): a bag's later fall steps.
    Furry,
}

#[allow(
    dead_code,
    reason = "the movers of later task groups carve through these"
)]
impl Carve {
    /// All six shapes.
    pub(crate) const ALL: [Self; 6] = [
        Self::Right,
        Self::Left,
        Self::Top,
        Self::Bottom,
        Self::Square,
        Self::Furry,
    ];

    /// The shape a mover carves before a step in `direction`.
    pub(crate) const fn for_direction(direction: Direction) -> Self {
        match direction {
            Direction::Right => Self::Right,
            Direction::Up => Self::Top,
            Direction::Left => Self::Left,
            Direction::Down => Self::Bottom,
        }
    }

    /// The shape's image: a blank image with the shape's mask.
    pub(crate) const fn image(self) -> ImageId {
        match self {
            Self::Right => ImageId::TUNNEL_RIGHT,
            Self::Left => ImageId::TUNNEL_LEFT,
            Self::Top => ImageId::TUNNEL_UP,
            Self::Bottom => ImageId::TUNNEL_DOWN,
            Self::Square => ImageId::TUNNEL_SQUARE,
            Self::Furry => ImageId::TUNNEL_FURRY,
        }
    }

    /// The shape's top-left corner relative to the carving entity's
    /// position (research 03 §8.3).
    pub(crate) const fn offset(self) -> (i32, i32) {
        match self {
            Self::Right => (16, -1),
            Self::Left => (-8, -1),
            Self::Top => (-4, -6),
            Self::Bottom | Self::Furry => (-4, 15),
            Self::Square => (-4, 17),
        }
    }

    /// The top-left corner of the shape carved by an entity at `at`.
    pub(crate) const fn origin(self, at: Px) -> Px {
        let (dx, dy) = self.offset();
        Px::new(at.x + dx, at.y + dy)
    }

    /// The rectangle the shape carved by an entity at `at` may blacken,
    /// before clipping to the picture.
    pub(crate) const fn rect(self, at: Px) -> Rect {
        Rect::of_image(self.origin(at), self.image().size())
    }
}

/// Carves `shape` for an entity at `at`: the shape is drawn into the
/// background beneath every sprite, clipped at the picture's edges
/// (research 03 §7.5, §8.3).
#[allow(dead_code, reason = "called by the movers of later task groups")]
pub(crate) fn carve(gfx: &mut Gfx, shape: Carve, at: Px) {
    gfx.draw_static(shape.image(), shape.origin(at));
}

/// Resets the palette to palette 0 at normal intensity and tiles rows
/// 14..=199 with the background tile of `plan` (playfield spec,
/// "Background drawing"; research 03 §8.2).
pub(crate) fn draw_background(fb: &mut Framebuffer, plan: LevelPlan) {
    // Selecting the palette resets the intensity too (quirk R3); the second
    // call states the intent and keeps it true should that ever change.
    fb.set_palette(Palette::Zero);
    fb.set_intensity(Intensity::Normal);
    fb.draw_background(plan);
}

/// Draws `cell` of `field` open, as a full draw does: the vertical opening
/// if any vertical slice is missing, the horizontal opening if any
/// horizontal slice is missing, then a join toward the right and toward the
/// lower neighbour when that neighbour has a tunnel in it (playfield spec,
/// "Tunnel drawing on a full draw"; research 03 §8.2).
fn draw_opened_cell(gfx: &mut Gfx, field: &Field, cell: CellPos) {
    let earth = field.cell(cell);
    let Px { x, y } = cell.to_px();
    if !earth.has_all_v_slices() {
        // Five bottom caps from the cell's top edge down and a top cap at
        // its bottom: the full 18-pixel height with rounded ends.
        for dy in [-15, -12, -9, -6, -3] {
            carve(gfx, Carve::Bottom, Px::new(x, y + dy));
        }
        carve(gfx, Carve::Top, Px::new(x, y + 3));
    }
    if !earth.has_all_h_slices() {
        // Four right caps across the cell and a left cap at its left edge:
        // the full 20-pixel width with rounded ends.
        for dx in [-16, -12, -8, -4] {
            carve(gfx, Carve::Right, Px::new(x + dx, y));
        }
        carve(gfx, Carve::Left, Px::new(x + 4, y));
    }
    let has_tunnel = |direction| {
        cell.neighbour(direction)
            .is_some_and(|next| field.cell(next).has_tunnel())
    };
    if has_tunnel(Direction::Right) {
        carve(gfx, Carve::Right, Px::new(x, y));
    }
    if has_tunnel(Direction::Down) {
        carve(gfx, Carve::Bottom, Px::new(x, y));
    }
}

/// The logical pixel of the emerald of `cell`: (h×20+12, v×18+21)
/// (playfield spec, "Emerald drawing").
pub(crate) const fn emerald_px(cell: CellPos) -> Px {
    let corner = cell.to_px();
    Px::new(corner.x, corner.y + EMERALD_DY)
}

/// Draws the 16×10 emerald of `cell` into the background beneath every
/// sprite (research 03 §8.4).
pub(crate) fn draw_emerald(gfx: &mut Gfx, cell: CellPos) {
    gfx.draw_static(ImageId::EMERALD, emerald_px(cell));
}

/// Erases the emerald of `cell` by painting its 16×10 silhouette black
/// (research 03 §8.4).
#[allow(
    dead_code,
    reason = "called by diggers, hobbins and bags (later task groups)"
)]
pub(crate) fn erase_emerald(gfx: &mut Gfx, cell: CellPos) {
    gfx.draw_static(ImageId::EMERALD_ERASER, emerald_px(cell));
}

/// The cell with column-major index `i`: (i div 10, i mod 10).
const fn column_major(i: u8) -> Option<CellPos> {
    CellPos::new(i / ROWS, i % ROWS)
}

/// Whether the full draw reveals a frame or has finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DrawProgress {
    /// More items remain; the next frame continues the draw.
    InProgress,
    /// Every item has been drawn; the next frame belongs to the next phase.
    Done,
}

/// Where the progressive draw stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Stage {
    /// Opened cells from column-major index `next` on.
    Cells { next: u8 },
    /// The current player's emeralds from column-major index `next` on.
    Emeralds { next: u8 },
    /// Everything has been drawn.
    Done,
}

/// The progressive full draw of the playfield: the cursor of the level-draw
/// phase (design D3, D9). It holds only where it stands; the field and
/// emeralds it reveals are passed in every frame and do not change during
/// the draw, since no tick runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LevelDraw {
    stage: Stage,
}

#[allow(dead_code, reason = "driven by the level-draw phase (task group 7)")]
impl LevelDraw {
    /// Starts a full draw: clears the picture, normalises `field` for the
    /// redraw (quirk Q05), resets the palette and draws the background of
    /// `plan`. The caller resets the sprites (see the module docs) and then
    /// calls [`frame`](Self::frame) once per frame until it reports
    /// [`DrawProgress::Done`].
    pub(crate) fn begin(gfx: &mut Gfx, field: &mut Field, plan: LevelPlan) -> Self {
        gfx.fb_mut().clear();
        field.normalise_for_redraw();
        draw_background(gfx.fb_mut(), plan);
        Self {
            stage: Stage::Cells { next: 0 },
        }
    }

    /// The number of items the draw of `field` and `player`'s emeralds
    /// reveals: opened cells plus emeralds. It takes ⌈items / 3⌉ frames, at
    /// least one.
    pub(crate) fn items(field: &Field, emeralds: &EmeraldMap, player: PlayerId) -> u16 {
        let opened = CellPos::all_column_major()
            .filter(|&cell| !field.cell(cell).is_untouched())
            .count();
        let emeralds = CellPos::all_column_major()
            .filter(|&cell| emeralds.has(player, cell))
            .count();
        u16::try_from(opened + emeralds).unwrap_or(u16::MAX)
    }

    /// Whether every item has been drawn.
    pub(crate) const fn is_done(self) -> bool {
        matches!(self.stage, Stage::Done)
    }

    /// Draws the next three items of the full draw of `field` with
    /// `player`'s emeralds. When the opened cells are exhausted, `bags`
    /// draws the bags before the first emerald; it is called in exactly one
    /// frame of the draw. Returns [`DrawProgress::Done`] on the frame that
    /// draws the last item.
    pub(crate) fn frame(
        &mut self,
        gfx: &mut Gfx,
        field: &Field,
        emeralds: &EmeraldMap,
        player: PlayerId,
        bags: impl FnOnce(&mut Gfx),
    ) -> DrawProgress {
        let mut bags = Some(bags);
        self.settle(gfx, field, emeralds, player, &mut bags);
        for _ in 0..DRAW_ITEMS_PER_FRAME {
            match self.stage {
                Stage::Cells { next } => {
                    if let Some(cell) = column_major(next) {
                        draw_opened_cell(gfx, field, cell);
                    }
                    self.stage = Stage::Cells { next: next + 1 };
                }
                Stage::Emeralds { next } => {
                    if let Some(cell) = column_major(next) {
                        draw_emerald(gfx, cell);
                    }
                    self.stage = Stage::Emeralds { next: next + 1 };
                }
                Stage::Done => break,
            }
            self.settle(gfx, field, emeralds, player, &mut bags);
        }
        if self.is_done() {
            DrawProgress::Done
        } else {
            DrawProgress::InProgress
        }
    }

    /// Moves the cursor onto the next item to draw, passing the bags and
    /// finishing the draw on the way when a stage has no item left.
    fn settle(
        &mut self,
        gfx: &mut Gfx,
        field: &Field,
        emeralds: &EmeraldMap,
        player: PlayerId,
        bags: &mut Option<impl FnOnce(&mut Gfx)>,
    ) {
        loop {
            match self.stage {
                Stage::Cells { next } => {
                    let found = (next..CELLS).find(|&i| {
                        column_major(i).is_some_and(|cell| !field.cell(cell).is_untouched())
                    });
                    if let Some(next) = found {
                        self.stage = Stage::Cells { next };
                        return;
                    }
                    if let Some(draw_bags) = bags.take() {
                        draw_bags(gfx);
                    }
                    self.stage = Stage::Emeralds { next: 0 };
                }
                Stage::Emeralds { next } => {
                    let found = (next..CELLS)
                        .find(|&i| column_major(i).is_some_and(|cell| emeralds.has(player, cell)));
                    self.stage = match found {
                        Some(next) => Stage::Emeralds { next },
                        None => Stage::Done,
                    };
                    return;
                }
                Stage::Done => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::level::{LevelLayout, LevelNumber, LevelSet};

    const W: usize = Framebuffer::WIDTH;

    fn cell(h: u8, v: u8) -> CellPos {
        CellPos::new(h, v).unwrap_or_else(|| unreachable!("test cell inside the grid"))
    }

    fn plan(n: u8) -> LevelPlan {
        LevelPlan::new(n).unwrap_or_else(|| unreachable!("plan {n} is valid"))
    }

    fn player(n: u8) -> PlayerId {
        PlayerId::new(n).unwrap_or_else(|| unreachable!("player {n} exists"))
    }

    /// The palette index of logical pixel (`x`, `y`): its top-left real
    /// pixel.
    fn px(fb: &Framebuffer, x: i32, y: i32) -> u8 {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            unreachable!("test pixel inside the picture")
        };
        fb.pixels()
            .get(2 * y * W + 2 * x)
            .copied()
            .unwrap_or_else(|| unreachable!("test pixel inside the picture"))
    }

    /// Whether all four real pixels of logical (`x`, `y`) are black.
    fn is_black(fb: &Framebuffer, x: i32, y: i32) -> bool {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return false;
        };
        [(0, 0), (1, 0), (0, 1), (1, 1)]
            .iter()
            .all(|&(dx, dy)| fb.pixels().get((2 * y + dy) * W + 2 * x + dx).copied() == Some(0))
    }

    /// Whether any of the four real pixels of logical (`x`, `y`) is black.
    fn any_black(fb: &Framebuffer, x: i32, y: i32) -> bool {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return false;
        };
        [(0, 0), (1, 0), (0, 1), (1, 1)]
            .iter()
            .any(|&(dx, dy)| fb.pixels().get((2 * y + dy) * W + 2 * x + dx).copied() == Some(0))
    }

    /// The logical pixels whose real pixels differ between two pictures.
    fn changed(before: &Framebuffer, after: &Framebuffer) -> Vec<(i32, i32)> {
        let mut out: Vec<(i32, i32)> = before
            .pixels()
            .iter()
            .zip(after.pixels())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .filter_map(|(i, _)| {
                let (x, y) = (i32::try_from(i % W).ok()?, i32::try_from(i / W).ok()?);
                Some((x / 2, y / 2))
            })
            .collect();
        out.dedup();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The tile pixel a background of `plan` shows at logical (`x`, `y`),
    /// `y` ≥ 14.
    fn tile_px(plan: LevelPlan, x: i32, y: i32) -> u8 {
        let tile = plan.tile().get();
        let stride = tile.size().real_width();
        let (Ok(tx), Ok(ty)) = (usize::try_from(x % 20), usize::try_from((y - 14) % 4)) else {
            unreachable!()
        };
        tile.pixels()
            .get(2 * ty * stride + 2 * tx)
            .copied()
            .unwrap_or_else(|| unreachable!())
    }

    /// A layout of solid earth with the given characters at the given cells.
    fn layout_with(marks: &[(u8, u8, u8)]) -> LevelLayout {
        let mut rows = [[b' '; 15]; 10];
        for &(h, v, c) in marks {
            if let Some(slot) = rows
                .get_mut(usize::from(v))
                .and_then(|row| row.get_mut(usize::from(h)))
            {
                *slot = c;
            }
        }
        LevelLayout::from_rows(rows)
    }

    /// Draws `field` in full with `player`'s emeralds; returns the number
    /// of frames the draw took and how often the bag hook ran.
    fn full_draw(
        gfx: &mut Gfx,
        field: &mut Field,
        emeralds: &EmeraldMap,
        player: PlayerId,
        plan: LevelPlan,
    ) -> (u32, u32) {
        let mut draw = LevelDraw::begin(gfx, field, plan);
        let (mut frames, mut bag_calls) = (0, 0);
        loop {
            frames += 1;
            let progress = draw.frame(gfx, field, emeralds, player, |_| bag_calls += 1);
            if progress == DrawProgress::Done {
                break;
            }
            assert!(frames < 1000, "the draw finishes");
        }
        (frames, bag_calls)
    }

    #[test]
    fn background_uses_layout_tile() {
        let level_9 = LevelNumber::new(9).unwrap_or_else(|| unreachable!());
        assert_eq!(level_9.plan(), plan(6));
        let mut gfx = Gfx::new();
        gfx.fb_mut().set_intensity(Intensity::High);
        gfx.fb_mut().set_palette(Palette::One);
        gfx.fb_mut().set_intensity(Intensity::High);
        draw_background(gfx.fb_mut(), level_9.plan());
        assert_eq!(gfx.fb().palette(), Palette::Zero);
        assert_eq!(gfx.fb().intensity(), Intensity::Normal);
        let mut expected = Framebuffer::new();
        expected.draw_background(plan(6));
        assert_eq!(gfx.fb().pixels(), expected.pixels());
        let mut other = Framebuffer::new();
        other.draw_background(plan(1));
        assert_ne!(gfx.fb().pixels(), other.pixels());
        // Sample the tile pattern directly.
        for (x, y) in [(0, 14), (7, 15), (19, 17), (150, 100), (319, 199)] {
            assert_eq!(px(gfx.fb(), x, y), tile_px(plan(6), x, y), "({x}, {y})");
        }
    }

    #[test]
    fn background_covers_rows_14_to_199() {
        let mut gfx = Gfx::new();
        gfx.fb_mut()
            .draw_text("12345", Px::new(0, 0), crate::video::TextColor::Three);
        let mut field = Field::EARTH;
        let (frames, bags) =
            full_draw(&mut gfx, &mut field, &EmeraldMap::EMPTY, player(0), plan(3));
        assert_eq!((frames, bags), (1, 1), "an empty draw takes one frame");
        for y in 0..200 {
            for x in 0..320 {
                let want = if y < 14 { 0 } else { tile_px(plan(3), x, y) };
                assert_eq!(px(gfx.fb(), x, y), want, "({x}, {y})");
            }
        }
    }

    #[test]
    fn right_cap_at_x_plus_16_y_minus_1() {
        let at = Px::new(12, 18);
        assert_eq!(Carve::Right.rect(at), Rect::new(Px::new(28, 17), 8, 18));
        let mut gfx = Gfx::new();
        draw_background(gfx.fb_mut(), plan(1));
        let before = gfx.fb().clone();
        carve(&mut gfx, Carve::Right, at);
        let touched = changed(&before, gfx.fb());
        assert!(!touched.is_empty(), "something is drawn");
        for &(x, y) in &touched {
            assert!((28..36).contains(&x) && (17..35).contains(&y), "({x}, {y})");
            assert!(is_black(gfx.fb(), x, y), "({x}, {y}) is black");
        }
        // The cap reaches its rectangle's left edge and its top and bottom
        // rows.
        assert_eq!(touched.iter().map(|p| p.0).min(), Some(28));
        assert_eq!(touched.iter().map(|p| p.1).min(), Some(17));
        assert_eq!(touched.iter().map(|p| p.1).max(), Some(34));
    }

    #[test]
    fn shape_rect_table() {
        let at = Px::new(100, 100);
        let table = [
            (Carve::Right, 116, 99, 8, 18),
            (Carve::Left, 92, 99, 8, 18),
            (Carve::Top, 96, 94, 24, 6),
            (Carve::Bottom, 96, 115, 24, 6),
            (Carve::Square, 96, 117, 24, 6),
            (Carve::Furry, 96, 115, 24, 8),
        ];
        assert_eq!(table.map(|row| row.0), Carve::ALL);
        for (shape, x, y, w, h) in table {
            let rect = Rect::new(Px::new(x, y), w, h);
            assert_eq!(shape.rect(at), rect, "{shape:?}");
            let mut gfx = Gfx::new();
            draw_background(gfx.fb_mut(), plan(2));
            let before = gfx.fb().clone();
            carve(&mut gfx, shape, at);
            let touched = changed(&before, gfx.fb());
            assert!(!touched.is_empty(), "{shape:?} carves something");
            for &(px, py) in &touched {
                assert!(
                    (x..x + w).contains(&px) && (y..y + h).contains(&py),
                    "{shape:?}: ({px}, {py}) inside {rect:?}"
                );
                assert!(is_black(gfx.fb(), px, py), "{shape:?}: black");
            }
        }
        assert_eq!(Carve::for_direction(Direction::Up), Carve::Top);
        assert_eq!(Carve::for_direction(Direction::Down), Carve::Bottom);
        assert_eq!(Carve::for_direction(Direction::Left), Carve::Left);
        assert_eq!(Carve::for_direction(Direction::Right), Carve::Right);
    }

    #[test]
    fn shapes_clip_at_screen_bottom() {
        // An entity on the bottom row, y = 180: the bottom cap's rectangle
        // is rows 195..=200 and the furry shape's 195..=202.
        let at = Px::new(292, 180);
        for shape in [Carve::Bottom, Carve::Furry, Carve::Square] {
            assert!(shape.rect(at).bottom() > 199, "{shape:?} hangs over");
            let mut gfx = Gfx::new();
            draw_background(gfx.fb_mut(), plan(5));
            let before = gfx.fb().clone();
            carve(&mut gfx, shape, at);
            let touched = changed(&before, gfx.fb());
            assert!(!touched.is_empty(), "{shape:?}: the visible part is drawn");
            assert!(touched.iter().all(|&(_, y)| y <= 199));
            assert!(
                touched
                    .iter()
                    .all(|&(x, y)| shape.rect(at).overlaps(Rect::new(Px::new(x, y), 1, 1)))
            );
        }
        // Off the right edge as well, with no failure.
        let mut gfx = Gfx::new();
        carve(&mut gfx, Carve::Right, Px::new(310, 190));
        carve(&mut gfx, Carve::Left, Px::new(-20, -20));
    }

    #[test]
    fn horizontal_corridor_is_continuous_black() {
        // Row 2, columns 3..=7 are `H`.
        let marks: Vec<(u8, u8, u8)> = (3..=7).map(|h| (h, 2, b'H')).collect();
        let mut field = Field::from_layout(&layout_with(&marks));
        let mut gfx = Gfx::new();
        full_draw(&mut gfx, &mut field, &EmeraldMap::EMPTY, player(0), plan(1));
        let (left, top) = (cell(3, 2).to_px().x, cell(3, 2).to_px().y);
        let right = cell(7, 2).to_px().x + 16;
        // The corridor's core rows, y to y + 14, are solid black from the
        // first cell's left edge to the last cell's sprite edge, with no
        // background between cells.
        for y in top..=top + 14 {
            for x in left..right {
                assert!(is_black(gfx.fb(), x, y), "({x}, {y}) is black");
            }
        }
        // Its toothed top and bottom edges, y − 1, y + 15 and y + 16, make
        // the corridor 18 pixels high: every 4-pixel group there has black.
        for y in [top - 1, top + 15, top + 16] {
            for x in (left..right).step_by(4) {
                assert!(
                    (x..x + 4).any(|x| any_black(gfx.fb(), x, y)),
                    "edge ({x}, {y})"
                );
            }
        }
        // Earth above and below stays background.
        for x in left..right {
            assert_ne!(px(gfx.fb(), x, top - 3), 0, "({x}) above");
            assert_ne!(px(gfx.fb(), x, top + 19), 0, "({x}) below");
        }
    }

    #[test]
    fn no_opening_into_solid_neighbour() {
        // An `H` at (4,5) with solid earth to its right and below, and a
        // second `H` at (9,5) whose right neighbour has a partly dug tunnel.
        let mut field = Field::from_layout(&layout_with(&[(4, 5, b'H'), (9, 5, b'H')]));
        let mut penalty = crate::sim::penalty::Penalty::default();
        // Dig one step right from (10,5): slice 0 of cell (11,5) goes, and
        // (11,5) stays untouched with a tunnel in it.
        field.dig(cell(10, 5).to_px(), Some(Direction::Right), &mut penalty);
        assert!(field.cell(cell(11, 5)).has_tunnel());
        assert!(field.cell(cell(11, 5)).is_untouched());
        // So (10,5) itself is solid; give (9,5)'s neighbour test a tunnel by
        // digging into (10,5) as well.
        field.dig(cell(9, 5).to_px(), Some(Direction::Right), &mut penalty);
        assert!(field.cell(cell(10, 5)).has_tunnel());

        let mut gfx = Gfx::new();
        full_draw(&mut gfx, &mut field, &EmeraldMap::EMPTY, player(0), plan(4));
        let Px { x, y } = cell(4, 5).to_px();
        // Nothing is carved into the solid right neighbour, x + 20 onward,
        // nor into the lower neighbour.
        for dy in -1..=16 {
            for dx in 20..40 {
                assert_eq!(
                    px(gfx.fb(), x + dx, y + dy),
                    tile_px(plan(4), x + dx, y + dy),
                    "right neighbour ({}, {})",
                    x + dx,
                    y + dy
                );
            }
        }
        for dx in -4..20 {
            for dy in 18..36 {
                assert_eq!(
                    px(gfx.fb(), x + dx, y + dy),
                    tile_px(plan(4), x + dx, y + dy),
                    "lower neighbour"
                );
            }
        }
        // The `H` at (9,5) has a tunnel to its right, so a join is carved
        // 4 pixels into (10,5).
        let Px { x, y } = cell(9, 5).to_px();
        for dx in 20..24 {
            assert!(is_black(gfx.fb(), x + dx, y + 8), "join at {}", x + dx);
        }
    }

    #[test]
    fn emerald_3_1_occupies_x72_y39() {
        assert_eq!(emerald_px(cell(3, 1)), Px::new(72, 39));
        let mut gfx = Gfx::new();
        draw_background(gfx.fb_mut(), plan(1));
        let background = gfx.fb().clone();
        draw_emerald(&mut gfx, cell(3, 1));
        let touched = changed(&background, gfx.fb());
        assert!(!touched.is_empty(), "something is drawn");
        assert!(
            touched
                .iter()
                .all(|&(x, y)| (72..=87).contains(&x) && (39..=48).contains(&y)),
            "{touched:?}"
        );
        // The image's rectangle is x 72..=87, y 39..=48; its silhouette
        // lies inside it.
        let rect = Rect::of_image(emerald_px(cell(3, 1)), ImageId::EMERALD.size());
        assert_eq!(rect, Rect::new(Px::new(72, 39), 16, 10));
        assert_eq!((rect.right(), rect.bottom()), (87, 48));
        let mut direct = background.clone();
        direct.blit_masked(ImageId::EMERALD, Px::new(72, 39));
        assert_eq!(gfx.fb(), &direct);

        // Erasing paints the silhouette black; nothing outside it changes.
        erase_emerald(&mut gfx, cell(3, 1));
        for (x, y) in changed(&background, gfx.fb()) {
            assert!((72..=87).contains(&x) && (39..=48).contains(&y));
        }
        for (i, (&was, &now)) in direct.pixels().iter().zip(gfx.fb().pixels()).enumerate() {
            assert!(was == now || now == 0, "real pixel {i} turned black");
        }
        assert_ne!(gfx.fb(), &direct);
        assert!(any_black(gfx.fb(), 80, 42), "the emerald's body is erased");
    }

    #[test]
    fn level_1_draw_takes_21_frames() {
        let set = LevelSet::builtin();
        let level = LevelNumber::FIRST;
        let p = player(0);
        let mut field = Field::for_level(&set, Some(level), false);
        let mut emeralds = EmeraldMap::EMPTY;
        emeralds.place_for_level(p, &set, Some(level), false);
        let opened = CellPos::all_column_major()
            .filter(|&c| !field.cell(c).is_untouched())
            .count();
        assert_eq!((opened, emeralds.count(p)), (33, 30));
        assert_eq!(LevelDraw::items(&field, &emeralds, p), 63);

        let mut normalised = field;
        normalised.normalise_for_redraw();
        let emeralds_before = emeralds;
        let mut gfx = Gfx::new();
        let (frames, bag_calls) = full_draw(&mut gfx, &mut field, &emeralds, p, level.plan());
        assert_eq!(frames, 21);
        assert_eq!(bag_calls, 1);
        assert_eq!(field, normalised, "the draw changes the field by Q05 only");
        assert_eq!(emeralds, emeralds_before);

        // The progressive draw ends with the same picture as drawing every
        // item at once, in the order of the module docs.
        let mut at_once = Gfx::new();
        draw_background(at_once.fb_mut(), level.plan());
        for c in CellPos::all_column_major().filter(|&c| !field.cell(c).is_untouched()) {
            draw_opened_cell(&mut at_once, &field, c);
        }
        for c in CellPos::all_column_major().filter(|&c| emeralds.has(p, c)) {
            draw_emerald(&mut at_once, c);
        }
        assert_eq!(gfx.fb(), at_once.fb());
        // The emerald of (3,1) is in the picture.
        let mut probe = at_once.fb().clone();
        probe.blit_masked(ImageId::EMERALD, emerald_px(cell(3, 1)));
        assert_eq!(&probe, gfx.fb());
    }

    #[test]
    fn draw_reveals_three_items_per_frame_and_bags_before_emeralds() {
        // Two opened cells and two emeralds: the bags come after the second
        // cell, in the first frame, before the first emerald.
        let mut field = Field::from_layout(&layout_with(&[(0, 0, b'S'), (5, 5, b'V')]));
        let mut emeralds = EmeraldMap::EMPTY;
        emeralds.place(player(1), &layout_with(&[(2, 2, b'C'), (7, 3, b'C')]));
        let p = player(1);
        let mut gfx = Gfx::new();
        let mut draw = LevelDraw::begin(&mut gfx, &mut field, plan(1));
        let before_emerald = std::cell::Cell::new(None);
        let progress = draw.frame(&mut gfx, &field, &emeralds, p, |g| {
            before_emerald.set(Some(g.fb().clone()));
        });
        assert_eq!(progress, DrawProgress::InProgress);
        let Some(at_bags) = before_emerald.take() else {
            unreachable!("the bags are drawn in the first frame")
        };
        // At the bag hook no emerald was drawn yet, and one was drawn after.
        let mut with_first = at_bags.clone();
        with_first.blit_masked(ImageId::EMERALD, emerald_px(cell(2, 2)));
        assert_eq!(&with_first, gfx.fb());
        assert_eq!(
            draw.frame(&mut gfx, &field, &emeralds, p, |_| unreachable!(
                "bags once"
            )),
            DrawProgress::Done
        );
        assert!(draw.is_done());
        // The first player's emeralds are not drawn for the second.
        let mut first_only = EmeraldMap::EMPTY;
        first_only.place(player(0), &layout_with(&[(1, 1, b'C')]));
        assert_eq!(LevelDraw::items(&field, &first_only, p), 2);
    }
}
