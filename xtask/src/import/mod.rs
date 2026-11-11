//! `cargo xtask import-assets`: read the upstream sprite, font, title and
//! icon tables as data and turn them into the committed files in `assets/`
//! (design D2, D7).
//!
//! Upstream C is never compiled or run: its source files are read as text by
//! the table reader in [`ctable`], which understands only array and pointer
//! table initialisers.

pub(crate) mod ctable;
#[cfg(test)]
mod fixture;
pub(crate) mod font;
pub(crate) mod icon;
pub(crate) mod index;
pub(crate) mod manifest;
pub(crate) mod sprites;
pub(crate) mod title;

use std::fmt;
use std::path::{Path, PathBuf};

use clap::Args;

use ctable::TableFile;

/// Arguments of `cargo xtask import-assets`.
#[derive(Debug, Args)]
pub(crate) struct ImportArgs {
    /// Checkout of upstream sobomax/digger at the pinned commit.
    #[arg(long, value_name = "DIR")]
    pub(crate) upstream: PathBuf,

    /// Directory the asset files are written to [default: assets/ in the
    /// workspace root].
    #[arg(long, value_name = "DIR")]
    pub(crate) out: Option<PathBuf>,

    /// Compare the import with the files in the output directory instead of
    /// writing; exit non-zero listing every file that differs.
    #[arg(long)]
    pub(crate) check: bool,
}

/// The upstream source file holding the VGA sprite arrays and `vgatable`.
pub(crate) const SPRITE_FILE: &str = "vgagrafx.c";
/// The upstream source file holding the glyph arrays and `ascii2vga`.
pub(crate) const FONT_FILE: &str = "alpha.c";
/// The upstream source file holding the compressed title picture.
pub(crate) const TITLE_FILE: &str = "title_gz.c";
/// The upstream source file holding the window icon.
pub(crate) const ICON_FILE: &str = "icon.c";

/// Why `cargo xtask import-assets` failed (design D7). Every variant names
/// the file it concerns; those about table contents also name the table.
#[derive(Debug)]
pub(crate) enum ImportError {
    /// A required upstream source file does not exist.
    MissingFile { file: PathBuf },
    /// A source file does not define a table the import needs.
    MissingTable { file: String, table: String },
    /// The text of a table could not be read.
    Syntax {
        file: String,
        line: usize,
        message: String,
    },
    /// A pointer table element or an array size names something the file
    /// does not define.
    UnknownSymbol {
        file: String,
        line: usize,
        table: String,
        symbol: String,
    },
    /// An array has the wrong number of bytes for its use.
    Length {
        file: String,
        table: String,
        expected: usize,
        got: usize,
    },
    /// A table has the wrong number of entries.
    Count {
        file: String,
        table: String,
        expected: usize,
        got: usize,
    },
    /// The title stream does not inflate to the expected picture.
    TitleInflate {
        file: String,
        table: String,
        message: String,
    },
    /// Reading or writing a file failed.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// `--check` found the output directory different from the import.
    Drift {
        dir: PathBuf,
        changed: Vec<String>,
        missing: Vec<String>,
        extra: Vec<String>,
    },
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFile { file } => write!(
                f,
                "{}: file not found (is --upstream a checkout of sobomax/digger?)",
                file.display()
            ),
            Self::MissingTable { file, table } => {
                write!(f, "{file}: table `{table}` is not defined")
            }
            Self::Syntax {
                file,
                line,
                message,
            } => write!(f, "{file}:{line}: {message}"),
            Self::UnknownSymbol {
                file,
                line,
                table,
                symbol,
            } => write!(
                f,
                "{file}:{line}: table `{table}` refers to unknown `{symbol}`"
            ),
            Self::Length {
                file,
                table,
                expected,
                got,
            } => write!(
                f,
                "{file}: table `{table}` has {got} bytes, expected {expected}"
            ),
            Self::Count {
                file,
                table,
                expected,
                got,
            } => write!(
                f,
                "{file}: table `{table}` has {got} entries, expected {expected}"
            ),
            Self::TitleInflate {
                file,
                table,
                message,
            } => write!(f, "{file}: table `{table}`: {message}"),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Drift {
                dir,
                changed,
                missing,
                extra,
            } => {
                write!(f, "{} differs from the import:", dir.display())?;
                for name in changed {
                    write!(f, "\n  changed: {name}")?;
                }
                for name in missing {
                    write!(f, "\n  missing: {name}")?;
                }
                for name in extra {
                    write!(f, "\n  extra:   {name}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ImportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Reads and parses one upstream source file.
pub(crate) fn read_source(upstream: &Path, name: &str) -> Result<TableFile, ImportError> {
    let path = upstream.join(name);
    if !path.is_file() {
        return Err(ImportError::MissingFile { file: path });
    }
    let text = std::fs::read(&path).map_err(|source| ImportError::Io {
        path: path.clone(),
        source,
    })?;
    ctable::parse(name, &text)
}

/// The default output directory: `assets/` in the workspace root.
fn default_out_dir() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap_or(manifest_dir).join("assets")
}

/// Output file names in `--out`.
pub(crate) const SPRITES_OUT: &str = "sprites.bin";
pub(crate) const FONT_OUT: &str = "font.bin";
pub(crate) const TITLE_OUT: &str = "title.zz";
pub(crate) const ICON_OUT: &str = "icon.rgba";
pub(crate) const INDEX_OUT: &str = "index.rs";
pub(crate) const MANIFEST_OUT: &str = "manifest.txt";

/// Files in the output directory that are maintained by hand, not by the
/// import: `--check` does not report them as extra.
pub(crate) const HAND_WRITTEN: [&str; 1] = ["README.md"];

/// One file the import produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutputFile {
    pub(crate) name: &'static str,
    pub(crate) bytes: Vec<u8>,
}

/// The complete, validated result of an import, held in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Import {
    /// Every output file; the manifest comes last.
    pub(crate) files: Vec<OutputFile>,
    /// CRC-32 of the inflated title picture.
    pub(crate) title_crc32: u32,
}

/// Reads and validates every upstream table and builds all outputs in
/// memory. Nothing is written.
///
/// Only the VGA sources are read: `cgagrafx.c` is never opened, and the CGA
/// tables in `alpha.c` are never looked up (spec "CGA data is not imported").
pub(crate) fn build(upstream: &Path) -> Result<Import, ImportError> {
    let sprites = sprites::import(
        &read_source(upstream, SPRITE_FILE)?,
        &sprites::upstream_sizes(),
    )?;
    let font = font::import(&read_source(upstream, FONT_FILE)?)?;
    let title = title::import(&read_source(upstream, TITLE_FILE)?)?;
    let icon = icon::import(&read_source(upstream, ICON_FILE)?)?;
    let index = index::render(&sprites.entries, &font.glyph_index);

    let mut files = vec![
        OutputFile {
            name: SPRITES_OUT,
            bytes: sprites.blob,
        },
        OutputFile {
            name: FONT_OUT,
            bytes: font.blob,
        },
        OutputFile {
            name: TITLE_OUT,
            bytes: title.stream,
        },
        OutputFile {
            name: ICON_OUT,
            bytes: icon,
        },
        OutputFile {
            name: INDEX_OUT,
            bytes: index.into_bytes(),
        },
    ];
    let manifest = manifest::render(files.iter().map(|file| (file.name, file.bytes.as_slice())));
    files.push(OutputFile {
        name: MANIFEST_OUT,
        bytes: manifest.into_bytes(),
    });
    Ok(Import {
        files,
        title_crc32: title.picture_crc32,
    })
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> ImportError + '_ {
    move |source| ImportError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Writes every file of `import` into `out`, each through a temporary file
/// in the same directory and a rename, the manifest last.
pub(crate) fn write(import: &Import, out: &Path) -> Result<(), ImportError> {
    std::fs::create_dir_all(out).map_err(io_error(out))?;
    for file in &import.files {
        let target = out.join(file.name);
        let temporary = out.join(format!(".{}.tmp", file.name));
        let written = std::fs::write(&temporary, &file.bytes)
            .map_err(io_error(&temporary))
            .and_then(|()| std::fs::rename(&temporary, &target).map_err(io_error(&target)));
        if written.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        written?;
    }
    Ok(())
}

/// Compares `import` with the files in `out` without modifying anything.
pub(crate) fn check(import: &Import, out: &Path) -> Result<(), ImportError> {
    let mut changed = Vec::new();
    let mut missing = Vec::new();
    for file in &import.files {
        let path = out.join(file.name);
        match std::fs::read(&path) {
            Ok(bytes) if bytes == file.bytes => {}
            Ok(_) => changed.push(file.name.to_owned()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(file.name.to_owned());
            }
            Err(source) => return Err(ImportError::Io { path, source }),
        }
    }
    let mut extra = Vec::new();
    match std::fs::read_dir(out) {
        Ok(entries) => {
            for entry in entries {
                let name = entry.map_err(io_error(out))?.file_name();
                let name = name.to_string_lossy();
                let known = import.files.iter().any(|file| file.name == name)
                    || HAND_WRITTEN.contains(&name.as_ref());
                if !known {
                    extra.push(name.into_owned());
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(ImportError::Io {
                path: out.to_owned(),
                source,
            });
        }
    }
    extra.sort();
    if changed.is_empty() && missing.is_empty() && extra.is_empty() {
        Ok(())
    } else {
        Err(ImportError::Drift {
            dir: out.to_owned(),
            changed,
            missing,
            extra,
        })
    }
}

/// Runs the import: validate everything, then write or check.
pub(crate) fn run(args: &ImportArgs) -> Result<(), ImportError> {
    let out = args.out.clone().unwrap_or_else(default_out_dir);
    let import = build(&args.upstream)?;
    eprintln!(
        "import-assets: title picture: CRC-32 of the {} inflated bytes is 0x{:08X}",
        title::PICTURE_LEN,
        import.title_crc32
    );
    if args.check {
        check(&import, &out)?;
        eprintln!(
            "import-assets: {} matches the import ({} files)",
            out.display(),
            import.files.len()
        );
    } else {
        write(&import, &out)?;
        for file in &import.files {
            eprintln!(
                "import-assets: wrote {} ({} bytes)",
                out.join(file.name).display(),
                file.bytes.len()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;

    use super::{ImportError, read_source};
    use crate::{Cli, Command};

    #[test]
    fn import_assets_parses_its_three_arguments() {
        let cli = Cli::try_parse_from([
            "cargo xtask",
            "import-assets",
            "--upstream",
            "up",
            "--out",
            "o",
            "--check",
        ]);
        let Ok(Cli {
            command: Command::ImportAssets(args),
        }) = cli
        else {
            unreachable!("arguments should parse: {cli:?}");
        };
        assert_eq!(args.upstream, PathBuf::from("up"));
        assert_eq!(args.out, Some(PathBuf::from("o")));
        assert!(args.check);
    }

    #[test]
    fn import_assets_defaults() {
        let cli = Cli::try_parse_from(["cargo xtask", "import-assets", "--upstream", "up"]);
        assert!(
            matches!(&cli, Ok(Cli { command: Command::ImportAssets(args) })
                if args.out.is_none() && !args.check),
            "{cli:?}"
        );
        assert!(super::default_out_dir().ends_with("assets"));
    }

    #[test]
    fn import_assets_requires_upstream() {
        let error = Cli::try_parse_from(["cargo xtask", "import-assets"]).err();
        let message = error.as_ref().map(ToString::to_string).unwrap_or_default();
        assert!(message.contains("--upstream"), "{message}");
        assert!(message.contains("Usage"), "{message}");
        assert_eq!(error.map(|e| e.exit_code()), Some(2));
    }

    #[test]
    fn missing_source_file_is_named() {
        let dir = std::env::temp_dir().join("digger-xtask-missing-upstream");
        let error = read_source(&dir, super::SPRITE_FILE).err();
        assert!(
            matches!(&error, Some(ImportError::MissingFile { file }) if file.ends_with("vgagrafx.c")),
            "{error:?}"
        );
        let message = error.map(|e| e.to_string()).unwrap_or_default();
        assert!(message.contains("vgagrafx.c"), "{message}");
    }

    // --- The whole import over the fixture checkout -----------------------

    use std::error::Error;
    use std::path::Path;

    use super::fixture::{TempDir, snapshot, upstream};
    use super::{
        FONT_FILE, HAND_WRITTEN, Import, MANIFEST_OUT, SPRITE_FILE, build, check, font, manifest,
        write,
    };

    fn file<'a>(import: &'a Import, name: &str) -> Option<&'a [u8]> {
        import
            .files
            .iter()
            .find(|file| file.name == name)
            .map(|file| file.bytes.as_slice())
    }

    #[test]
    fn fixture_import_produces_every_file() -> Result<(), Box<dyn Error>> {
        let checkout = upstream("full")?;
        let import = build(checkout.path())?;
        let names: Vec<&str> = import.files.iter().map(|file| file.name).collect();
        assert_eq!(
            names,
            [
                "sprites.bin",
                "font.bin",
                "title.zz",
                "icon.rgba",
                "index.rs",
                "manifest.txt"
            ]
        );
        assert_eq!(file(&import, "font.bin").map(<[u8]>::len), Some(23_040));
        assert_eq!(file(&import, "title.zz").map(<[u8]>::len), Some(2772));
        assert_eq!(file(&import, "icon.rgba").map(<[u8]>::len), Some(16_384));
        let index = String::from_utf8(file(&import, "index.rs").unwrap_or_default().to_vec())?;
        assert!(
            index.contains("SPRITE_INDEX: [SpriteEntry; 120]"),
            "{index}"
        );
        assert!(index.contains("GLYPH_INDEX: [Option<u8>; 95]"), "{index}");
        Ok(())
    }

    #[test]
    fn manifest_lists_every_output() -> Result<(), Box<dyn Error>> {
        let checkout = upstream("manifest")?;
        let import = build(checkout.path())?;
        let text = std::str::from_utf8(file(&import, MANIFEST_OUT).unwrap_or_default())?;
        assert!(text.contains(manifest::UPSTREAM_COMMIT), "{text}");
        let entries = manifest::parse(text).ok_or("malformed manifest")?;
        let mut expected: Vec<manifest::Entry> = import
            .files
            .iter()
            .filter(|file| file.name != MANIFEST_OUT)
            .map(|file| manifest::Entry::of(file.name, &file.bytes))
            .collect();
        expected.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(entries, expected);
        assert_eq!(entries.len(), import.files.len() - 1);
        Ok(())
    }

    #[test]
    fn import_is_deterministic() -> Result<(), Box<dyn Error>> {
        let checkout = upstream("deterministic")?;
        let (first, second) = (TempDir::new("det-a")?, TempDir::new("det-b")?);
        write(&build(checkout.path())?, first.path())?;
        write(&build(checkout.path())?, second.path())?;
        let files = snapshot(first.path())?;
        assert_eq!(files.len(), 6);
        assert_eq!(files, snapshot(second.path())?);
        // Writing again over the first output changes nothing either.
        write(&build(checkout.path())?, first.path())?;
        assert_eq!(files, snapshot(first.path())?);
        Ok(())
    }

    /// Fills `dir` with stand-ins for committed assets.
    fn populate(dir: &Path) -> std::io::Result<()> {
        std::fs::write(dir.join("sprites.bin"), b"old sprites")?;
        std::fs::write(dir.join("font.bin"), b"old font")?;
        std::fs::write(dir.join("manifest.txt"), b"old manifest")?;
        std::fs::write(dir.join("README.md"), b"hand written")
    }

    #[test]
    fn failed_import_leaves_out_dir_untouched() -> Result<(), Box<dyn Error>> {
        let out = TempDir::new("failed-out")?;
        populate(out.path())?;
        let before = snapshot(out.path())?;

        // A wrong checkout: the sprite source is missing.
        let empty = TempDir::new("failed-empty")?;
        let args = super::ImportArgs {
            upstream: empty.path().to_owned(),
            out: Some(out.path().to_owned()),
            check: false,
        };
        assert!(matches!(
            super::run(&args),
            Err(ImportError::MissingFile { file }) if file.ends_with(SPRITE_FILE)
        ));
        assert_eq!(snapshot(out.path())?, before);

        // A late failure: everything is valid except one glyph, and the font
        // is read after the sprites.
        let checkout = upstream("failed-glyph")?;
        let alpha = checkout.path().join(FONT_FILE);
        let text = std::fs::read_to_string(&alpha)?;
        let bad = text.replacen(
            "static const uint8_t vgaletQ[]={\n  0x",
            "static const uint8_t vgaletQ[]={\n  0x00,0x",
            1,
        );
        assert_ne!(bad, text);
        std::fs::write(&alpha, bad)?;
        let args = super::ImportArgs {
            upstream: checkout.path().to_owned(),
            ..args
        };
        let error = super::run(&args).err();
        assert!(
            matches!(&error, Some(ImportError::Length { table, expected: font::GLYPH_BYTES, .. })
                if table == "vgaletQ"),
            "{error:?}"
        );
        assert_eq!(snapshot(out.path())?, before);

        // A missing output directory is not created by a failed import.
        let absent = out.path().join("absent");
        let args = super::ImportArgs {
            out: Some(absent.clone()),
            ..args
        };
        assert!(super::run(&args).is_err());
        assert!(!absent.exists());
        Ok(())
    }

    #[test]
    fn check_passes_on_identical_output() -> Result<(), Box<dyn Error>> {
        let checkout = upstream("check-ok")?;
        let out = TempDir::new("check-ok-out")?;
        let import = build(checkout.path())?;
        write(&import, out.path())?;
        // A hand-written README is not an extra file.
        std::fs::write(out.path().join(HAND_WRITTEN[0]), b"assets")?;
        let before = snapshot(out.path())?;
        let args = super::ImportArgs {
            upstream: checkout.path().to_owned(),
            out: Some(out.path().to_owned()),
            check: true,
        };
        super::run(&args)?;
        check(&import, out.path())?;
        assert_eq!(snapshot(out.path())?, before);
        Ok(())
    }

    #[test]
    fn check_reports_changed_file() -> Result<(), Box<dyn Error>> {
        let checkout = upstream("check-drift")?;
        let out = TempDir::new("check-drift-out")?;
        let import = build(checkout.path())?;
        write(&import, out.path())?;
        let font = out.path().join("font.bin");
        let mut altered = std::fs::read(&font)?;
        if let Some(byte) = altered.get_mut(100) {
            *byte ^= 1;
        }
        std::fs::write(&font, &altered)?;
        std::fs::remove_file(out.path().join("icon.rgba"))?;
        std::fs::write(out.path().join("stray.bin"), b"?")?;
        let before = snapshot(out.path())?;

        let args = super::ImportArgs {
            upstream: checkout.path().to_owned(),
            out: Some(out.path().to_owned()),
            check: true,
        };
        let error = super::run(&args).err();
        assert!(
            matches!(&error, Some(ImportError::Drift { changed, missing, extra, .. })
                if changed == &["font.bin"] && missing == &["icon.rgba"] && extra == &["stray.bin"]),
            "{error:?}"
        );
        let message = error.map(|e| e.to_string()).unwrap_or_default();
        assert!(message.contains("changed: font.bin"), "{message}");
        assert!(message.contains("missing: icon.rgba"), "{message}");
        assert!(message.contains("stray.bin"), "{message}");
        // Nothing was modified, the altered font included.
        assert_eq!(snapshot(out.path())?, before);
        assert_eq!(std::fs::read(&font)?, altered);

        // Checking an empty directory lists every file as missing.
        let empty = TempDir::new("check-empty")?;
        let error = check(&import, empty.path()).err();
        assert!(
            matches!(&error, Some(ImportError::Drift { missing, .. }) if missing.len() == 6),
            "{error:?}"
        );
        Ok(())
    }

    #[test]
    fn cga_tables_are_ignored() -> Result<(), Box<dyn Error>> {
        let clean = upstream("cga-clean")?;
        let expected = build(clean.path())?;

        let checkout = upstream("cga")?;
        // A CGA sprite file that is not even readable as a table file ...
        std::fs::write(
            checkout.path().join("cgagrafx.c"),
            "const uint8_t *cgatable[]={ cgamissing, \"unterminated",
        )?;
        // ... and CGA font tables in alpha.c whose contents are invalid for
        // any import: a 7-byte glyph and a 3-entry character table.
        let alpha = checkout.path().join(FONT_FILE);
        let text = std::fs::read_to_string(&alpha)?;
        let start = text
            .find("static const uint8_t cgaletA")
            .ok_or("no cgaletA")?;
        let end = text
            .find("static const uint8_t vgaletA")
            .ok_or("no vgaletA")?;
        let mut bad = text.clone();
        bad.replace_range(
            start..end,
            "static const uint8_t cgaletA[]={1,2,3,4,5,6,7};\n",
        );
        let start = bad
            .find("const uint8_t * const ascii2cga")
            .ok_or("no ascii2cga")?;
        let end = bad
            .find("const uint8_t * const ascii2vga")
            .ok_or("no ascii2vga")?;
        bad.replace_range(
            start..end,
            "const uint8_t * const ascii2cga[]={cgaletA,0,cgaletA};\n",
        );
        std::fs::write(&alpha, bad)?;

        let import = build(checkout.path())?;
        assert_eq!(import, expected);
        assert!(import.files.iter().all(|file| !file.name.contains("cga")));
        let text = std::str::from_utf8(file(&import, MANIFEST_OUT).unwrap_or_default())?;
        assert!(!text.to_lowercase().contains("cga"), "{text}");
        Ok(())
    }
}
