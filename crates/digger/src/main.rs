//! The Digger desktop game.
//!
//! For now the binary only reports its name and version.

#![forbid(unsafe_code)]

fn main() {
    println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
}
