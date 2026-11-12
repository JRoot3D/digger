//! The size of the browser's game canvas (`web-build` spec, canvas
//! presentation and scaling; add-web-build-and-releases, design decision
//! 6). Pure, so it is tested on the host.
//!
//! The scaling rule is the desktop's [`viewport`], applied to the browser
//! viewport in device pixels. The canvas holds only the picture: its
//! backing store is the viewport rectangle's size, and its size in CSS
//! pixels is that divided by the device pixel ratio, so each backing pixel
//! is one device pixel. The page centres it on black, which is the
//! letterbox; nothing but game pixels is ever in the canvas.
//!
//! Device pixels and CSS pixels are distinct types, so the two cannot be
//! mixed up.

use super::scale::viewport;
use crate::config::ScaleMode;

/// A size in device (physical) pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct DeviceSize {
    /// Width, device pixels.
    pub(crate) width: u32,
    /// Height, device pixels.
    pub(crate) height: u32,
}

/// A size in CSS pixels, which may be fractional.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct CssSize {
    /// Width, CSS pixels.
    pub(crate) width: f64,
    /// Height, CSS pixels.
    pub(crate) height: f64,
}

/// What the browser reports about its viewport.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct BrowserViewport {
    /// `window.innerWidth` × `window.innerHeight`.
    pub(crate) css: CssSize,
    /// `window.devicePixelRatio`.
    pub(crate) pixel_ratio: f64,
}

/// The canvas for one browser viewport.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct CanvasSize {
    /// The backing store: the picture's size in device pixels.
    pub(crate) backing: DeviceSize,
    /// The element's size on the page.
    pub(crate) css: CssSize,
    /// The whole-number scale of the picture, 0 when it is reduced or
    /// stretched to a size that is no multiple of 640×400.
    pub(crate) scale: u32,
}

/// Device pixels from a CSS length and a pixel ratio, rounded to the
/// nearest pixel; anything not finite or negative counts as nothing.
fn device(css: f64, ratio: f64) -> u32 {
    let pixels = (css * ratio).round();
    if pixels.is_finite() && pixels >= 1.0 {
        // In range: checked above, and a browser viewport is far below
        // u32::MAX device pixels.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a positive whole number, clamped to the u32 range"
        )]
        let pixels = pixels.min(f64::from(u32::MAX)) as u32;
        pixels
    } else {
        0
    }
}

/// The canvas for `view` in scale `mode` (the stored `video.scale`).
pub(crate) fn canvas_size(view: BrowserViewport, mode: ScaleMode) -> CanvasSize {
    let ratio = if view.pixel_ratio.is_finite() && view.pixel_ratio > 0.0 {
        view.pixel_ratio
    } else {
        1.0
    };
    let rect = viewport(
        device(view.css.width, ratio),
        device(view.css.height, ratio),
        mode,
    );
    let scale =
        if rect.w.is_multiple_of(640) && rect.h.is_multiple_of(400) && rect.w / 640 == rect.h / 400
        {
            rect.w / 640
        } else {
            0
        };
    CanvasSize {
        backing: DeviceSize {
            width: rect.w,
            height: rect.h,
        },
        css: CssSize {
            width: f64::from(rect.w) / ratio,
            height: f64::from(rect.h) / ratio,
        },
        scale,
    }
}

#[cfg(test)]
mod tests {
    use super::{BrowserViewport, CanvasSize, CssSize, DeviceSize, canvas_size};
    use crate::config::ScaleMode;

    fn view(width: f64, height: f64, pixel_ratio: f64) -> BrowserViewport {
        BrowserViewport {
            css: CssSize { width, height },
            pixel_ratio,
        }
    }

    fn backing(size: CanvasSize) -> (u32, u32) {
        (size.backing.width, size.backing.height)
    }

    #[test]
    fn canvas_full_hd_is_scale_2() {
        let size = canvas_size(view(1920.0, 1080.0, 1.0), ScaleMode::Integer);
        assert_eq!(
            size,
            CanvasSize {
                backing: DeviceSize {
                    width: 1280,
                    height: 800
                },
                css: CssSize {
                    width: 1280.0,
                    height: 800.0
                },
                scale: 2,
            }
        );
    }

    #[test]
    fn canvas_high_density_is_scale_3() {
        // 1280×720 CSS pixels at 2 device pixels each: 2560×1440 device
        // pixels, scale 3, every game pixel a 3×3 block of device pixels.
        let size = canvas_size(view(1280.0, 720.0, 2.0), ScaleMode::Integer);
        assert_eq!(backing(size), (1920, 1200));
        assert_eq!(size.scale, 3);
        assert!((size.css.width - 960.0).abs() < 1e-9);
        assert!((size.css.height - 600.0).abs() < 1e-9);
    }

    #[test]
    fn canvas_follows_the_viewport() {
        // Shrinking from 1920×1080 to 1000×700 device pixels: scale 1.
        assert_eq!(
            backing(canvas_size(view(1000.0, 700.0, 1.0), ScaleMode::Integer)),
            (640, 400)
        );
        // Fullscreen on 2560×1440: scale 3.
        assert_eq!(
            canvas_size(view(2560.0, 1440.0, 1.0), ScaleMode::Integer).scale,
            3
        );
        // Below 640×400 the whole picture, reduced to fit, 8:5 kept.
        let small = canvas_size(view(320.0, 400.0, 1.0), ScaleMode::Integer);
        assert_eq!(backing(small), (320, 200));
        assert_eq!(small.scale, 0);
        // A fractional pixel ratio rounds to whole device pixels.
        let zoomed = canvas_size(view(1536.0, 864.0, 1.25), ScaleMode::Integer);
        assert_eq!(backing(zoomed), (1280, 800));
        assert!((zoomed.css.width - 1024.0).abs() < 1e-9);
        // The other stored modes apply as on the desktop.
        assert_eq!(
            backing(canvas_size(view(1920.0, 1080.0, 1.0), ScaleMode::Fit)),
            (1728, 1080)
        );
        assert_eq!(
            backing(canvas_size(view(1920.0, 1080.0, 1.0), ScaleMode::Stretch)),
            (1920, 1080)
        );
        // Nonsense from the browser gives an empty canvas, never a panic.
        assert_eq!(
            backing(canvas_size(view(0.0, 0.0, 1.0), ScaleMode::Integer)),
            (0, 0)
        );
        assert_eq!(
            backing(canvas_size(
                view(f64::NAN, 1080.0, f64::INFINITY),
                ScaleMode::Integer
            )),
            (0, 0)
        );
        assert_eq!(
            backing(canvas_size(view(1920.0, 1080.0, 0.0), ScaleMode::Integer)),
            (1280, 800)
        );
    }
}
