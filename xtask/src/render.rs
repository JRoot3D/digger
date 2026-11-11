//! `cargo xtask render-sheet`: write the golden scenes and a contact sheet of
//! all images as PPM files, for a person to check by eye before golden
//! checksums are recorded (design D8).
//!
//! Every picture is translated through palette 0 at normal intensity. The
//! scenes are the very ones `crates/digger-core/tests/golden_frames.rs` pins.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::Args;
use digger_core::assets::TitlePicture;
use digger_core::video::{Framebuffer, LevelPlan, Palette};

#[path = "../../crates/digger-core/tests/common/scenes.rs"]
mod scenes;

/// Arguments of `cargo xtask render-sheet`.
#[derive(Debug, Args)]
pub(crate) struct RenderArgs {
    /// Directory the PPM files are written to [default: target/sheets/ in
    /// the workspace root].
    #[arg(long, value_name = "DIR")]
    pub(crate) out: Option<PathBuf>,
}

/// Why `cargo xtask render-sheet` failed.
#[derive(Debug)]
pub(crate) enum RenderError {
    /// The embedded title picture did not decode.
    Title(digger_core::AssetError),
    /// A file could not be written.
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Title(error) => write!(f, "title picture: {error}"),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Title(error) => Some(error),
            Self::Io { source, .. } => Some(source),
        }
    }
}

fn default_out_dir() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap_or(manifest_dir)
        .join("target")
        .join("sheets")
}

/// Every picture the command writes, with its file stem, in a fixed order.
fn pictures(title: &TitlePicture) -> Vec<(String, Framebuffer)> {
    let mut out = vec![
        ("contact-sheet".to_owned(), scenes::contact_sheet()),
        ("font".to_owned(), scenes::text_sheet()),
        ("numbers".to_owned(), scenes::number_sheet()),
        ("title".to_owned(), scenes::title(title)),
        ("title-text".to_owned(), scenes::title_with_text(title)),
    ];
    for plan in LevelPlan::all() {
        out.push((
            format!("background-{}", plan.number()),
            scenes::background(plan),
        ));
    }
    out.push(("sprite-scene".to_owned(), scenes::sprite_scene().0));
    out
}

/// The picture as a binary PPM (P6), 640x400, through palette 0 at normal
/// intensity.
pub(crate) fn ppm(fb: &Framebuffer) -> Vec<u8> {
    let mut fb = fb.clone();
    // Selecting a palette also selects normal intensity (quirk R3).
    fb.set_palette(Palette::Zero);
    let colors = fb.colors();
    let header = format!("P6\n{} {}\n255\n", Framebuffer::WIDTH, Framebuffer::HEIGHT);
    let mut out = Vec::with_capacity(header.len() + 3 * Framebuffer::BYTES);
    out.extend_from_slice(header.as_bytes());
    for &index in fb.pixels() {
        let rgb = colors
            .get(usize::from(index & 0x0F))
            .copied()
            .unwrap_or_default();
        out.extend_from_slice(&[rgb.r, rgb.g, rgb.b]);
    }
    out
}

/// Runs `cargo xtask render-sheet`.
pub(crate) fn run(args: &RenderArgs) -> Result<(), RenderError> {
    let out_dir = args.out.clone().unwrap_or_else(default_out_dir);
    fs::create_dir_all(&out_dir).map_err(|source| RenderError::Io {
        path: out_dir.clone(),
        source,
    })?;
    let title = TitlePicture::decode().map_err(RenderError::Title)?;
    for (stem, fb) in pictures(&title) {
        let path = out_dir.join(format!("{stem}.ppm"));
        fs::write(&path, ppm(&fb)).map_err(|source| RenderError::Io {
            path: path.clone(),
            source,
        })?;
        println!(
            "render-sheet: {} (CRC-32 {:08X})",
            path.display(),
            fb.crc32()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppm_header_and_size() {
        let fb = Framebuffer::new();
        let bytes = ppm(&fb);
        assert!(bytes.starts_with(b"P6\n640 400\n255\n"));
        assert_eq!(bytes.len(), 15 + 3 * 256_000);
    }

    #[test]
    fn pictures_cover_every_sheet() {
        let title = TitlePicture::decode().unwrap_or_else(|e| unreachable!("title decodes: {e}"));
        let names: Vec<String> = pictures(&title).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names.len(), 14);
        assert!(names.contains(&"sprite-scene".to_owned()));
        assert!(names.contains(&"background-8".to_owned()));
    }
}
