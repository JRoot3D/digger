//! The viewport for each scale mode and the column and row maps (design D5).
//!
//! Everything here is in physical pixels and integer arithmetic. The
//! picture is 640×400; the viewport is the rectangle of the window it is
//! drawn into, centred, and the rest of the window is black (`desktop-app`
//! spec, integer-scaled presentation and other scale modes).

use crate::config::ScaleMode;

/// The width of the picture in pixels.
pub(crate) const PICTURE_WIDTH: u32 = 640;
/// The height of the picture in pixels.
pub(crate) const PICTURE_HEIGHT: u32 = 400;

/// A rectangle of the window, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct Rect {
    /// The left column.
    pub(crate) x: u32,
    /// The top row.
    pub(crate) y: u32,
    /// The width.
    pub(crate) w: u32,
    /// The height.
    pub(crate) h: u32,
}

/// Where the picture goes in a window of `win_w`×`win_h` pixels.
///
/// `integer` takes the largest whole multiple of 640×400 that fits, and is
/// `fit` when not even 1× fits; `fit` takes the largest 8:5 rectangle;
/// `stretch` takes the whole window. The result is centred. A window with
/// no pixels gets an empty viewport.
pub(crate) fn viewport(win_w: u32, win_h: u32, mode: ScaleMode) -> Rect {
    if win_w == 0 || win_h == 0 {
        return Rect::default();
    }
    let (w, h) = match mode {
        ScaleMode::Integer => {
            let k = (win_w / PICTURE_WIDTH).min(win_h / PICTURE_HEIGHT);
            if k == 0 {
                fit(win_w, win_h)
            } else {
                (PICTURE_WIDTH * k, PICTURE_HEIGHT * k)
            }
        }
        ScaleMode::Fit => fit(win_w, win_h),
        ScaleMode::Stretch => (win_w, win_h),
    };
    Rect {
        x: (win_w - w) / 2,
        y: (win_h - h) / 2,
        w,
        h,
    }
}

/// The largest 8:5 size inside `win_w`×`win_h`, both at least 1.
fn fit(win_w: u32, win_h: u32) -> (u32, u32) {
    let (ww, wh) = (u64::from(win_w), u64::from(win_h));
    let (pw, ph) = (u64::from(PICTURE_WIDTH), u64::from(PICTURE_HEIGHT));
    if ww * ph <= wh * pw {
        // Narrower than 8:5: full width, height from it (never above
        // `win_h`, so it fits in u32).
        (win_w, narrow(ww * ph / pw).max(1))
    } else {
        (narrow(wh * pw / ph).max(1), win_h)
    }
}

/// A size computed in u64 that is known to be no larger than a u32 window
/// size.
fn narrow(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// A window size and scale mode worked out once per resize: the viewport
/// and, for each of its columns and rows, the picture column and row shown
/// there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layout {
    win_w: u32,
    win_h: u32,
    viewport: Rect,
    col_map: Vec<u16>,
    row_map: Vec<u16>,
}

impl Layout {
    /// The layout of a `win_w`×`win_h` window in `mode`.
    pub(crate) fn new(win_w: u32, win_h: u32, mode: ScaleMode) -> Self {
        let viewport = viewport(win_w, win_h, mode);
        Self {
            win_w,
            win_h,
            viewport,
            col_map: source_map(viewport.w, PICTURE_WIDTH),
            row_map: source_map(viewport.h, PICTURE_HEIGHT),
        }
    }

    /// The window width.
    pub(crate) const fn width(&self) -> u32 {
        self.win_w
    }

    /// The window height.
    pub(crate) const fn height(&self) -> u32 {
        self.win_h
    }

    /// Where the picture is drawn.
    pub(crate) const fn viewport(&self) -> Rect {
        self.viewport
    }

    /// For each viewport column, the picture column shown there:
    /// `dx × 640 / w`.
    pub(crate) fn col_map(&self) -> &[u16] {
        &self.col_map
    }

    /// For each viewport row, the picture row shown there: `dy × 400 / h`.
    pub(crate) fn row_map(&self) -> &[u16] {
        &self.row_map
    }
}

/// `dst` destination positions mapped onto `src` source positions:
/// `d × src / dst`, so every value is below `src`.
fn source_map(dst: u32, src: u32) -> Vec<u16> {
    (0..u64::from(dst))
        .map(|d| {
            let s = d * u64::from(src) / u64::from(dst);
            u16::try_from(s).unwrap_or(u16::MAX)
        })
        .collect()
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::*;

    const fn rect(x: u32, y: u32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn viewport_integer_1920x1080() {
        let v = viewport(1920, 1080, ScaleMode::Integer);
        assert_eq!(v, rect(320, 140, 1280, 800));
        // 320 black columns on each side, 140 black rows above and below.
        assert_eq!(1920 - v.x - v.w, 320);
        assert_eq!(1080 - v.y - v.h, 140);
    }

    #[test]
    fn viewport_integer_1300x1300() {
        assert_eq!(
            viewport(1300, 1300, ScaleMode::Integer),
            rect(10, 250, 1280, 800)
        );
    }

    #[test]
    fn viewport_integer_exact_and_large() {
        assert_eq!(viewport(640, 400, ScaleMode::Integer), rect(0, 0, 640, 400));
        assert_eq!(
            viewport(3840, 2160, ScaleMode::Integer),
            rect(320, 80, 3200, 2000)
        );
    }

    #[test]
    fn viewport_fit_1920x1080() {
        let v = viewport(1920, 1080, ScaleMode::Fit);
        assert_eq!(v, rect(96, 0, 1728, 1080));
        assert_eq!(1920 - v.x - v.w, 96);
    }

    #[test]
    fn viewport_fit_tall_window() {
        assert_eq!(viewport(800, 900, ScaleMode::Fit), rect(0, 200, 800, 500));
    }

    #[test]
    fn viewport_integer_small_falls_back() {
        let v = viewport(500, 400, ScaleMode::Integer);
        assert_eq!(v, rect(0, 44, 500, 312));
        assert_eq!(400 - v.y - v.h, 44);
        assert_eq!(v, viewport(500, 400, ScaleMode::Fit));
        // Wide enough but too low.
        assert_eq!(
            viewport(1000, 300, ScaleMode::Integer),
            rect(260, 0, 480, 300)
        );
    }

    #[test]
    fn viewport_stretch() {
        assert_eq!(
            viewport(1920, 1080, ScaleMode::Stretch),
            rect(0, 0, 1920, 1080)
        );
        assert_eq!(viewport(7, 3, ScaleMode::Stretch), rect(0, 0, 7, 3));
    }

    #[test]
    fn viewport_tiny_and_empty_windows() {
        for mode in ScaleMode::ALL {
            assert_eq!(viewport(1, 1, mode), rect(0, 0, 1, 1), "{mode}");
            assert_eq!(viewport(0, 400, mode), Rect::default(), "{mode}");
            assert_eq!(viewport(640, 0, mode), Rect::default(), "{mode}");
        }
        // A very wide strip still gets one row and an 8:5 width.
        assert_eq!(viewport(1000, 1, ScaleMode::Fit), rect(499, 0, 1, 1));
        assert_eq!(viewport(1, 1000, ScaleMode::Fit), rect(0, 499, 1, 1));
    }

    #[test]
    fn viewport_odd_sizes_fit_inside_and_centre() {
        for (w, h) in [
            (641, 401),
            (1279, 799),
            (1366, 768),
            (1001, 333),
            (333, 1001),
        ] {
            for mode in ScaleMode::ALL {
                let v = viewport(w, h, mode);
                assert!(v.w >= 1 && v.h >= 1, "{w}x{h} {mode}");
                assert!(v.x + v.w <= w && v.y + v.h <= h, "{w}x{h} {mode}");
                assert!(
                    (w - v.w) / 2 == v.x && (h - v.h) / 2 == v.y,
                    "{w}x{h} {mode}"
                );
            }
        }
    }

    #[test]
    fn maps_monotonic() {
        for (w, h) in [
            (640, 400),
            (1280, 800),
            (1920, 1080),
            (500, 400),
            (1, 1),
            (7, 3),
            (1366, 768),
        ] {
            for mode in ScaleMode::ALL {
                let layout = Layout::new(w, h, mode);
                let v = layout.viewport();
                let cols = layout.col_map();
                let rows = layout.row_map();
                assert_eq!(cols.len(), usize::try_from(v.w).unwrap_or(0));
                assert_eq!(rows.len(), usize::try_from(v.h).unwrap_or(0));
                assert_eq!(cols.first(), Some(&0));
                assert_eq!(rows.first(), Some(&0));
                assert!(cols.windows(2).all(|p| p[0] <= p[1]), "{w}x{h} {mode}");
                assert!(rows.windows(2).all(|p| p[0] <= p[1]), "{w}x{h} {mode}");
                assert!(cols.iter().all(|&c| c < 640));
                assert!(rows.iter().all(|&r| r < 400));
                // When the viewport is at least as large as the picture,
                // every picture column and row is shown.
                if v.w >= 640 {
                    assert_eq!(cols.last(), Some(&639));
                    assert!(cols.windows(2).all(|p| p[1] - p[0] <= 1));
                }
                if v.h >= 400 {
                    assert_eq!(rows.last(), Some(&399));
                    assert!(rows.windows(2).all(|p| p[1] - p[0] <= 1));
                }
            }
        }
        let layout = Layout::new(1280, 800, ScaleMode::Integer);
        assert_eq!(&layout.col_map()[..6], &[0, 0, 1, 1, 2, 2]);
        assert_eq!((layout.width(), layout.height()), (1280, 800));
    }
}
