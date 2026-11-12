//! Window attributes, the icon, fullscreen and the softbuffer surface
//! (design D4, D5).
//!
//! The window is created in `resumed`, as winit requires on every platform
//! (and Wayland and the browser need for the surface). Its client area is
//! requested in physical pixels: 1280×800 by default, 640×400 at least
//! (`desktop-app` spec, game window). Presenting expands the picture's
//! palette indices into the surface through the pure maths of
//! [`crate::present`]; the layout is worked out again whenever the window's
//! physical size changes (a resize or a scale-factor change).

use std::fmt;
use std::num::NonZeroU32;
use std::rc::Rc;

use digger_core::video::Framebuffer;
use softbuffer::{Context, Surface};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Fullscreen, Icon, Window, WindowAttributes};

use crate::config::ScaleMode;
use crate::present::blit::{blit, palette_table};
use crate::present::scale::{Layout, PICTURE_HEIGHT, PICTURE_WIDTH};
use crate::report;

/// The window title (research 03 §10).
pub(super) const TITLE: &str = "D I G G E R";
/// The client area the window opens with: the picture at scale 2.
pub(super) const DEFAULT_SIZE: PhysicalSize<u32> = PhysicalSize::new(1280, 800);
/// The smallest client area: the picture at scale 1.
pub(super) const MIN_SIZE: PhysicalSize<u32> = PhysicalSize::new(PICTURE_WIDTH, PICTURE_HEIGHT);

/// The 64×64 RGBA window icon imported from upstream (`assets/icon.rgba`).
const ICON_RGBA: &[u8] = include_bytes!("../../../../assets/icon.rgba");
/// The icon's width and height.
const ICON_SIDE: u32 = 64;

/// The window attributes the game opens with.
pub(super) fn attributes(fullscreen: bool) -> WindowAttributes {
    let mut attributes = Window::default_attributes()
        .with_title(TITLE)
        .with_inner_size(DEFAULT_SIZE)
        .with_min_inner_size(MIN_SIZE)
        .with_resizable(true)
        .with_window_icon(icon());
    if fullscreen {
        attributes = attributes.with_fullscreen(Some(Fullscreen::Borderless(None)));
    }
    attributes
}

/// The window icon; platforms without per-window icons (Wayland, macOS)
/// ignore it.
fn icon() -> Option<Icon> {
    Icon::from_rgba(ICON_RGBA.to_vec(), ICON_SIDE, ICON_SIDE).ok()
}

/// Where the window was before it went fullscreen.
#[derive(Debug, Clone, Copy)]
struct Windowed {
    size: PhysicalSize<u32>,
    /// `None` where the platform does not tell (Wayland).
    position: Option<PhysicalPosition<i32>>,
}

/// The game window and its softbuffer surface.
pub(super) struct GameWindow {
    window: Rc<Window>,
    /// Kept for the surface's lifetime.
    _context: Context<Rc<Window>>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    /// How the picture fits the current client area.
    layout: Layout,
    scale: ScaleMode,
    /// Whether the window is fullscreen, as last requested.
    fullscreen: bool,
    /// The window's geometry to return to when fullscreen ends.
    windowed: Option<Windowed>,
    /// A presentation failure was reported already; later ones are not.
    present_failed: bool,
}

impl fmt::Debug for GameWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GameWindow")
            .field("window", &self.window.id())
            .field("layout", &self.layout.viewport())
            .field("scale", &self.scale)
            .field("fullscreen", &self.fullscreen)
            .finish_non_exhaustive()
    }
}

impl GameWindow {
    /// Opens the window and its surface.
    ///
    /// # Errors
    ///
    /// The reason, as text, when the window or the surface cannot be made.
    pub(super) fn open(
        event_loop: &ActiveEventLoop,
        fullscreen: bool,
        scale: ScaleMode,
    ) -> Result<Self, String> {
        let window = event_loop
            .create_window(attributes(fullscreen))
            .map_err(|error| format!("cannot open the game window: {error}"))?;
        let window = Rc::new(window);
        let context = Context::new(Rc::clone(&window))
            .map_err(|error| format!("cannot draw into the game window: {error}"))?;
        let surface = Surface::new(&context, Rc::clone(&window))
            .map_err(|error| format!("cannot draw into the game window: {error}"))?;
        if fullscreen {
            window.set_cursor_visible(false);
        }
        let size = window.inner_size();
        Ok(Self {
            window,
            _context: context,
            surface,
            layout: Layout::new(size.width, size.height, scale),
            scale,
            fullscreen,
            // Started fullscreen, the window returns to its default size.
            windowed: fullscreen.then_some(Windowed {
                size: DEFAULT_SIZE,
                position: None,
            }),
            present_failed: false,
        })
    }

    /// Asks for the picture to be presented.
    pub(super) fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// The client area changed size (a resize, a scale-factor change or a
    /// fullscreen switch): the layout follows and the picture is presented
    /// again.
    pub(super) fn resized(&mut self) {
        self.update_layout();
        self.window.request_redraw();
    }

    /// Works the layout out again if the client area's size changed.
    fn update_layout(&mut self) {
        let size = self.window.inner_size();
        if size.width != self.layout.width() || size.height != self.layout.height() {
            self.layout = Layout::new(size.width, size.height, self.scale);
        }
    }

    /// Presents `picture`: the viewport scaled through its current palette,
    /// black around it.
    pub(super) fn present(&mut self, picture: &Framebuffer) {
        self.update_layout();
        let (Some(width), Some(height)) = (
            NonZeroU32::new(self.layout.width()),
            NonZeroU32::new(self.layout.height()),
        ) else {
            // A minimised window has no client area to draw into.
            return;
        };
        let result = self.surface.resize(width, height).and_then(|()| {
            let mut buffer = self.surface.buffer_mut()?;
            blit(
                picture.pixels(),
                &palette_table(picture.colors()),
                &self.layout,
                &mut buffer,
            );
            buffer.present()
        });
        if let Err(error) = result
            && !self.present_failed
        {
            self.present_failed = true;
            report::notice(format!("cannot present the picture: {error}"));
        }
    }

    /// Switches between the window and borderless fullscreen on the current
    /// monitor (`desktop-app` spec, fullscreen): the pointer is hidden while
    /// fullscreen, and the window returns with its previous size and
    /// position. Returns whether the window is now fullscreen.
    pub(super) fn toggle_fullscreen(&mut self) -> bool {
        if self.fullscreen {
            self.window.set_fullscreen(None);
            self.window.set_cursor_visible(true);
            if let Some(windowed) = self.windowed.take() {
                // The platform may already have restored it; asking again
                // is harmless. The size it settles on arrives as a resize.
                let _ = self.window.request_inner_size(windowed.size);
                if let Some(position) = windowed.position {
                    self.window.set_outer_position(position);
                }
            }
            self.fullscreen = false;
        } else {
            self.windowed = Some(Windowed {
                size: self.window.inner_size(),
                position: self.window.outer_position().ok(),
            });
            self.window
                .set_fullscreen(Some(Fullscreen::Borderless(None)));
            self.window.set_cursor_visible(false);
            self.fullscreen = true;
        }
        self.window.request_redraw();
        self.fullscreen
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_SIZE, ICON_RGBA, ICON_SIDE, MIN_SIZE, icon};
    use crate::config::ScaleMode;
    use crate::present::scale::viewport;

    #[test]
    fn icon_is_64_by_64_rgba() {
        assert_eq!(ICON_RGBA.len(), 64 * 64 * 4);
        assert_eq!(ICON_SIDE, 64);
        assert!(icon().is_some());
    }

    #[test]
    fn default_window_shows_the_picture_at_scale_2() {
        let view = viewport(DEFAULT_SIZE.width, DEFAULT_SIZE.height, ScaleMode::Integer);
        assert_eq!((view.x, view.y, view.w, view.h), (0, 0, 1280, 800));
        assert_eq!((MIN_SIZE.width, MIN_SIZE.height), (640, 400));
    }
}
