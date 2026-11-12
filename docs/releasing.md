# Releasing

How a release of Digger is made. A release is a Git tag `vX.Y.Z` (or
`vX.Y.Z-suffix` for a pre-release); the workflow
[`.github/workflows/release.yml`](../.github/workflows/release.yml) builds
everything from that tag and stops at a **draft** GitHub release, which a
maintainer checks and publishes by hand. The design is in
`openspec/changes/add-web-build-and-releases/design.md` (decisions 13 to 15).

## One-time repository setup

Done once, in the repository's settings on GitHub:

1. **Settings → Pages → Build and deployment → Source: GitHub Actions.** The
   workflow deploys with the Pages actions, not from a branch.
2. **Settings → Environments → `github-pages` → Deployment branches and
   tags:** add a tag rule `v*`. GitHub creates this environment allowing
   only the default branch, and the `publish` job, which runs for a tag and
   uses the environment, is refused without the rule (pre-release tags
   included, although they skip the deployment itself).
3. **Settings → Actions → General → Workflow permissions** may stay
   *read-only*: the workflow asks for `contents: write`, `pages: write` and
   `id-token: write` in its `publish` job only.

## 1. Bump the version

The version has one source in the tree: `version` in the
`[workspace.package]` table of the root `Cargo.toml`. Every crate inherits
it, the programs report it (`digger --version`), and the web page shows it
in its footer. Change it there, for example to `0.2.0` or `0.2.0-rc.1`,
then refresh the lock file and run the checks:

```bash
cargo update --workspace
```

```bash
cargo xtask ci
```

```bash
cargo deny check
```

Commit the change (`chore(release): 0.2.0`) on `main` through the usual
pull request, so CI has run on exactly the commit that will be tagged.

Before tagging, the tag can be checked against the version as the workflow
will check it (it prints the version and whether it is a pre-release):

```bash
.github/scripts/release-version.sh v0.1.0
```

A release archive can also be built locally on its own platform, as the
workflow does, to look at it before tagging (the archive is written to
`dist/`; the web build needs `wasm-bindgen-cli` at the version of the
`wasm-bindgen` library in `Cargo.lock`, and every archive needs
`cargo about`):

```bash
cargo install --locked cargo-about --version 0.9.3 --features cli
```

```bash
cargo xtask dist --platform linux-x86_64 --version 0.1.0
```

```bash
cargo xtask dist --platform web --version 0.1.0 --check-reproducible
```

## 2. Tag

Tag the bumped commit on `main` with an annotated tag and push the tag
(here for version 0.2.0):

```bash
git tag -a v0.2.0 -m "Digger 0.2.0"
```

```bash
git push origin v0.2.0
```

A tag that is not exactly `v` followed by the workspace version fails the
workflow's first job and builds nothing; delete it (`git push origin
--delete v0.2.0` and `git tag -d v0.2.0`), fix the version or the tag, and
tag again.

## 3. What the workflow does

| Job | Runner | What it does |
|---|---|---|
| `verify` | `ubuntu-24.04` | Checks the tag against the workspace version (`.github/scripts/release-version.sh`), runs `cargo xtask ci` (format, Clippy, tests, documentation, dependency direction) and the licence gate `cargo deny check` |
| `linux-x86_64` | `ubuntu-22.04` | `cargo xtask dist --platform linux-x86_64`: `digger-V-linux-x86_64.tar.gz` |
| `linux-aarch64` | `ubuntu-22.04-arm` | The same on a hosted arm64 runner: `digger-V-linux-aarch64.tar.gz` |
| `windows-x86_64` | `windows-2022` | MSVC build with the C runtime linked statically: `digger-V-windows-x86_64.zip` |
| `macos-universal` | `macos-15` | Both Apple targets, joined with `lipo`, ad-hoc signed, in `Digger.app`: `digger-V-macos-universal.tar.gz` |
| `web` | `ubuntu-24.04` | `cargo xtask dist --platform web` (`digger-V-web.tar.gz`), then the Playwright smoke tests in Chromium, Firefox and WebKit against the `site/` of that archive |
| `reproducible` | `ubuntu-22.04` | Builds the Linux x86_64 and the web archive twice each, in two copies of the tree in different directories, and fails unless the SHA-256 values agree |
| `publish` | `ubuntu-24.04` | Collects the five archives, writes `SHA256SUMS` (one `sha256  file` line per archive, sorted by name), creates the draft release with all six files, and deploys the web `site/` to GitHub Pages unless the tag has a pre-release suffix |

Every archive has one top-level folder named like the archive, holding the
program (or `site/`), `LICENSE`, `NOTICE`, `THIRD-PARTY-LICENSES.html`
(the licences of every crate linked into that build, made by
`cargo about`), `MANUAL.md` and `README.md`; the Linux archives add
`digger.desktop` and `digger.png`. Archive entries are sorted, dated
`SOURCE_DATE_EPOCH` (the tagged commit's time) and owned by user 0, so the
same tag built on the same runner image gives the same bytes.

The Linux builds run on Ubuntu 22.04, which sets the minimum glibc of the
Linux programs to 2.35; macOS builds target macOS 11.

## 4. Check the draft

Open the draft on the repository's *Releases* page and check:

- it holds `SHA256SUMS` and five archives: `digger-V-linux-x86_64.tar.gz`,
  `digger-V-linux-aarch64.tar.gz`, `digger-V-windows-x86_64.zip`,
  `digger-V-macos-universal.tar.gz` and `digger-V-web.tar.gz`;
- the downloads match the sums: download them all into one folder and run
  `sha256sum -c SHA256SUMS` there;
- each native program starts on its system, plays with sound, and
  `digger --version` reports V;
- for a release (not a pre-release), the Pages site shows the game and its
  footer shows version V and links to the `vV` tag.

With the GitHub command-line tool the draft can be fetched in one go:

```bash
gh release download v0.2.0 --dir release-v0.2.0
```

## 5. Publish

Edit the generated notes if needed and press *Publish release*, or:

```bash
gh release edit v0.2.0 --draft=false
```

A pre-release tag (`v0.2.0-rc.1`) is marked as a pre-release and never
deploys the site; it is the way to try the workflow. Its draft is usually
deleted after inspection (next section).

## Rollback

- **A draft that is wrong:** delete it, and the tag, then fix and tag again:

  ```bash
  gh release delete v0.2.0 --yes --cleanup-tag
  ```

  Also delete the local tag with `git tag -d v0.2.0`.
- **A published release that is wrong:** do not move or reuse its tag.
  Publish a new patch release (`v0.2.1`) with the fix. The bad release may
  be marked as a pre-release or deleted with the command above.
- **The site:** the deployment always shows the last release tag that ran.
  To put back the site of an earlier release, re-run that tag's workflow
  (*Actions → Release →* the earlier tag's run *→ Re-run all jobs*); it
  rebuilds the archives and deploys that tag's site; a release that
  already exists for the tag, draft or published, is left as it is.
  From the command line:

  ```bash
  gh run list --workflow release.yml
  ```

  ```bash
  gh run rerun <run id>
  ```

## Fallback: Linux aarch64 with `cross`

The `linux-aarch64` job builds natively on GitHub's hosted arm64 runner,
because a cross build needs the arm64 ALSA and udev libraries that `cpal`
and `gilrs` link. If those runners are unavailable, build that archive on
an x86_64 Linux machine with Docker and [`cross`](https://github.com/cross-rs/cross).
This fallback is documented but not exercised by the workflow; check its
output before uploading it.

1. Install `cross` from its repository (its crates.io release is old):

   ```bash
   cargo install cross --locked --git https://github.com/cross-rs/cross
   ```

2. Give the build container the arm64 development packages with a
   `Cross.toml` at the workspace root (not committed):

   ```toml
   [target.aarch64-unknown-linux-gnu]
   pre-build = [
     "dpkg --add-architecture $CROSS_DEB_ARCH",
     "apt-get update && apt-get install -y --no-install-recommends libasound2-dev:$CROSS_DEB_ARCH libudev-dev:$CROSS_DEB_ARCH",
   ]
   ```

3. Build the developer tool, then run it directly with `CARGO=cross`, so
   `xtask dist` builds the program through `cross` and does everything else
   (licence file, layout, archive) on the host as usual. `cargo xtask` cannot
   be used here, because `cargo run` sets `CARGO` to itself:

   ```bash
   cargo build --profile xtask -p xtask
   ```

   ```bash
   CARGO=cross target/xtask/xtask dist --platform linux-aarch64 --version 0.2.0
   ```

   The program is not started on an x86_64 host, so check its version on an
   arm64 machine before uploading `dist/digger-0.2.0-linux-aarch64.tar.gz`
   to the draft (`gh release upload v0.2.0 <file>`) and adding its line to
   `SHA256SUMS`.

The image's glibc decides the minimum glibc of that build; cross's current
images use Ubuntu 20.04 (glibc 2.31), below the 2.35 the manual states.

## Licences

The workspace is licensed GPL-2.0-or-later. `cargo deny check` (in CI and
in the `verify` job) checks the licences of every crate of the dependency
graphs of all release targets, `wasm32-unknown-unknown` included
(`deny.toml`, `[graph] targets`), against the allow-list in `deny.toml`,
where each entry names the crates that need it. Most dependencies are
offered under MIT or Apache-2.0 and are taken under MIT.

A few crates are offered **under Apache-2.0 only**: `winit` and `dpi` (in
every build, the web one included), `cpal` (every build), and on Linux
`gethostname`, `ab_glyph`, `ab_glyph_rasterizer` and `owned_ttf_parser`.
Apache-2.0 is compatible with version 3 of the GNU GPL but not with
version 2. The binaries that contain these crates, which are all release
binaries, are therefore conveyed under the "or later" option of
GPL-2.0-or-later, that is under the GNU GPL version 3; the source code
stays GPL-2.0-or-later. Each archive's `LICENSE` holds the GPL version 2
text and `THIRD-PARTY-LICENSES.html` the licence texts of the crates in
that build. Version 3 requires recipients to get a copy of its own text,
which the archives do not carry yet; adding it is an open question of the
design (`openspec/changes/add-web-build-and-releases/design.md`, "Open
Questions") to settle before the first public release. A new dependency whose
licence is not in the allow-list fails the release until its licence has
been reviewed and added with a comment.
