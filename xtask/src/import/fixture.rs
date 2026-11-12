//! Hand-made upstream-like source files for the importer's tests.
//!
//! Everything here is generated: no upstream data is copied into the tests.
//! The full fixture has the upstream shape (120 image numbers, 40 glyphs and
//! the 95-entry character table, a 2772-byte zlib stream inflating to 256000
//! bytes, 4096 icon words, eight 10×15 level layouts) with made-up contents.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::font::{GLYPH_BYTES, GLYPH_NAMES};
use super::music::{BONUS_NOTES, DIRGE_NOTES, MUSIC_FILE, NORMAL_NOTES};
use super::sprites::{ImageSize, upstream_size, upstream_sizes};
use super::title::{PICTURE_LEN, STREAM_LEN};
use super::{FONT_FILE, ICON_FILE, LEVEL_FILE, SPRITE_FILE, TITLE_FILE};

/// An 8-bit array definition in upstream style.
pub(crate) fn c_bytes(name: &str, values: &[u8]) -> String {
    let mut text = format!("static const uint8_t {name}[]={{\n");
    let rows: Vec<String> = values
        .chunks(16)
        .map(|row| {
            let cells: Vec<String> = row.iter().map(|v| format!("0x{v:02x}")).collect();
            format!("  {}", cells.join(","))
        })
        .collect();
    text += &rows.join(",\n");
    text += "};\n";
    text
}

/// A 32-bit array definition in the style of `icon.c`.
pub(crate) fn c_words(name: &str, values: &[u32]) -> String {
    let mut text = format!("#include <SDL.h>\n\nuint32_t {name}[] = {{\n");
    for row in values.chunks(8) {
        for value in row {
            let _ = write!(text, "0x{value:08x}, ");
        }
        text.push('\n');
    }
    text += "};\n";
    text
}

/// A made-up byte pattern, varying with `seed`, over the values of `set`.
fn pattern(seed: usize, len: usize, set: &[u8]) -> Vec<u8> {
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            set.get((state >> 16) % set.len().max(1))
                .copied()
                .unwrap_or(0)
        })
        .collect()
}

/// The contents of fixture glyph `glyph` (all zero for the space glyph).
pub(crate) fn glyph_bytes(glyph: usize) -> Vec<u8> {
    if glyph == GLYPH_NAMES.len() - 1 {
        return vec![0; GLYPH_BYTES];
    }
    pattern(glyph, GLYPH_BYTES, &[0, 10, 12, 15])
}

/// The glyph array the fixture's `ascii2vga` names for character `code`.
fn glyph_name(code: u8) -> Option<&'static str> {
    let glyph = match code {
        b'A'..=b'Z' => usize::from(code - b'A'),
        b'a'..=b'z' => usize::from(code - b'a'),
        b'0'..=b'9' => 26 + usize::from(code - b'0'),
        b'.' => 36,
        b':' => 37,
        b'_' => 38,
        b' ' => 39,
        _ => return None,
    };
    GLYPH_NAMES.get(glyph).copied()
}

/// A fixture `alpha.c` whose VGA glyphs hold `glyph_len` bytes each, with a
/// small CGA part in front as upstream has.
pub(crate) fn alpha_c(glyph_len: usize) -> String {
    let mut text = String::from("/* Digger fixture */\n\n#include \"def.h\"\n\n");
    text += &c_bytes("cgaletA", &[0xaa; 36]);
    for (glyph, name) in GLYPH_NAMES.iter().enumerate() {
        let mut bytes = glyph_bytes(glyph);
        bytes.resize(glyph_len, 0);
        text += &c_bytes(name, &bytes);
    }
    let cga: Vec<&str> = (32u8..127)
        .map(|code| if code == b'A' { "cgaletA" } else { "0" })
        .collect();
    let _ = writeln!(
        text,
        "\nconst uint8_t * const ascii2cga[0x5f]={{\n  {}}};",
        cga.join(",")
    );
    let vga: Vec<&str> = (32u8..127)
        .map(|code| glyph_name(code).unwrap_or("0"))
        .collect();
    let _ = writeln!(
        text,
        "const uint8_t * const ascii2vga[0x5f]={{\n  {}}};",
        vga.join(",")
    );
    text
}

/// The arrays a fixture image number uses, by the scheme of upstream: blank
/// and eraser entries share an all-zero block, background tiles use it as
/// their mask.
fn sprite_arrays(number: usize, size: ImageSize) -> (String, String) {
    let image = format!("vgaimg{}x{}", size.w, size.h);
    let mask = format!("vgamask{}x{}", size.w, size.h);
    match number {
        0 | 31 => ("vgazero480".into(), "vgaediggermask".into()),
        81 => (image, "vgaediggermask".into()),
        94..=101 => (image, "vgazero480".into()),
        102..=107 | 109 | 112 => ("vgazero480".into(), mask),
        _ => (image, mask),
    }
}

/// A fixture `vgagrafx.c` with the 240-entry index table.
pub(crate) fn vgagrafx_c() -> String {
    let mut text = String::from("/* Digger fixture */\n\n#include \"def.h\"\n\n");
    text += &c_bytes("vgazero480", &[0; 960]);
    text += &c_bytes("vgaediggermask", &[0; 960]);
    let mut sizes = upstream_sizes();
    sizes.sort();
    sizes.dedup();
    for (seed, size) in sizes.iter().enumerate() {
        let image = pattern(seed, size.bytes(), &[0xff, 0, 2, 4, 6, 10, 12, 14, 15]);
        text += &c_bytes(&format!("vgaimg{}x{}", size.w, size.h), &image);
        let mask = pattern(seed + 100, size.bytes(), &[0, 0xff]);
        text += &c_bytes(&format!("vgamask{}x{}", size.w, size.h), &mask);
    }
    text += "\nconst uint8_t *vgatable[]={\n";
    for number in 0..120 {
        if let Some(size) = upstream_size(number) {
            let (image, mask) = sprite_arrays(number, size);
            let _ = writeln!(text, "  {image}, {mask}, /* {number} */");
        }
    }
    text += "};\n";
    text
}

/// A fixture title picture of `len` bytes.
pub(crate) fn title_picture(len: usize) -> Vec<u8> {
    let mut picture = vec![0; len];
    for (offset, value) in picture.iter_mut().enumerate().take(1500) {
        *value = [0, 4, 7, 8, 15].get(offset % 5).copied().unwrap_or(0);
    }
    picture
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in bytes {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

/// A zlib stream of exactly 2772 bytes inflating to `picture`: a stored block
/// holding the first `m` bytes, then the rest compressed, with `m` chosen so
/// that the total length comes out right.
pub(crate) fn title_stream(picture: &[u8]) -> Option<Vec<u8>> {
    let build = |stored_len: usize| -> Option<Vec<u8>> {
        let (stored, rest) = picture.split_at_checked(stored_len)?;
        let len = u16::try_from(stored_len).ok()?;
        let mut stream = vec![0x78, 0x01, 0x00];
        stream.extend(len.to_le_bytes());
        stream.extend((!len).to_le_bytes());
        stream.extend(stored);
        stream.extend(miniz_oxide::deflate::compress_to_vec(rest, 6));
        stream.extend(adler32(picture).to_be_bytes());
        Some(stream)
    };
    let mut stored_len = 0;
    for _ in 0..32 {
        let len = build(stored_len)?.len();
        if len == STREAM_LEN {
            return build(stored_len);
        }
        stored_len = if len < STREAM_LEN {
            stored_len + (STREAM_LEN - len)
        } else {
            stored_len.checked_sub(len - STREAM_LEN)?
        };
    }
    (stored_len.saturating_sub(16)..stored_len + 16)
        .filter_map(build)
        .find(|stream| stream.len() == STREAM_LEN)
}

/// A fixture `title_gz.c`.
pub(crate) fn title_gz_c() -> Option<String> {
    let stream = title_stream(&title_picture(PICTURE_LEN))?;
    Some(format!(
        "#include <zlib.h>\n\n#include \"def.h\"\n#define CTITLELEN 2772\n#define UTITLELEN 256000\n\n{}\n\
         void gettitle(unsigned char *buf)\n{{\n\tuLongf uncomplen=UTITLELEN;\n\
         \tuncompress(buf, &uncomplen, title_gz, CTITLELEN);\n}}\n",
        c_bytes("title_gz", &stream).replace("title_gz[]", "title_gz[CTITLELEN]")
    ))
}

/// A fixture `icon.c`.
pub(crate) fn icon_c() -> String {
    let words: Vec<u32> = (0..4096u32)
        .map(|i| i.wrapping_mul(0x9E37_79B9) | 0xFF00_0000)
        .collect();
    c_words("Icon", &words)
}

/// Eight made-up 10×15 layouts over the layout alphabet.
pub(crate) fn fixture_layouts() -> Vec<Vec<String>> {
    (0..8)
        .map(|layout| {
            (0..10)
                .map(|row| {
                    let bytes = pattern(layout * 10 + row, 15, b" SHVBC");
                    String::from_utf8_lossy(&bytes).into_owned()
                })
                .collect()
        })
        .collect()
}

/// A fixture `game.c` whose `dgstate` initialiser holds `layouts` as its
/// `.leveldat` member, between other members as upstream has them.
pub(crate) fn game_c(layouts: &[Vec<String>]) -> String {
    let mut text = String::from(
        "/* Digger fixture */\n\n#include <stdint.h>\n#include \"game.h\"\n\n\
         struct gamestate dgstate = {\n  .nplayers = 1, .diggers = 1, .name = \"x\",\n  .leveldat = {",
    );
    let groups: Vec<String> = layouts
        .iter()
        .map(|rows| {
            let rows: Vec<String> = rows.iter().map(|row| format!("\"{row}\"")).collect();
            format!("{{{}}}", rows.join(",\n                "))
        })
        .collect();
    text += &groups.join(",\n               ");
    text += "}\n};\n\nvoid\ngame_tick(void)\n{\n  dgstate.ticks++;\n}\n";
    text
}

/// Made-up tune values (`divisor, duration` pairs and the end marker) for
/// the fixture's `sound_backend.c`, with the upstream shapes.
pub(crate) fn fixture_tune(table: &str) -> Vec<u16> {
    const DIVISORS: [u16; 6] = [0x11d1, 0xfdf, 0xe24, 0xd59, 0xbe4, 0x7d00];
    let tune = |seed: usize, notes: usize| -> Vec<u16> {
        let mut values: Vec<u16> = pattern(seed, notes, &[0, 1, 2, 3, 4, 5])
            .iter()
            .zip(pattern(seed + 1, notes, &[2, 4, 6]))
            .flat_map(|(&pick, units)| {
                let divisor = DIVISORS.get(usize::from(pick)).copied().unwrap_or(0x11d1);
                [divisor, u16::from(units)]
            })
            .collect();
        values.push(0x7d64);
        values
    };
    match table {
        "backgjingle" => tune(1, NORMAL_NOTES),
        "bonusjingle" => tune(3, BONUS_NOTES),
        "battlejingle" => tune(5, 37),
        _ => {
            // 12 notes lasting 50 units, then 12 rests.
            let mut values = vec![0x7d00, 6];
            for _ in 0..11 {
                values.extend([0xefb, 4]);
            }
            for _ in 0..DIRGE_NOTES - 12 {
                values.extend([0x7d00, 16]);
            }
            values.push(0x7d64);
            values
        }
    }
}

/// A fixture `sound_backend.c`: the four tune lists among code and other
/// initialised declarations, as upstream has them. Each `(table, values)` of
/// `overrides` replaces that list's values.
pub(crate) fn sound_backend_c(overrides: &[(&str, Vec<u16>)]) -> String {
    let mut text = String::from(
        "/* Digger fixture */\n\n#include \"def.h\"\n\n\
         static const int16_t newlevjingle[11]={0x8e8,0x712,0x5f2,0x7f0,0x6ac,0x54c,\n\
         0x7f0,0x6ac,0x54c,0x474,0x474};\n\
         static bool soundfireflag[FIREBALLS]={false,false},sff[FIREBALLS];\n\
         static int16_t soundfirevalue[FIREBALLS],soundfiren[FIREBALLS]={0,0};\n\n\
         void soundfire(int n)\n{\n  soundfirevalue[n]=500;\n  soundfireflag[n]=true;\n}\n\n",
    );
    for table in ["bonusjingle", "backgjingle", "dirge", "battlejingle"] {
        let values = overrides
            .iter()
            .find(|(name, _)| *name == table)
            .map_or_else(|| fixture_tune(table), |(_, values)| values.clone());
        let cells: Vec<String> = values.iter().map(|v| format!("0x{v:x}")).collect();
        let rows: Vec<String> = cells.chunks(16).map(|row| row.join(",")).collect();
        let _ = writeln!(
            text,
            "static const int16_t {table}[]={{\n  {}}};\n",
            rows.join(",\n  ")
        );
    }
    text += "void musicupdate(void)\n{\n  if (dirge[musicp]==0x7d64)\n    musicoff();\n}\n";
    text
}

/// A directory removed again when dropped.
#[derive(Debug)]
pub(crate) struct TempDir(PathBuf);

impl TempDir {
    /// A fresh, empty directory; `tag` keeps concurrent tests apart.
    pub(crate) fn new(tag: &str) -> std::io::Result<Self> {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("digger-xtask-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A complete fixture upstream checkout holding the six source files.
pub(crate) fn upstream(tag: &str) -> Result<TempDir, Box<dyn std::error::Error>> {
    let dir = TempDir::new(tag)?;
    std::fs::write(dir.path().join(SPRITE_FILE), vgagrafx_c())?;
    std::fs::write(dir.path().join(FONT_FILE), alpha_c(GLYPH_BYTES))?;
    std::fs::write(
        dir.path().join(TITLE_FILE),
        title_gz_c().ok_or("no fixture title stream")?,
    )?;
    std::fs::write(dir.path().join(ICON_FILE), icon_c())?;
    std::fs::write(dir.path().join(LEVEL_FILE), game_c(&fixture_layouts()))?;
    std::fs::write(dir.path().join(MUSIC_FILE), sound_backend_c(&[]))?;
    Ok(dir)
}

/// Every file of `dir` with its contents, by name.
pub(crate) fn snapshot(dir: &Path) -> std::io::Result<Vec<(String, Vec<u8>)>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        files.push((
            entry.file_name().to_string_lossy().into_owned(),
            std::fs::read(entry.path())?,
        ));
    }
    files.sort();
    Ok(files)
}
