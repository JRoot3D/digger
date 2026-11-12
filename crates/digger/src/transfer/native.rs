//! The desktop's transfer: recordings are written into the `recordings`
//! folder of the data directory (design D11 of `add-desktop-app`).

use std::fs;
use std::io::{self, Write as _};
use std::path::PathBuf;

use super::{FileTransfer, TransferError};

/// How many numbered names are tried before giving up.
const MAX_SUFFIX: u32 = 9999;

/// Writes exported recordings into a folder.
#[derive(Debug, Clone)]
pub(crate) struct NativeFileTransfer {
    dir: PathBuf,
}

impl NativeFileTransfer {
    /// A transfer into `dir`, created when the first recording is saved.
    pub(crate) const fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Writes `bytes` under `name` in the folder, creating it if needed. A
    /// name that is taken gets `-2`, `-3`, … before the extension; an
    /// existing file is never overwritten. Returns the file written.
    fn write_new(&self, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
        fs::create_dir_all(&self.dir)?;
        let (stem, ext) = name
            .rsplit_once('.')
            .map_or((name, None), |(stem, ext)| (stem, Some(ext)));
        let numbered = |n: u32| {
            let stem = if n == 1 {
                stem.to_owned()
            } else {
                format!("{stem}-{n}")
            };
            match ext {
                Some(ext) => format!("{stem}.{ext}"),
                None => stem,
            }
        };
        for n in 1..=MAX_SUFFIX {
            let path = self.dir.join(numbered(n));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    file.write_all(bytes)?;
                    return Ok(path);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "every name from {} to {} is taken",
                numbered(1),
                numbered(MAX_SUFFIX)
            ),
        ))
    }
}

impl FileTransfer for NativeFileTransfer {
    fn export(&mut self, name: &str, bytes: &[u8]) -> Result<(), TransferError> {
        self.write_new(name, bytes)
            .map(drop)
            .map_err(|source| TransferError::Io {
                path: self.dir.clone(),
                source,
            })
    }
}
