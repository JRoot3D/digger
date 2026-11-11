//! The indexed picture and every pixel operation on it.
//!
//! # Logical and real coordinates
//!
//! The picture is a [`Framebuffer`] of 640x400 palette indices, stored at
//! real resolution because the art is genuine 640x400 art and the pixel
//! probe distinguishes real columns. Game logic, however, addresses it in a
//! 320x200 logical space ([`Px`](crate::geom::Px)): logical (x, y) is real
//! column 2x and real row 2y. Image sizes count width in units of 4 logical
//! pixels and height in logical pixels, so an image of size `w`, `h` covers
//! `8w x 2h` real pixels (research 03 §2). Every operation takes logical
//! coordinates, may be given positions partly or wholly outside the picture,
//! and clips to it; none can fail.
//!
//! # Why the picture is game state
//!
//! The original game decides whether a fireball has hit earth by reading
//! pixels back from the screen, so what is drawn, including text and the
//! background, influences the simulation. The picture is therefore owned by
//! the game and drawn into on every frame, whether or not a presenter ever
//! shows it, and every operation is exact and deterministic: the same
//! sequence of operations gives byte-identical pixels on every platform,
//! which [`Framebuffer::crc32`] checks (research 03 §11, §13 item 1). The
//! [`probe`](Framebuffer::probe) is the read-back.
//!
//! # Operations
//!
//! - [`clear`](Framebuffer::clear), [`set_palette`](Framebuffer::set_palette)
//!   and [`set_intensity`](Framebuffer::set_intensity).
//! - [`blit_masked`](Framebuffer::blit_masked): masked images.
//! - [`capture`](Framebuffer::capture) and [`restore`](Framebuffer::restore):
//!   [`SavedRect`] copies of picture areas, as the sprite layer uses them.
//! - [`draw_char`](Framebuffer::draw_char),
//!   [`draw_text`](Framebuffer::draw_text),
//!   [`erase_text`](Framebuffer::erase_text) and
//!   [`draw_number`](Framebuffer::draw_number): opaque 12x12 text cells in a
//!   [`TextColor`].
//! - [`draw_background`](Framebuffer::draw_background) for a [`LevelPlan`]
//!   and [`draw_title`](Framebuffer::draw_title).
//! - [`probe`](Framebuffer::probe), returning [`ProbeBits`].
//!
//! # Presentation
//!
//! A presenter reads [`Framebuffer::pixels`] and expands each index through
//! the 16 colours of [`Framebuffer::colors`]; reading changes nothing. There
//! is no present or flush call: when and whether to show a frame is the
//! caller's business.

mod backdrop;
mod crc;
mod framebuffer;
mod palette;
mod saved;
mod text;

pub use backdrop::LevelPlan;
pub use crc::crc32;
pub use framebuffer::{Framebuffer, ProbeBits};
pub use palette::{Intensity, Palette, Rgb, colors};
pub use saved::SavedRect;
pub use text::{CELL_ADVANCE, TextColor};

#[cfg(test)]
mod tests;
