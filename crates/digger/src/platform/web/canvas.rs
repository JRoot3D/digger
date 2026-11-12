//! The game canvas and its softbuffer surface (add-web-build-and-releases,
//! design decision 6).
//!
//! The winit window is the page's `<canvas id="game">`. Its backing store
//! is the size of the picture's viewport in device pixels, worked out by
//! [`canvas_size`] from the browser viewport and the pixel ratio, and its
//! CSS size is that divided by the pixel ratio; the page centres it on
//! black. The browser viewport is read again on every animation callback
//! and on winit's `Resized` and `ScaleFactorChanged`, so a resize, a zoom,
//! a move to another display or fullscreen changes the scale from the next
//! presented picture on, without touching the game.

use std::fmt;
use std::num::NonZeroU32;
use std::rc::Rc;

use digger_core::video::Framebuffer;
use softbuffer::{Context, Surface};
use web_sys::HtmlCanvasElement;
use winit::event_loop::ActiveEventLoop;
use winit::platform::web::WindowAttributesExtWebSys;
use winit::window::Window;

use crate::config::ScaleMode;
use crate::present::blit::{blit, palette_table};
use crate::present::canvas::{BrowserViewport, CanvasSize, CssSize, canvas_size};
use crate::present::scale::Layout;
use crate::report;

/// The game canvas and its surface.
pub(super) struct GameCanvas {
    window: Rc<Window>,
    canvas: HtmlCanvasElement,
    /// Kept for the surface's lifetime.
    _context: Context<Rc<Window>>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    scale: ScaleMode,
    /// The browser viewport the size was worked out for.
    view: BrowserViewport,
    /// The canvas's current size.
    size: CanvasSize,
    /// How the picture fills the backing store.
    layout: Layout,
    /// A presentation failure was reported already; later ones are not.
    present_failed: bool,
}

impl fmt::Debug for GameCanvas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GameCanvas")
            .field("size", &self.size)
            .field("scale", &self.scale)
            .finish_non_exhaustive()
    }
}

/// The browser viewport now.
fn browser_viewport() -> BrowserViewport {
    let Some(window) = web_sys::window() else {
        return BrowserViewport::default();
    };
    let length = |value: Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>| {
        value.ok().and_then(|value| value.as_f64()).unwrap_or(0.0)
    };
    BrowserViewport {
        css: CssSize {
            width: length(window.inner_width()),
            height: length(window.inner_height()),
        },
        pixel_ratio: window.device_pixel_ratio(),
    }
}

impl GameCanvas {
    /// The winit window over `canvas`, with its surface.
    ///
    /// # Errors
    ///
    /// The reason, as text, when the window or the surface cannot be made.
    pub(super) fn open(
        event_loop: &ActiveEventLoop,
        canvas: HtmlCanvasElement,
        scale: ScaleMode,
    ) -> Result<Self, String> {
        let attributes = Window::default_attributes()
            .with_canvas(Some(canvas.clone()))
            // Only the keys bound to the game lose their browser action
            // (`dom::guard_bound_keys`); winit would block all of them.
            .with_prevent_default(false)
            .with_focusable(true);
        let window = event_loop
            .create_window(attributes)
            .map_err(|error| format!("cannot use the game canvas: {error}"))?;
        let window = Rc::new(window);
        let context = Context::new(Rc::clone(&window))
            .map_err(|error| format!("cannot draw into the game canvas: {error}"))?;
        let surface = Surface::new(&context, Rc::clone(&window))
            .map_err(|error| format!("cannot draw into the game canvas: {error}"))?;
        let mut game = Self {
            window,
            canvas,
            _context: context,
            surface,
            scale,
            view: BrowserViewport::default(),
            size: CanvasSize::default(),
            layout: Layout::new(0, 0, scale),
            present_failed: false,
        };
        game.update_layout();
        let _ = game.canvas.focus();
        Ok(game)
    }

    /// Asks for the next animation callback.
    pub(super) fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// The canvas element.
    pub(super) const fn element(&self) -> &HtmlCanvasElement {
        &self.canvas
    }

    /// The picture's whole-number scale, 0 when it is reduced or
    /// stretched.
    pub(super) const fn scale(&self) -> u32 {
        self.size.scale
    }

    /// Works the canvas size out again if the browser viewport changed;
    /// returns whether it did.
    pub(super) fn update_layout(&mut self) -> bool {
        let view = browser_viewport();
        if view == self.view {
            return false;
        }
        self.view = view;
        let size = canvas_size(view, self.scale);
        let style = self.canvas.style();
        let _ = style.set_property("width", &format!("{}px", size.css.width));
        let _ = style.set_property("height", &format!("{}px", size.css.height));
        self.size = size;
        self.layout = Layout::new(size.backing.width, size.backing.height, self.scale);
        true
    }

    /// Presents `picture` at the current size.
    pub(super) fn present(&mut self, picture: &Framebuffer) {
        let (Some(width), Some(height)) = (
            NonZeroU32::new(self.layout.width()),
            NonZeroU32::new(self.layout.height()),
        ) else {
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
}
