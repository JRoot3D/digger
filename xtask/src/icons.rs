//! `cargo xtask icons`: the desktop icons of the release archives
//! (add-web-build-and-releases, task 6.2), made from the game's own 64×64
//! window icon (`assets/icon.rgba`).
//!
//! - `packaging/linux/digger.png`: the icon as a 64×64 PNG, for the Linux
//!   desktop entry.
//! - `packaging/macos/digger.icns`: an Apple icon file holding the icon at
//!   64, 128, 256 and 512 pixels, each a PNG scaled up by whole numbers
//!   (every icon pixel a sharp square), for `Digger.app`.
//!
//! Both files are committed; `--check` (and the test below) compares them
//! with what this command writes, so they cannot drift from the game icon.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;
use flate2::Crc;

/// The side of the game icon, in pixels.
const ICON_SIDE: usize = 64;

/// Arguments of `cargo xtask icons`.
#[derive(Debug, Args)]
pub(crate) struct IconsArgs {
    /// Compare the committed files with the generated ones instead of
    /// writing them; fails if they differ.
    #[arg(long)]
    check: bool,
}

/// The workspace root.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// The generated files: path relative to the workspace and content.
pub(crate) fn generate(root: &Path) -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let path = root.join("assets/icon.rgba");
    let rgba =
        fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if rgba.len() != ICON_SIDE * ICON_SIDE * 4 {
        return Err(format!(
            "{} holds {} bytes, not a 64x64 RGBA image",
            path.display(),
            rgba.len()
        ));
    }
    Ok(vec![
        ("packaging/linux/digger.png", png(&rgba, ICON_SIDE)),
        ("packaging/macos/digger.icns", icns(&rgba)),
    ])
}

/// Runs `cargo xtask icons`.
pub(crate) fn run(args: &IconsArgs) -> Result<(), String> {
    let root = workspace_root();
    let mut stale = Vec::new();
    for (name, bytes) in generate(&root)? {
        let path = root.join(name);
        if args.check {
            if fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
                stale.push(name);
            }
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        fs::write(&path, &bytes)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        println!("xtask icons: wrote {name} ({} bytes)", bytes.len());
    }
    if stale.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} differ from the game icon; run `cargo xtask icons`",
            stale.join(", ")
        ))
    }
}

/// `rgba` (a square image `side` pixels wide, 4 bytes per pixel) scaled up
/// by the whole number `factor`, each pixel becoming a `factor`-square.
fn scale_up(rgba: &[u8], side: usize, factor: usize) -> Vec<u8> {
    let out_side = side * factor;
    let mut out = Vec::with_capacity(out_side * out_side * 4);
    for row in rgba.chunks_exact(side * 4) {
        let mut scaled_row = Vec::with_capacity(out_side * 4);
        for pixel in row.as_chunks::<4>().0 {
            for _ in 0..factor {
                scaled_row.extend_from_slice(pixel);
            }
        }
        for _ in 0..factor {
            out.extend_from_slice(&scaled_row);
        }
    }
    out
}

/// A PNG file of the square RGBA image `rgba`, `side` pixels wide: 8 bits
/// per channel, no filtering, no metadata, so the output depends only on
/// the pixels.
pub(crate) fn png(rgba: &[u8], side: usize) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgba.len() + side);
    for row in rgba.chunks_exact(side * 4) {
        // Filter type 0 (none) before every row.
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let side = u32::try_from(side).unwrap_or(u32::MAX);
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&side.to_be_bytes());
    header.extend_from_slice(&side.to_be_bytes());
    // Bit depth 8, colour type 6 (RGBA), deflate, adaptive filtering, no
    // interlace.
    header.extend_from_slice(&[8, 6, 0, 0, 0]);

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    chunk(&mut out, *b"IHDR", &header);
    chunk(
        &mut out,
        *b"IDAT",
        &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 9),
    );
    chunk(&mut out, *b"IEND", &[]);
    out
}

/// Appends one PNG chunk: length, type, data and the CRC-32 of type and
/// data.
fn chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&u32::try_from(data.len()).unwrap_or(u32::MAX).to_be_bytes());
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let mut crc = Crc::new();
    crc.update(&kind);
    crc.update(data);
    out.extend_from_slice(&crc.sum().to_be_bytes());
}

/// An Apple icon file of the 64×64 icon `rgba`: the PNG entries `icp6`
/// (64 pixels), `ic07` (128), `ic08` (256) and `ic09` (512).
pub(crate) fn icns(rgba: &[u8]) -> Vec<u8> {
    let entries: [([u8; 4], usize); 4] =
        [(*b"icp6", 1), (*b"ic07", 2), (*b"ic08", 4), (*b"ic09", 8)];
    let mut body = Vec::new();
    for (kind, factor) in entries {
        let image = png(&scale_up(rgba, ICON_SIDE, factor), ICON_SIDE * factor);
        body.extend_from_slice(&kind);
        body.extend_from_slice(
            &u32::try_from(image.len() + 8)
                .unwrap_or(u32::MAX)
                .to_be_bytes(),
        );
        body.extend_from_slice(&image);
    }
    let mut out = b"icns".to_vec();
    out.extend_from_slice(
        &u32::try_from(body.len() + 8)
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    out.extend_from_slice(&body);
    out
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::{generate, icns, png, scale_up, workspace_root};

    #[test]
    fn packaging_icons_match_game_icon() {
        let root = workspace_root();
        for (name, bytes) in generate(&root).unwrap() {
            let committed = std::fs::read(root.join(name)).unwrap_or_default();
            assert!(
                committed == bytes,
                "{name} differs from the game icon; run `cargo xtask icons`"
            );
        }
    }

    #[test]
    fn png_has_signature_header_and_end() {
        let image = png(&[255; 2 * 2 * 4], 2);
        assert_eq!(&image[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&image[12..16], b"IHDR");
        assert_eq!(&image[16..24], &[0, 0, 0, 2, 0, 0, 0, 2]);
        assert_eq!(&image[image.len() - 12..image.len() - 4], b"\0\0\0\0IEND");
        // The CRC-32 of an empty IEND chunk is a known constant.
        assert_eq!(&image[image.len() - 4..], &[0xae, 0x42, 0x60, 0x82]);
    }

    #[test]
    fn scaling_makes_squares() {
        let red_blue = [
            255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255, 255, 0, 0, 255,
        ];
        let scaled = scale_up(&red_blue, 2, 2);
        assert_eq!(scaled.len(), 4 * 4 * 4);
        // Row 1 equals row 0; pixel (1, 0) equals pixel (0, 0).
        assert_eq!(&scaled[..16], &scaled[16..32]);
        assert_eq!(&scaled[..4], &scaled[4..8]);
        assert_eq!(&scaled[8..12], &[0, 0, 255, 255]);
    }

    #[test]
    fn icns_lists_four_sizes() {
        let file = icns(&[0; 64 * 64 * 4]);
        assert_eq!(&file[..4], b"icns");
        assert_eq!(
            u32::from_be_bytes(file[4..8].try_into().unwrap()) as usize,
            file.len()
        );
        let mut kinds = Vec::new();
        let mut at = 8;
        while at < file.len() {
            kinds.push(String::from_utf8_lossy(&file[at..at + 4]).into_owned());
            at += u32::from_be_bytes(file[at + 4..at + 8].try_into().unwrap()) as usize;
        }
        assert_eq!(kinds, ["icp6", "ic07", "ic08", "ic09"]);
    }
}
