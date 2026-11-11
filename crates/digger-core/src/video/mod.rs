//! The indexed picture and every pixel operation on it.
//!
//! The picture is a [`Framebuffer`] of 640x400 palette indices. Game logic
//! addresses it in a 320x200 logical space ([`Px`](crate::geom::Px)):
//! logical (x, y) is real column 2x and real row 2y, and an image of width
//! `w` (units of 4 logical pixels) and height `h` covers `8w x 2h` real
//! pixels (research 03 §2). The art is genuine 640x400 art, so the picture
//! is stored at real resolution.
//!
//! The picture is game state: the fireball's [`probe`](Framebuffer::probe)
//! reads it back, so every operation is exact and deterministic whether or
//! not a presenter ever shows a frame. A presenter reads
//! [`Framebuffer::pixels`] and expands each index through
//! [`Framebuffer::colors`].

mod crc;
mod framebuffer;
mod palette;
mod saved;
mod text;

pub use crc::crc32;
pub use framebuffer::{Framebuffer, ProbeBits};
pub use palette::{Intensity, Palette, Rgb, colors};
pub use saved::SavedRect;
pub use text::{CELL_ADVANCE, TextColor};

#[cfg(test)]
mod tests;
