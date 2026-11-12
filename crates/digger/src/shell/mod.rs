//! The device-free program logic, driven one frame at a time: `Shell`,
//! `Scene`, `ShellInput` and `FrameOutput` (design D2). Nothing here touches a
//! window, an audio device or a gamepad, so the browser build can reuse it.

mod game_over;
mod initials;
mod play;
mod recording;
mod redefine;
mod title;
