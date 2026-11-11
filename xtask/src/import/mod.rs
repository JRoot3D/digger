//! `cargo xtask import-assets`: read the upstream sprite, font, title and
//! icon tables as data and turn them into the committed files in `assets/`
//! (design D2, D7).
//!
//! Upstream C is never compiled or run: its source files are read as text by
//! the table reader in [`ctable`], which understands only array and pointer
//! table initialisers.

pub(crate) mod ctable;

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
    #[expect(dead_code, reason = "raised by the asset import of task group 2")]
    Length {
        file: String,
        table: String,
        expected: usize,
        got: usize,
    },
    /// A table has the wrong number of entries.
    #[expect(dead_code, reason = "raised by the asset import of task group 2")]
    Count {
        file: String,
        table: String,
        expected: usize,
        got: usize,
    },
    /// The title stream does not inflate to the expected picture.
    #[expect(dead_code, reason = "raised by the asset import of task group 2")]
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

fn report(source: &TableFile, table: &str, entries: usize) {
    eprintln!(
        "import-assets: {}: {} tables read, `{table}` has {entries} entries",
        source.file(),
        source.tables().count()
    );
}

/// Runs the import.
///
/// Task group 1 provides the reader only: every upstream table the import
/// needs is read and checked for presence, and nothing is written yet.
pub(crate) fn run(args: &ImportArgs) -> Result<(), ImportError> {
    let out = args.out.clone().unwrap_or_else(default_out_dir);
    // The CGA files (`cgagrafx.c`, and the CGA half of `alpha.c`) are never
    // asked for.
    let sprites = read_source(&args.upstream, SPRITE_FILE)?;
    report(&sprites, "vgatable", sprites.pointers("vgatable")?.len());
    let font = read_source(&args.upstream, FONT_FILE)?;
    report(&font, "ascii2vga", font.pointers("ascii2vga")?.len());
    let title = read_source(&args.upstream, TITLE_FILE)?;
    report(&title, "title_gz", title.bytes("title_gz")?.len());
    let icon = read_source(&args.upstream, ICON_FILE)?;
    report(&icon, "Icon", icon.words("Icon")?.len());
    eprintln!(
        "import-assets: tables read; writing or checking {} is not implemented yet",
        out.display()
    );
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
}
