//! The platform layer: the only modules that use `winit`, `softbuffer`,
//! `cpal` and `gilrs` (design D1). Nothing outside this module imports them;
//! the test `platform_imports_are_confined` checks it.

mod app;
mod audio;
mod pads;
mod window;
