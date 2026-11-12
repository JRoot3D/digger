//! Deterministic release archives (add-web-build-and-releases, design
//! decision 14): `tar.gz` and `zip` writers whose output depends only on
//! the files put in, and a SHA-256 helper.
//!
//! Determinism comes from the [`Archive`] model rather than from the file
//! system: entries are sorted by path, every entry carries the same
//! modification time (`SOURCE_DATE_EPOCH`), owner and group are 0 with
//! empty names, modes are fixed (0755 for directories and programs, 0644
//! for everything else), and the gzip header has no time stamp, no file
//! name and the "unknown" operating system. Only pure-Rust crates are used
//! (`tar`, `zip` with deflate through `flate2`, `sha2`), as `deny.toml`
//! requires.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::Path;

use flate2::Compression;
use flate2::GzBuilder;
use sha2::{Digest, Sha256};

/// The mode of directories and programs.
pub(crate) const MODE_EXECUTABLE: u32 = 0o755;
/// The mode of every other file.
pub(crate) const MODE_FILE: u32 = 0o644;

/// What one archive entry is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Entry {
    /// A directory.
    Dir,
    /// A file and whether it is a program.
    File {
        /// The file's content.
        data: Vec<u8>,
        /// Whether it gets the program mode 0755.
        executable: bool,
    },
}

impl Entry {
    /// The entry's fixed mode.
    pub(crate) const fn mode(&self) -> u32 {
        match self {
            Self::Dir
            | Self::File {
                executable: true, ..
            } => MODE_EXECUTABLE,
            Self::File { .. } => MODE_FILE,
        }
    }
}

/// The content of an archive: entries by `/`-separated relative path, kept
/// sorted, and the one modification time of all entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Archive {
    entries: BTreeMap<String, Entry>,
    /// Seconds since 1970-01-01 00:00 UTC.
    mtime: u64,
}

impl Archive {
    /// An empty archive whose entries are dated `mtime` (seconds since the
    /// Unix epoch).
    pub(crate) const fn new(mtime: u64) -> Self {
        Self {
            entries: BTreeMap::new(),
            mtime,
        }
    }

    /// Adds `entry` at `path`, and every parent directory of it.
    pub(crate) fn add(&mut self, path: &str, entry: Entry) {
        let path = path.trim_matches('/');
        let mut parent = String::new();
        let mut segments = path.split('/').peekable();
        while let Some(segment) = segments.next() {
            if segments.peek().is_none() {
                break;
            }
            if !parent.is_empty() {
                parent.push('/');
            }
            parent.push_str(segment);
            self.entries.entry(parent.clone()).or_insert(Entry::Dir);
        }
        self.entries.insert(path.to_owned(), entry);
    }

    /// Adds the directory `dir` and everything below it under `prefix`;
    /// files whose path relative to `dir` is in `programs` get mode 0755.
    pub(crate) fn add_tree(
        &mut self,
        dir: &Path,
        prefix: &str,
        programs: &[&str],
    ) -> Result<(), String> {
        self.add(prefix, Entry::Dir);
        self.add_tree_below(dir, prefix, "", programs)
    }

    fn add_tree_below(
        &mut self,
        dir: &Path,
        prefix: &str,
        relative: &str,
        programs: &[&str],
    ) -> Result<(), String> {
        let listing =
            fs::read_dir(dir).map_err(|error| format!("cannot list {}: {error}", dir.display()))?;
        for item in listing {
            let item = item.map_err(|error| format!("cannot list {}: {error}", dir.display()))?;
            let name = item.file_name().to_string_lossy().into_owned();
            let inner = if relative.is_empty() {
                name.clone()
            } else {
                format!("{relative}/{name}")
            };
            let path = item.path();
            let kind = item
                .file_type()
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            if kind.is_dir() {
                self.add(&format!("{prefix}/{inner}"), Entry::Dir);
                self.add_tree_below(&path, prefix, &inner, programs)?;
            } else if kind.is_file() {
                let data = fs::read(&path)
                    .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
                let executable = programs.contains(&inner.as_str());
                self.add(
                    &format!("{prefix}/{inner}"),
                    Entry::File { data, executable },
                );
            } else {
                return Err(format!(
                    "{} is neither a file nor a directory",
                    path.display()
                ));
            }
        }
        Ok(())
    }

    /// The entries in archive order.
    pub(crate) fn entries(&self) -> impl Iterator<Item = (&str, &Entry)> {
        self.entries
            .iter()
            .map(|(path, entry)| (path.as_str(), entry))
    }

    /// The archive as a gzip-compressed tar file.
    pub(crate) fn to_tar_gz(&self) -> Result<Vec<u8>, String> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, entry) in self.entries() {
            let mut header = tar::Header::new_gnu();
            header.set_mode(entry.mode());
            header.set_uid(0);
            header.set_gid(0);
            header.set_mtime(self.mtime);
            let fail =
                |error: std::io::Error| format!("cannot add {path} to the tar file: {error}");
            match entry {
                Entry::Dir => {
                    header.set_entry_type(tar::EntryType::Directory);
                    header.set_size(0);
                    builder
                        .append_data(&mut header, format!("{path}/"), std::io::empty())
                        .map_err(fail)?;
                }
                Entry::File { data, .. } => {
                    header.set_entry_type(tar::EntryType::Regular);
                    header.set_size(u64::try_from(data.len()).unwrap_or(u64::MAX));
                    builder
                        .append_data(&mut header, path, data.as_slice())
                        .map_err(fail)?;
                }
            }
        }
        let tar = builder
            .into_inner()
            .map_err(|error| format!("cannot finish the tar file: {error}"))?;
        // No time stamp, no file name, operating system 255 ("unknown").
        let mut encoder = GzBuilder::new()
            .mtime(0)
            .operating_system(255)
            .write(Vec::new(), Compression::best());
        encoder
            .write_all(&tar)
            .and_then(|()| encoder.finish())
            .map_err(|error| format!("cannot compress the tar file: {error}"))
    }

    /// The archive as a zip file, every file compressed with deflate at
    /// level 9.
    pub(crate) fn to_zip(&self) -> Result<Vec<u8>, String> {
        let time = dos_time(self.mtime)?;
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (path, entry) in self.entries() {
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .compression_level(Some(9))
                .last_modified_time(time)
                // Unix attributes on every host: the zip crate marks
                // entries written on Windows as DOS by default, which drops
                // the modes and makes the bytes depend on the build host.
                .system(zip::System::Unix)
                .unix_permissions(entry.mode());
            let fail = |error: &dyn std::fmt::Display| {
                format!("cannot add {path} to the zip file: {error}")
            };
            match entry {
                Entry::Dir => writer
                    .add_directory(format!("{path}/"), options)
                    .map_err(|error| fail(&error))?,
                Entry::File { data, .. } => {
                    writer
                        .start_file(path, options)
                        .map_err(|error| fail(&error))?;
                    writer.write_all(data).map_err(|error| fail(&error))?;
                }
            }
        }
        writer
            .finish()
            .map(Cursor::into_inner)
            .map_err(|error| format!("cannot finish the zip file: {error}"))
    }
}

/// The zip (MS-DOS) date and time of `seconds` since the Unix epoch, in
/// UTC; times before 1980, which the format cannot hold, become
/// 1980-01-01 00:00.
pub(crate) fn dos_time(seconds: u64) -> Result<zip::DateTime, String> {
    let (year, month, day, hour, minute, second) = civil(seconds);
    if year < 1980 {
        return zip::DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0)
            .map_err(|error| error.to_string());
    }
    let narrow = |value: u64| u8::try_from(value).unwrap_or(u8::MAX);
    zip::DateTime::from_date_and_time(
        u16::try_from(year).unwrap_or(u16::MAX),
        narrow(month),
        narrow(day),
        narrow(hour),
        narrow(minute),
        narrow(second),
    )
    .map_err(|error| format!("{seconds} cannot be written as a zip time: {error}"))
}

/// The UTC calendar date and time (year, month, day, hour, minute,
/// second) of `seconds` since 1970-01-01 00:00 UTC, in the proleptic
/// Gregorian calendar.
pub(crate) fn civil(seconds: u64) -> (u64, u64, u64, u64, u64, u64) {
    let days = seconds / 86_400;
    let rest = seconds % 86_400;
    // Days counted from 0000-03-01, so that the leap day ends each year.
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day, rest / 3600, rest % 3600 / 60, rest % 60)
}

/// The SHA-256 of `bytes` as 64 lower-case hexadecimal digits.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use std::io::Read;

    use super::{Archive, Entry, MODE_EXECUTABLE, MODE_FILE, civil, dos_time, sha256_hex};

    fn file(data: &str) -> Entry {
        Entry::File {
            data: data.as_bytes().to_vec(),
            executable: false,
        }
    }

    /// The same files, added in two different orders.
    fn samples() -> (Archive, Archive) {
        let mut one = Archive::new(1_700_000_000);
        one.add("digger-1.0/README.md", file("read me"));
        one.add(
            "digger-1.0/digger",
            Entry::File {
                data: vec![0x7f, b'E', b'L', b'F'],
                executable: true,
            },
        );
        one.add("digger-1.0/site/index.html", file("<!doctype html>"));
        let mut two = Archive::new(1_700_000_000);
        two.add("digger-1.0/site/index.html", file("<!doctype html>"));
        two.add(
            "digger-1.0/digger",
            Entry::File {
                data: vec![0x7f, b'E', b'L', b'F'],
                executable: true,
            },
        );
        two.add("digger-1.0/README.md", file("read me"));
        (one, two)
    }

    #[test]
    fn tar_same_input_same_bytes() {
        let (one, two) = samples();
        let first = one.to_tar_gz().unwrap();
        assert_eq!(first, one.to_tar_gz().unwrap());
        assert_eq!(first, two.to_tar_gz().unwrap());
        // Gzip header: no time stamp, no name, operating system 255.
        assert_eq!(&first[..10], &[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 2, 255]);
        // A different time gives different bytes, so the time is in there.
        let mut later = one.clone();
        later.mtime += 1;
        assert_ne!(first, later.to_tar_gz().unwrap());
    }

    #[test]
    fn zip_same_input_same_bytes() {
        let (one, two) = samples();
        let first = one.to_zip().unwrap();
        assert_eq!(first, one.to_zip().unwrap());
        assert_eq!(first, two.to_zip().unwrap());
        let mut later = one.clone();
        later.mtime += 3600;
        assert_ne!(first, later.to_zip().unwrap());
    }

    #[test]
    fn archive_entries_sorted_and_mode_fixed() {
        let (one, _) = samples();
        let expected = [
            ("digger-1.0", MODE_EXECUTABLE),
            ("digger-1.0/README.md", MODE_FILE),
            ("digger-1.0/digger", MODE_EXECUTABLE),
            ("digger-1.0/site", MODE_EXECUTABLE),
            ("digger-1.0/site/index.html", MODE_FILE),
        ];

        // The tar file, read back.
        let packed = one.to_tar_gz().unwrap();
        let mut tar_bytes = Vec::new();
        flate2::read::GzDecoder::new(packed.as_slice())
            .read_to_end(&mut tar_bytes)
            .unwrap();
        let mut reader = tar::Archive::new(tar_bytes.as_slice());
        let mut seen = Vec::new();
        for entry in reader.entries().unwrap() {
            let entry = entry.unwrap();
            let header = entry.header();
            assert_eq!(header.uid().unwrap(), 0);
            assert_eq!(header.gid().unwrap(), 0);
            assert_eq!(header.mtime().unwrap(), 1_700_000_000);
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            seen.push((
                path.trim_end_matches('/').to_owned(),
                header.mode().unwrap(),
            ));
        }
        let expected_owned: Vec<(String, u32)> = expected
            .iter()
            .map(|(path, mode)| ((*path).to_owned(), *mode))
            .collect();
        assert_eq!(seen, expected_owned);

        // The zip file, read back.
        let packed = one.to_zip().unwrap();
        let mut reader = zip::ZipArchive::new(std::io::Cursor::new(packed)).unwrap();
        let mut seen = Vec::new();
        for index in 0..reader.len() {
            let entry = reader.by_index(index).unwrap();
            let time = entry.last_modified().unwrap();
            assert_eq!(
                (
                    time.year(),
                    time.month(),
                    time.day(),
                    time.hour(),
                    time.minute()
                ),
                (2023, 11, 14, 22, 13)
            );
            seen.push((
                entry.name().unwrap().trim_end_matches('/').to_owned(),
                entry.unix_mode().unwrap() & 0o7777,
            ));
        }
        assert_eq!(seen, expected_owned);
    }

    #[test]
    fn calendar_dates() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(951_782_400), (2000, 2, 29, 0, 0, 0));
        assert_eq!(civil(1_700_000_000), (2023, 11, 14, 22, 13, 20));
        assert_eq!(civil(4_102_444_799), (2099, 12, 31, 23, 59, 59));
        // Before 1980 the zip time is its earliest value.
        let early = dos_time(0).unwrap();
        assert_eq!((early.year(), early.month(), early.day()), (1980, 1, 1));
    }

    #[test]
    fn sha256_of_known_input() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
