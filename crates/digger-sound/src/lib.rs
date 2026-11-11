//! Digger sound effects and music.
//!
//! This crate holds the sound-effect and music state machines and the sample
//! synthesis. It is pure: sound commands go in and PCM samples come out. It
//! opens no audio device and never feeds back into the simulation.

#![forbid(unsafe_code)]

#[cfg(test)]
mod tests {
    #[test]
    fn crate_builds() {
        assert_eq!(env!("CARGO_PKG_NAME"), "digger-sound");
    }
}
