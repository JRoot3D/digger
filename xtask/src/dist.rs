//! `cargo xtask dist`: one release archive, built reproducibly
//! (add-web-build-and-releases, design decisions 13 and 14).
//!
//! `cargo xtask dist --platform <name> --version <V>` checks that V is the
//! workspace version, builds the program for that platform with `--locked`
//! and the workspace and cargo home paths remapped, writes the licences of
//! every linked crate with `cargo about`, stages the archive layout and
//! writes `dist/digger-<V>-<platform>.<tar.gz|zip>` with the deterministic
//! writers of [`crate::archive`]. With `--check-reproducible` it does all
//! of that twice, in two copies of the source tree in different
//! directories, and fails unless both archives have the same SHA-256.
//!
//! ```text
//! digger-<V>-<platform>/
//!   digger | digger.exe | Digger.app/ | site/
//!   LICENSE  NOTICE  THIRD-PARTY-LICENSES.html  MANUAL.md  README.md
//!   digger.desktop  digger.png                      (Linux only)
//! ```

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::{Args, ValueEnum};

use crate::archive::{Archive, sha256_hex};
use crate::web;

/// A platform a release archive is made for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Platform {
    /// 64-bit PC Linux (glibc).
    #[value(name = "linux-x86_64")]
    LinuxX86_64,
    /// 64-bit ARM Linux (glibc), built on an ARM machine.
    #[value(name = "linux-aarch64")]
    LinuxAarch64,
    /// 64-bit Windows, MSVC tool chain, static C runtime.
    #[value(name = "windows-x86_64")]
    WindowsX86_64,
    /// macOS 11 or later, one program for Apple silicon and Intel, as an
    /// application bundle.
    #[value(name = "macos-universal")]
    MacosUniversal,
    /// The static web site.
    #[value(name = "web")]
    Web,
}

impl Platform {
    /// The name in the archive's file name.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::LinuxX86_64 => "linux-x86_64",
            Self::LinuxAarch64 => "linux-aarch64",
            Self::WindowsX86_64 => "windows-x86_64",
            Self::MacosUniversal => "macos-universal",
            Self::Web => "web",
        }
    }

    /// The Rust targets built for the platform.
    pub(crate) const fn targets(self) -> &'static [&'static str] {
        match self {
            Self::LinuxX86_64 => &["x86_64-unknown-linux-gnu"],
            Self::LinuxAarch64 => &["aarch64-unknown-linux-gnu"],
            Self::WindowsX86_64 => &["x86_64-pc-windows-msvc"],
            Self::MacosUniversal => &["aarch64-apple-darwin", "x86_64-apple-darwin"],
            Self::Web => &["wasm32-unknown-unknown"],
        }
    }

    /// The archive's file-name extension.
    pub(crate) const fn extension(self) -> &'static str {
        match self {
            Self::WindowsX86_64 => "zip",
            _ => "tar.gz",
        }
    }

    /// The program's path inside the archive's top directory, if the
    /// archive holds one.
    pub(crate) const fn program(self) -> Option<&'static str> {
        match self {
            Self::LinuxX86_64 | Self::LinuxAarch64 => Some("digger"),
            Self::WindowsX86_64 => Some("digger.exe"),
            Self::MacosUniversal => Some("Digger.app/Contents/MacOS/digger"),
            Self::Web => None,
        }
    }

    /// Whether the machine running xtask can start this platform's
    /// program.
    fn runs_here(self) -> bool {
        use std::env::consts::{ARCH, OS};
        match self {
            Self::LinuxX86_64 => OS == "linux" && ARCH == "x86_64",
            Self::LinuxAarch64 => OS == "linux" && ARCH == "aarch64",
            Self::WindowsX86_64 => OS == "windows" && ARCH == "x86_64",
            Self::MacosUniversal => OS == "macos",
            Self::Web => false,
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Arguments of `cargo xtask dist`.
#[derive(Debug, Args)]
pub(crate) struct DistArgs {
    /// The platform to build the archive for.
    #[arg(long)]
    platform: Platform,
    /// The version being released; must equal the workspace version.
    #[arg(long)]
    version: String,
    /// Build the archive twice, in two copies of the source tree in
    /// different directories, and fail unless both are byte-identical.
    #[arg(long)]
    check_reproducible: bool,
}

/// The workspace root.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// One archive to build.
#[derive(Debug, Clone)]
struct Job {
    /// The source tree.
    root: PathBuf,
    /// Cargo's target directory for the build.
    target_dir: PathBuf,
    /// Where the archive is written.
    out_dir: PathBuf,
    /// The version.
    version: String,
    /// The platform.
    platform: Platform,
    /// The time of every archive entry, seconds since the Unix epoch.
    epoch: u64,
}

impl Job {
    /// The archive's name without extension, also its top directory.
    fn name(&self) -> String {
        format!("digger-{}-{}", self.version, self.platform)
    }
}

/// Runs `cargo xtask dist`.
pub(crate) fn run(args: &DistArgs) -> Result<(), String> {
    let root = workspace_root();
    let manifest = fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|error| format!("cannot read Cargo.toml: {error}"))?;
    check_version(&args.version, &manifest)?;
    let epoch = source_date_epoch(&root)?;
    let target_dir =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let job = Job {
        out_dir: root.join("dist"),
        root,
        target_dir,
        version: args.version.clone(),
        platform: args.platform,
        epoch,
    };
    if args.check_reproducible {
        check_reproducible(&job)
    } else {
        let archive = build_archive(&job)?;
        report(&archive)
    }
}

/// Prints the archive's path and SHA-256.
fn report(archive: &Path) -> Result<(), String> {
    let bytes =
        fs::read(archive).map_err(|error| format!("cannot read {}: {error}", archive.display()))?;
    println!("xtask dist: {}  {}", sha256_hex(&bytes), archive.display());
    Ok(())
}

/// The `version` of `[workspace.package]` in the workspace manifest's text.
pub(crate) fn workspace_version(manifest: &str) -> Result<String, String> {
    let table: toml::Table = manifest
        .parse()
        .map_err(|error| format!("Cargo.toml is not valid TOML: {error}"))?;
    table
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| String::from("Cargo.toml has no [workspace.package] version"))
}

/// Checks that `wanted` is the workspace version of `manifest`.
pub(crate) fn check_version(wanted: &str, manifest: &str) -> Result<(), String> {
    let version = workspace_version(manifest)?;
    if wanted == version {
        Ok(())
    } else {
        Err(format!(
            "--version {wanted} does not match the workspace version {version} in Cargo.toml"
        ))
    }
}

/// The time of the archive entries: `SOURCE_DATE_EPOCH`, else the commit
/// time of `HEAD`.
fn source_date_epoch(root: &Path) -> Result<u64, String> {
    if let Some(value) = std::env::var_os("SOURCE_DATE_EPOCH") {
        let text = value.to_string_lossy();
        return text
            .trim()
            .parse()
            .map_err(|_| format!("SOURCE_DATE_EPOCH is not a number of seconds: {text}"));
    }
    let output = Command::new("git")
        .current_dir(root)
        .args(["log", "-1", "--format=%ct"])
        .output()
        .map_err(|error| format!("cannot run git: {error}; set SOURCE_DATE_EPOCH"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim()
        .parse()
        .map_err(|_| String::from("cannot read the commit time of HEAD; set SOURCE_DATE_EPOCH"))
}

/// The cargo home directory: `CARGO_HOME`, else `.cargo` in the home
/// directory.
fn cargo_home() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join(".cargo"))
        })
}

/// The `--config` value giving `target`'s builds the flags that keep the
/// machine's paths out of the program: the source tree becomes
/// `/digger` and the cargo home `/cargo`. Cargo joins this array to the
/// flags of `.cargo/config.toml` (the Windows static C runtime).
pub(crate) fn remap_config(target: &str, root: &Path, cargo_home: Option<&Path>) -> String {
    let mut flags = Vec::new();
    if let Some(home) = cargo_home {
        flags.push(format!("--remap-path-prefix={}=/cargo", home.display()));
    }
    flags.push(format!("--remap-path-prefix={}=/digger", root.display()));
    let quoted: Vec<String> = flags
        .into_iter()
        .map(|flag| toml::Value::String(flag).to_string())
        .collect();
    format!("target.{target}.rustflags=[{}]", quoted.join(", "))
}

/// Runs `command`, failing with `what` when it does not succeed.
fn run_command(command: &mut Command, what: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("cannot run {what}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{what} failed ({status})"))
    }
}

/// The environment of every release build.
fn build_env(job: &Job) -> Vec<(String, String)> {
    vec![
        (String::from("SOURCE_DATE_EPOCH"), job.epoch.to_string()),
        // The page footer links to the source of this tag.
        (String::from("DIGGER_REVISION"), format!("v{}", job.version)),
    ]
}

/// Builds the release program for `target`; returns its path.
fn build_program(job: &Job, target: &str) -> Result<PathBuf, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    println!("xtask dist: cargo build -p digger --release --locked --target {target}");
    let mut command = Command::new(cargo);
    command
        .current_dir(&job.root)
        .args([
            "build",
            "-p",
            "digger",
            "--release",
            "--locked",
            "--target",
            target,
        ])
        .arg("--target-dir")
        .arg(&job.target_dir)
        .arg("--config")
        .arg(remap_config(target, &job.root, cargo_home().as_deref()))
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .envs(build_env(job));
    if job.platform == Platform::MacosUniversal {
        command.env("MACOSX_DEPLOYMENT_TARGET", "11.0");
    }
    run_command(&mut command, "the release build")?;
    let file = if target.contains("windows") {
        "digger.exe"
    } else {
        "digger"
    };
    Ok(job.target_dir.join(target).join("release").join(file))
}

/// Copies `from` to `to`, creating `to`'s directory.
fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    fs::copy(from, to).map(|_| ()).map_err(|error| {
        format!(
            "cannot copy {} to {}: {error}",
            from.display(),
            to.display()
        )
    })
}

/// Copies the directory `from` to `to`, recursively.
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|error| format!("cannot create {}: {error}", to.display()))?;
    let listing =
        fs::read_dir(from).map_err(|error| format!("cannot list {}: {error}", from.display()))?;
    for item in listing {
        let item = item.map_err(|error| format!("cannot list {}: {error}", from.display()))?;
        let path = item.path();
        let target = to.join(item.file_name());
        if path.is_dir() {
            copy_tree(&path, &target)?;
        } else {
            copy(&path, &target)?;
        }
    }
    Ok(())
}

/// `packaging/macos/Info.plist` for `version`: the short version is the
/// version as released, the bundle version its numeric part (macOS wants
/// up to three dot-separated numbers there).
pub(crate) fn info_plist(template: &str, version: &str) -> String {
    let numeric = version.split(['-', '+']).next().unwrap_or(version);
    template
        .replace("@BUNDLE_VERSION@", numeric)
        .replace("@VERSION@", version)
}

/// Builds the universal program and assembles `Digger.app` in `stage`.
fn stage_macos(job: &Job, stage: &Path) -> Result<(), String> {
    let mut parts = Vec::new();
    for target in Platform::MacosUniversal.targets() {
        parts.push(build_program(job, target)?);
    }
    let app = stage.join("Digger.app");
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    fs::create_dir_all(&macos)
        .map_err(|error| format!("cannot create {}: {error}", macos.display()))?;
    run_command(
        Command::new("lipo")
            .arg("-create")
            .arg("-output")
            .arg(macos.join("digger"))
            .args(&parts),
        "lipo",
    )?;
    let packaging = job.root.join("packaging").join("macos");
    let template = fs::read_to_string(packaging.join("Info.plist"))
        .map_err(|error| format!("cannot read packaging/macos/Info.plist: {error}"))?;
    fs::write(
        contents.join("Info.plist"),
        info_plist(&template, &job.version),
    )
    .map_err(|error| format!("cannot write Info.plist: {error}"))?;
    copy(
        &packaging.join("digger.icns"),
        &contents.join("Resources").join("digger.icns"),
    )?;
    // Apple silicon refuses unsigned programs; an ad-hoc signature (no
    // identity) is enough for a program the user chose to open.
    run_command(
        Command::new("codesign")
            .args(["--force", "--sign", "-"])
            .arg(&app),
        "codesign",
    )
}

/// Writes `THIRD-PARTY-LICENSES.html` for the platform's targets to `to`.
fn licences(job: &Job, to: &Path) -> Result<(), String> {
    let mut command = Command::new("cargo");
    command
        .current_dir(&job.root)
        .args(["about", "generate", "--locked", "--offline"])
        .args(["--manifest-path", "crates/digger/Cargo.toml"])
        .args(["--config", "packaging/about.toml"]);
    for target in job.platform.targets() {
        command.args(["--target", target]);
    }
    command
        .arg("--output-file")
        .arg(to)
        .arg("packaging/about.hbs");
    run_command(&mut command, "cargo about").map_err(|error| {
        format!("{error}; it is installed with `cargo install --locked cargo-about --features cli`")
    })
}

/// Builds the job's archive; returns its path.
fn build_archive(job: &Job) -> Result<PathBuf, String> {
    let name = job.name();
    // Every crate of every target, so `cargo about --offline` finds them.
    run_command(
        Command::new("cargo")
            .current_dir(&job.root)
            .args(["fetch", "--locked"]),
        "cargo fetch",
    )?;
    let stage = job.target_dir.join("dist-stage").join(&name);
    if stage.exists() {
        fs::remove_dir_all(&stage)
            .map_err(|error| format!("cannot clear {}: {error}", stage.display()))?;
    }
    fs::create_dir_all(&stage)
        .map_err(|error| format!("cannot create {}: {error}", stage.display()))?;

    match job.platform {
        Platform::LinuxX86_64 | Platform::LinuxAarch64 | Platform::WindowsX86_64 => {
            let program =
                build_program(job, job.platform.targets().first().copied().unwrap_or(""))?;
            let file = job.platform.program().unwrap_or("digger");
            copy(&program, &stage.join(file))?;
        }
        Platform::MacosUniversal => stage_macos(job, &stage)?,
        Platform::Web => {
            let site = web::build(
                &job.root,
                &web::BuildOptions {
                    target_dir: Some(job.target_dir.clone()),
                    cargo_config: vec![remap_config(
                        "wasm32-unknown-unknown",
                        &job.root,
                        cargo_home().as_deref(),
                    )],
                    env: build_env(job),
                },
            )?;
            copy_tree(&site, &stage.join("site"))?;
        }
    }
    if matches!(job.platform, Platform::LinuxX86_64 | Platform::LinuxAarch64) {
        let linux = job.root.join("packaging").join("linux");
        copy(&linux.join("digger.desktop"), &stage.join("digger.desktop"))?;
        copy(&linux.join("digger.png"), &stage.join("digger.png"))?;
    }
    for (from, to) in [
        ("LICENSE", "LICENSE"),
        ("NOTICE", "NOTICE"),
        ("docs/manual.md", "MANUAL.md"),
        ("README.md", "README.md"),
    ] {
        copy(&job.root.join(from), &stage.join(to))?;
    }
    licences(job, &stage.join("THIRD-PARTY-LICENSES.html"))?;

    if let Some(program) = job.platform.program()
        && job.platform.runs_here()
    {
        check_program_version(&stage.join(program), &job.version)?;
    }

    let mut archive = Archive::new(job.epoch);
    let programs: Vec<&str> = job.platform.program().into_iter().collect();
    archive.add_tree(&stage, &name, &programs)?;
    let bytes = match job.platform.extension() {
        "zip" => archive.to_zip()?,
        _ => archive.to_tar_gz()?,
    };
    fs::create_dir_all(&job.out_dir)
        .map_err(|error| format!("cannot create {}: {error}", job.out_dir.display()))?;
    let path = job
        .out_dir
        .join(format!("{name}.{}", job.platform.extension()));
    fs::write(&path, bytes).map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(path)
}

/// Checks that `program --version` prints `digger <version>`.
fn check_program_version(program: &Path, version: &str) -> Result<(), String> {
    let output = Command::new(program)
        .arg("--version")
        .output()
        .map_err(|error| format!("cannot run {}: {error}", program.display()))?;
    let printed = String::from_utf8_lossy(&output.stdout);
    let wanted = format!("digger {version}");
    if output.status.success() && printed.trim() == wanted {
        println!("xtask dist: the program reports `{wanted}`");
        Ok(())
    } else {
        Err(format!(
            "{} --version printed `{}`, not `{wanted}`",
            program.display(),
            printed.trim()
        ))
    }
}

/// The files of the source tree at `root` that git tracks or would track
/// (untracked files that are not ignored), relative to `root`.
fn source_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if !output.status.success() {
        return Err(String::from("git ls-files failed"));
    }
    Ok(output
        .stdout
        .split(|&byte| byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| PathBuf::from(String::from_utf8_lossy(name).into_owned()))
        // A file deleted from the work tree but still in the index.
        .filter(|name| root.join(name).is_file())
        .collect())
}

/// Builds the archive in two copies of the source tree at different paths
/// and compares the SHA-256 values; on success the archive is also written
/// to the job's output directory.
fn check_reproducible(job: &Job) -> Result<(), String> {
    let files = source_files(&job.root)?;
    let base = job.target_dir.join("reproducible");
    if base.exists() {
        fs::remove_dir_all(&base)
            .map_err(|error| format!("cannot clear {}: {error}", base.display()))?;
    }
    let mut results = Vec::new();
    for copy_root in [base.join("one").join("digger"), base.join("second-copy")] {
        println!("xtask dist: building in {}", copy_root.display());
        for file in &files {
            copy(&job.root.join(file), &copy_root.join(file))?;
        }
        let copy_job = Job {
            root: copy_root.clone(),
            target_dir: copy_root.join("target"),
            out_dir: copy_root.join("dist"),
            ..job.clone()
        };
        let archive = build_archive(&copy_job)?;
        let bytes = fs::read(&archive)
            .map_err(|error| format!("cannot read {}: {error}", archive.display()))?;
        println!("xtask dist: {}  {}", sha256_hex(&bytes), archive.display());
        results.push((archive, sha256_hex(&bytes)));
    }
    let [(first, first_hash), (second, second_hash)] = results.as_slice() else {
        return Err(String::from("expected two builds"));
    };
    if first_hash != second_hash {
        return Err(format!(
            "the archive is not reproducible: {} has SHA-256 {first_hash}, {} has {second_hash}",
            first.display(),
            second.display()
        ));
    }
    let kept = job.out_dir.join(first.file_name().unwrap_or_default());
    copy(first, &kept)?;
    println!(
        "xtask dist: reproducible: both builds have SHA-256 {first_hash}; kept as {}",
        kept.display()
    );
    // The two copies hold full target directories; a failed check keeps
    // them for comparison.
    fs::remove_dir_all(&base).map_err(|error| format!("cannot remove {}: {error}", base.display()))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use std::path::Path;

    use super::{Platform, check_version, info_plist, remap_config, workspace_version};

    #[test]
    fn version_must_equal_workspace_version() {
        let manifest = "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"1.2.0\"\n";
        assert_eq!(workspace_version(manifest).unwrap(), "1.2.0");
        assert!(check_version("1.2.0", manifest).is_ok());
        assert_eq!(
            check_version("1.2.1", manifest).unwrap_err(),
            "--version 1.2.1 does not match the workspace version 1.2.0 in Cargo.toml"
        );
        assert!(workspace_version("[package]\nversion = \"1.0.0\"\n").is_err());
        // The workspace's own manifest has a version.
        let ours =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml")).unwrap();
        assert!(check_version(env!("CARGO_PKG_VERSION"), &ours).is_ok());
    }

    #[test]
    fn platform_names_targets_and_formats() {
        let names: Vec<&str> = [
            Platform::LinuxX86_64,
            Platform::LinuxAarch64,
            Platform::WindowsX86_64,
            Platform::MacosUniversal,
            Platform::Web,
        ]
        .iter()
        .map(|platform| platform.name())
        .collect();
        assert_eq!(
            names,
            [
                "linux-x86_64",
                "linux-aarch64",
                "windows-x86_64",
                "macos-universal",
                "web"
            ]
        );
        assert_eq!(Platform::WindowsX86_64.extension(), "zip");
        assert_eq!(Platform::Web.extension(), "tar.gz");
        assert_eq!(Platform::MacosUniversal.targets().len(), 2);
        assert_eq!(Platform::Web.program(), None);
        assert_eq!(Platform::WindowsX86_64.program(), Some("digger.exe"));
    }

    #[test]
    fn remapping_covers_tree_and_cargo_home() {
        let config = remap_config(
            "x86_64-pc-windows-msvc",
            Path::new(r"C:\work\digger"),
            Some(Path::new(r"C:\Users\ann\.cargo")),
        );
        assert_eq!(
            config,
            r"target.x86_64-pc-windows-msvc.rustflags=['--remap-path-prefix=C:\Users\ann\.cargo=/cargo', '--remap-path-prefix=C:\work\digger=/digger']"
        );
        let value: toml::Table = config
            .replacen('=', " = ", 1)
            .replace("target.x86_64-pc-windows-msvc.rustflags", "flags")
            .parse()
            .unwrap();
        assert_eq!(value.get("flags").unwrap().as_array().unwrap().len(), 2);
    }

    #[test]
    fn bundle_versions() {
        let template = "<string>@VERSION@</string><string>@BUNDLE_VERSION@</string>";
        assert_eq!(
            info_plist(template, "0.1.0-rc.1"),
            "<string>0.1.0-rc.1</string><string>0.1.0</string>"
        );
        assert_eq!(
            info_plist(template, "1.2.0"),
            "<string>1.2.0</string><string>1.2.0</string>"
        );
    }
}
