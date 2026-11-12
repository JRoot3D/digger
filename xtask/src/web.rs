//! `cargo xtask web`: the browser build (add-web-build-and-releases, design
//! decisions 2 and 11).
//!
//! Builds the `digger` binary for `wasm32-unknown-unknown` with the
//! `web-release` profile, checks that the installed `wasm-bindgen` command
//! is exactly the version of the `wasm-bindgen` library in `Cargo.lock`,
//! generates the JavaScript glue with `wasm-bindgen --target web
//! --no-typescript`, assembles `dist/web/` from the generated files and the
//! hand-written page in `web/`, and checks the result against the download
//! size budget. With `--serve` it then serves `dist/web/` on `127.0.0.1`
//! with a small static server, which the browser tests also use.

use std::fmt;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Args;
use flate2::Compression;
use flate2::write::GzEncoder;

/// The uncompressed download budget: 2 MiB (`web-build` spec, download
/// size budget).
pub(crate) const MAX_UNCOMPRESSED: u64 = 2 * 1024 * 1024;
/// The budget with every file compressed by gzip at level 9: 1 MiB (same
/// requirement).
pub(crate) const MAX_GZIP: u64 = 1024 * 1024;

/// The browser target.
const TARGET: &str = "wasm32-unknown-unknown";
/// The profile of the browser build.
const PROFILE: &str = "web-release";
/// The hand-written page files, copied from `web/`.
const PAGE_FILES: [&str; 3] = ["index.html", "style.css", "boot.js"];

/// Arguments of `cargo xtask web`.
#[derive(Debug, Args)]
pub(crate) struct WebArgs {
    /// After building, serve dist/web/ on 127.0.0.1 until interrupted.
    #[arg(long)]
    serve: bool,
    /// The port to serve on.
    #[arg(long, default_value_t = 8000, requires = "serve")]
    port: u16,
    /// The URL path the site is served under, such as /games/digger/.
    #[arg(long, default_value = "/", requires = "serve")]
    prefix: String,
}

/// The workspace root.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// Runs `cargo xtask web`.
pub(crate) fn run(args: &WebArgs) -> Result<(), String> {
    let root = workspace_root();
    let dist = build(&root)?;
    if args.serve {
        serve(&dist, args.port, &Prefix::new(&args.prefix)?)?;
    }
    Ok(())
}

/// Builds the site into `dist/web/` under `root` and checks its size;
/// returns the directory.
pub(crate) fn build(root: &Path) -> Result<PathBuf, String> {
    let lock = fs::read_to_string(root.join("Cargo.lock"))
        .map_err(|error| format!("cannot read Cargo.lock: {error}"))?;
    let wanted = locked_version(&lock, "wasm-bindgen")
        .ok_or_else(|| String::from("Cargo.lock does not list wasm-bindgen"))?;
    let installed = Command::new("wasm-bindgen")
        .arg("--version")
        .output()
        .map_err(|error| {
            format!(
                "cannot run wasm-bindgen ({error}); install it with `{}`",
                install_command(&wanted)
            )
        })?;
    check_cli_version(&wanted, &String::from_utf8_lossy(&installed.stdout))?;

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    println!("xtask web: cargo build -p digger --target {TARGET} --profile {PROFILE} --locked");
    let status = Command::new(cargo)
        .current_dir(root)
        .args([
            "build",
            "-p",
            "digger",
            "--target",
            TARGET,
            "--profile",
            PROFILE,
            "--locked",
        ])
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err(String::from("the web build failed"));
    }
    let target_dir =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let wasm = target_dir.join(TARGET).join(PROFILE).join("digger.wasm");

    let dist = root.join("dist").join("web");
    if dist.exists() {
        fs::remove_dir_all(&dist)
            .map_err(|error| format!("cannot clear {}: {error}", dist.display()))?;
    }
    fs::create_dir_all(&dist)
        .map_err(|error| format!("cannot create {}: {error}", dist.display()))?;
    println!("xtask web: wasm-bindgen --target web --no-typescript");
    let status = Command::new("wasm-bindgen")
        .args(["--target", "web", "--no-typescript", "--out-name", "digger"])
        .arg("--out-dir")
        .arg(&dist)
        .arg(&wasm)
        .status()
        .map_err(|error| format!("cannot run wasm-bindgen: {error}"))?;
    if !status.success() {
        return Err(String::from("wasm-bindgen failed"));
    }
    for name in PAGE_FILES {
        let from = root.join("web").join(name);
        fs::copy(&from, dist.join(name))
            .map_err(|error| format!("cannot copy {}: {error}", from.display()))?;
    }

    let sizes = measure(&dist)?;
    for size in &sizes {
        println!(
            "xtask web: {:<20} {:>9} bytes, {:>9} bytes gzip -9",
            size.name, size.raw, size.gzip
        );
    }
    let totals = check_budget(&sizes).map_err(|error| error.to_string())?;
    println!("xtask web: {totals}");
    println!("xtask web: the site is in {}", dist.display());
    Ok(dist)
}

/// The version of `package` in the lock file's text, the first listed.
pub(crate) fn locked_version(lock: &str, package: &str) -> Option<String> {
    let name = format!("name = \"{package}\"");
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() == name {
            let version = lines.next()?.trim();
            return version
                .strip_prefix("version = \"")
                .and_then(|rest| rest.strip_suffix('"'))
                .map(str::to_owned);
        }
    }
    None
}

/// The command that installs the matching `wasm-bindgen`.
fn install_command(version: &str) -> String {
    format!("cargo install wasm-bindgen-cli --version {version} --locked")
}

/// Checks the output of `wasm-bindgen --version` (`wasm-bindgen 0.2.130`)
/// against the library version `wanted`.
pub(crate) fn check_cli_version(wanted: &str, output: &str) -> Result<(), String> {
    let installed = output.split_whitespace().nth(1).unwrap_or("");
    if installed == wanted {
        return Ok(());
    }
    let found = if installed.is_empty() {
        String::from("an unknown version")
    } else {
        format!("version {installed}")
    };
    Err(format!(
        "wasm-bindgen {found} is installed, but Cargo.lock has wasm-bindgen {wanted}; \
         install the matching command with `{}`",
        install_command(wanted)
    ))
}

/// One file of the site and its sizes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileSize {
    /// The file's name.
    pub(crate) name: String,
    /// Its size in bytes.
    pub(crate) raw: u64,
    /// Its size after gzip at level 9.
    pub(crate) gzip: u64,
}

/// The sizes of every file in `dir`, by name.
fn measure(dir: &Path) -> Result<Vec<FileSize>, String> {
    let mut sizes = Vec::new();
    let entries =
        fs::read_dir(dir).map_err(|error| format!("cannot list {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot list {}: {error}", dir.display()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let bytes =
            fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        sizes.push(FileSize {
            name: entry.file_name().to_string_lossy().into_owned(),
            raw: len(bytes.len()),
            gzip: len(gzip(&bytes).len()),
        });
    }
    sizes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(sizes)
}

/// A length as `u64`.
fn len(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// `bytes` compressed by gzip at its highest level.
pub(crate) fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    // Writing into a vector cannot fail.
    let _ = encoder.write_all(bytes);
    encoder.finish().unwrap_or_default()
}

/// The measured totals of a site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Totals {
    /// All files, uncompressed.
    pub(crate) raw: u64,
    /// All files, each compressed by gzip at level 9.
    pub(crate) gzip: u64,
}

/// Bytes as MiB with two decimals.
fn mib(bytes: u64) -> String {
    let hundredths = bytes.saturating_mul(100) / (1024 * 1024);
    format!("{}.{:02} MiB", hundredths / 100, hundredths % 100)
}

impl fmt::Display for Totals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "total {} bytes ({}) uncompressed, limit {} bytes; {} bytes ({}) gzip -9, limit {} bytes",
            self.raw,
            mib(self.raw),
            MAX_UNCOMPRESSED,
            self.gzip,
            mib(self.gzip),
            MAX_GZIP
        )
    }
}

/// A site over its size budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BudgetError {
    /// The uncompressed total is above 2 MiB.
    Uncompressed(Totals),
    /// The gzip total is above 1 MiB.
    Gzip(Totals),
}

impl fmt::Display for BudgetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uncompressed(totals) => write!(
                f,
                "the web build is over its size budget: {} bytes ({}) uncompressed, \
                 the limit is {MAX_UNCOMPRESSED} bytes (2 MiB)",
                totals.raw,
                mib(totals.raw)
            ),
            Self::Gzip(totals) => write!(
                f,
                "the web build is over its size budget: {} bytes ({}) with gzip -9, \
                 the limit is {MAX_GZIP} bytes (1 MiB)",
                totals.gzip,
                mib(totals.gzip)
            ),
        }
    }
}

/// Sums `files` and checks the totals against the budget.
pub(crate) fn check_budget(files: &[FileSize]) -> Result<Totals, BudgetError> {
    let totals = Totals {
        raw: files.iter().map(|file| file.raw).sum(),
        gzip: files.iter().map(|file| file.gzip).sum(),
    };
    if totals.raw > MAX_UNCOMPRESSED {
        Err(BudgetError::Uncompressed(totals))
    } else if totals.gzip > MAX_GZIP {
        Err(BudgetError::Gzip(totals))
    } else {
        Ok(totals)
    }
}

/// The URL path a site is served under: starts and ends with `/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Prefix(String);

impl Prefix {
    /// The prefix `text`, with the slashes added where missing.
    pub(crate) fn new(text: &str) -> Result<Self, String> {
        let trimmed = text.trim_matches('/');
        if trimmed
            .split('/')
            .any(|segment| segment == ".." || segment == "." || segment.contains(['?', '#', '%']))
        {
            return Err(format!("not a usable URL path prefix: {text}"));
        }
        if trimmed.is_empty() {
            Ok(Self(String::from("/")))
        } else {
            Ok(Self(format!("/{trimmed}/")))
        }
    }
}

/// What the server answers for a request path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Route {
    /// The file at this path relative to the site directory.
    File(String),
    /// A redirect to this path.
    Redirect(String),
    /// Nothing is served there.
    NotFound,
}

/// The answer for the request target `target` under `prefix`.
pub(crate) fn route(prefix: &Prefix, target: &str) -> Route {
    let path = target.split(['?', '#']).next().unwrap_or("");
    let Some(rest) = path.strip_prefix(prefix.0.as_str()) else {
        // The prefix without its last slash, or the site root above it.
        if path == prefix.0.trim_end_matches('/') || path == "/" {
            return Route::Redirect(prefix.0.clone());
        }
        return Route::NotFound;
    };
    let rest = if rest.is_empty() { "index.html" } else { rest };
    if rest.ends_with('/')
        || rest.split('/').any(|segment| {
            segment.is_empty() || segment == ".." || segment == "." || segment.contains(['\\', '%'])
        })
    {
        return Route::NotFound;
    }
    Route::File(rest.to_owned())
}

/// The content type of a file, by its extension.
pub(crate) fn content_type(name: &str) -> &'static str {
    match name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Serves `dir` under `prefix` on `127.0.0.1:port` until interrupted.
fn serve(dir: &Path, port: u16, prefix: &Prefix) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|error| format!("cannot listen on 127.0.0.1:{port}: {error}"))?;
    let port = listener.local_addr().map_or(port, |address| address.port());
    println!(
        "xtask web: serving http://127.0.0.1:{port}{} (Ctrl+C stops)",
        prefix.0
    );
    // Flushed, so a test runner waiting for the line sees it.
    let _ = io::stdout().flush();
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let dir = dir.to_path_buf();
        let prefix = prefix.clone();
        std::thread::spawn(move || {
            if let Err(error) = answer(stream, &dir, &prefix) {
                eprintln!("xtask web: a request failed: {error}");
            }
        });
    }
    Ok(())
}

/// Answers one HTTP/1.1 request on `stream`, then closes it.
fn answer(stream: TcpStream, dir: &Path, prefix: &Prefix) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    // The headers are read and ignored.
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
    }
    let mut parts = request.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let mut stream = stream;
    if method != "GET" && method != "HEAD" {
        return respond(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"method not allowed\n",
            true,
        );
    }
    let body = method == "GET";
    match route(prefix, target) {
        Route::Redirect(to) => {
            let head = format!(
                "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(head.as_bytes())
        }
        Route::File(name) => match fs::read(dir.join(&name)) {
            Ok(bytes) => respond(&mut stream, "200 OK", content_type(&name), &bytes, body),
            Err(_) => respond(
                &mut stream,
                "404 Not Found",
                "text/plain; charset=utf-8",
                b"not found\n",
                body,
            ),
        },
        Route::NotFound => respond(
            &mut stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"not found\n",
            body,
        ),
    }
}

/// Writes a response with `status`, `kind` and, if `body`, `bytes`.
fn respond(
    stream: &mut TcpStream,
    status: &str,
    kind: &str,
    bytes: &[u8],
    body: bool,
) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\
         Cache-Control: no-cache\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    stream.write_all(head.as_bytes())?;
    if body {
        stream.write_all(bytes)?;
    }
    stream.flush()
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::{
        BudgetError, FileSize, MAX_GZIP, MAX_UNCOMPRESSED, Prefix, Route, Totals, check_budget,
        check_cli_version, content_type, gzip, locked_version, route,
    };

    fn file(name: &str, raw: u64, gzip: u64) -> FileSize {
        FileSize {
            name: name.to_owned(),
            raw,
            gzip,
        }
    }

    #[test]
    fn wasm_bindgen_version_mismatch_message() {
        assert_eq!(
            check_cli_version("0.2.130", "wasm-bindgen 0.2.130\n"),
            Ok(())
        );
        let message = check_cli_version("0.2.130", "wasm-bindgen 0.2.100\n").unwrap_err();
        assert_eq!(
            message,
            "wasm-bindgen version 0.2.100 is installed, but Cargo.lock has wasm-bindgen \
             0.2.130; install the matching command with \
             `cargo install wasm-bindgen-cli --version 0.2.130 --locked`"
        );
        let unknown = check_cli_version("0.2.130", "").unwrap_err();
        assert!(
            unknown.starts_with("wasm-bindgen an unknown version is installed"),
            "{unknown}"
        );
        assert!(unknown.ends_with("`cargo install wasm-bindgen-cli --version 0.2.130 --locked`"));
    }

    #[test]
    fn lock_file_version_is_read() {
        let lock = "[[package]]\nname = \"wasm-bindgen-futures\"\nversion = \"0.4.80\"\n\n\
                    [[package]]\nname = \"wasm-bindgen\"\nversion = \"0.2.130\"\n";
        assert_eq!(
            locked_version(lock, "wasm-bindgen").as_deref(),
            Some("0.2.130")
        );
        assert_eq!(
            locked_version(lock, "wasm-bindgen-futures").as_deref(),
            Some("0.4.80")
        );
        assert_eq!(locked_version(lock, "js-sys"), None);
        // The workspace's own lock file lists the library.
        let ours = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.lock"))
            .unwrap_or_default();
        assert!(locked_version(&ours, "wasm-bindgen").is_some());
    }

    #[test]
    fn budget_within_limits_passes() {
        // The spec's example: 1.6 MiB uncompressed, 0.7 MiB compressed.
        let files = [
            file("index.html", 3_000, 1_200),
            file("digger.js", 50_000, 9_000),
            file("digger_bg.wasm", 1_624_727, 723_819),
        ];
        let totals = check_budget(&files).unwrap();
        assert_eq!(
            totals,
            Totals {
                raw: 1_677_727,
                gzip: 734_019
            }
        );
        let line = totals.to_string();
        assert!(
            line.contains("1677727 bytes (1.60 MiB) uncompressed"),
            "{line}"
        );
        assert!(line.contains("734019 bytes (0.70 MiB) gzip -9"), "{line}");
        // Exactly at the limits is still within them.
        assert!(check_budget(&[file("a", MAX_UNCOMPRESSED, MAX_GZIP)]).is_ok());
    }

    #[test]
    fn budget_over_uncompressed_limit_fails() {
        // The spec's example: the uncompressed total rises to 2.2 MiB.
        let files = [
            file("digger.js", 50_000, 9_000),
            file("digger_bg.wasm", 2_256_868, 800_000),
        ];
        let error = check_budget(&files).unwrap_err();
        assert!(matches!(error, BudgetError::Uncompressed(_)));
        assert_eq!(
            error.to_string(),
            "the web build is over its size budget: 2306868 bytes (2.20 MiB) uncompressed, \
             the limit is 2097152 bytes (2 MiB)"
        );
        let over_gzip = check_budget(&[file("a", 1_000, MAX_GZIP + 1)]).unwrap_err();
        assert!(matches!(over_gzip, BudgetError::Gzip(_)));
        assert!(
            over_gzip
                .to_string()
                .contains("the limit is 1048576 bytes (1 MiB)")
        );
    }

    #[test]
    fn gzip_is_smaller_for_repetitive_data() {
        let data = vec![b'a'; 10_000];
        let packed = gzip(&data);
        assert!(packed.len() < 100, "{}", packed.len());
        // A gzip member: magic bytes, deflate, no timestamp.
        assert_eq!(&packed[..4], &[0x1f, 0x8b, 8, 0]);
        assert_eq!(&packed[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn content_types_by_extension() {
        assert_eq!(content_type("digger_bg.wasm"), "application/wasm");
        assert_eq!(content_type("digger.js"), "text/javascript; charset=utf-8");
        assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
        assert_eq!(content_type("style.css"), "text/css; charset=utf-8");
        assert_eq!(content_type("game.drf"), "application/octet-stream");
    }

    #[test]
    fn requests_route_under_the_prefix() {
        let prefix = Prefix::new("games/digger").unwrap();
        assert_eq!(prefix, Prefix::new("/games/digger/").unwrap());
        let file = |name: &str| Route::File(name.to_owned());
        assert_eq!(route(&prefix, "/games/digger/"), file("index.html"));
        assert_eq!(
            route(&prefix, "/games/digger/?speed=0&quiet"),
            file("index.html")
        );
        assert_eq!(
            route(&prefix, "/games/digger/digger_bg.wasm"),
            file("digger_bg.wasm")
        );
        let redirect = Route::Redirect(String::from("/games/digger/"));
        assert_eq!(route(&prefix, "/games/digger"), redirect);
        assert_eq!(route(&prefix, "/"), redirect);
        assert_eq!(route(&prefix, "/index.html"), Route::NotFound);
        assert_eq!(route(&prefix, "/games/digger/../secret"), Route::NotFound);
        assert_eq!(route(&prefix, "/games/digger/a//b"), Route::NotFound);
        assert_eq!(route(&prefix, "/games/digger/sub/"), Route::NotFound);
        let root = Prefix::new("/").unwrap();
        assert_eq!(route(&root, "/"), file("index.html"));
        assert_eq!(route(&root, "/boot.js"), file("boot.js"));
        assert!(Prefix::new("/a/../b").is_err());
    }
}
