# Digger (Rust)

An idiomatic Rust re-creation of [Digger Remastered](https://digger.org/), the
1983 Windmill Software arcade game as reverse-engineered by Andrew Jenner and
extended in [sobomax/digger](https://github.com/sobomax/digger).

**Status:** the game is playable on the desktop (Linux, Windows, macOS) and
in a web browser. The roadmap and its phases are in
[`docs/porting-plan.md`](docs/porting-plan.md); the player's manual is
[`docs/manual.md`](docs/manual.md).

## Download

Each release on the repository's
[Releases page](https://github.com/JRoot3D/digger/releases) has one archive
per platform and a `SHA256SUMS` file to check them with
`sha256sum -c SHA256SUMS`:

| Archive | For |
|---|---|
| `digger-<version>-linux-x86_64.tar.gz` | 64-bit PC Linux, glibc 2.35 or later |
| `digger-<version>-linux-aarch64.tar.gz` | 64-bit ARM Linux, glibc 2.35 or later |
| `digger-<version>-windows-x86_64.zip` | 64-bit Windows |
| `digger-<version>-macos-universal.tar.gz` | macOS 11 or later, Apple silicon and Intel |
| `digger-<version>-web.tar.gz` | The web version, to put on any static web server |

Unpack the archive and start `digger` (`digger.exe` on Windows,
`Digger.app` on macOS). The builds are not signed, so Windows and macOS
warn on the first start; the manual's "System requirements" section says
how to start them anyway. Each archive holds the manual (`MANUAL.md`), the
licence and the licences of the libraries it contains.

## Play online

The web version of the latest release is at
<https://jroot3d.github.io/digger/>. It needs a keyboard or a gamepad and
a current Chrome, Edge, Firefox or Safari on a desktop computer; sound
starts after the first key press or click. Settings and high scores are
kept in the browser. See "Playing in a web browser" in the manual.

## Building and testing

Install [rustup](https://rustup.rs/). The toolchain is pinned to Rust 1.99 in
`rust-toolchain.toml` and is installed automatically by the first `cargo`
command run in the repository.

```bash
cargo build --workspace
```

```bash
cargo test --workspace
```

Run the same checks as continuous integration (format, Clippy with warnings
denied, tests, documentation, dependency direction) before every commit:

```bash
cargo xtask ci
```

Play the desktop version from the checkout:

```bash
cargo run --release -p digger
```

### Web build

The web version needs the WebAssembly target (installed with the pinned
toolchain) and `wasm-bindgen-cli` at exactly the version of the
`wasm-bindgen` library in `Cargo.lock` (`cargo xtask web` names the
command to install it when it is missing or of another version):

```bash
cargo install --locked wasm-bindgen-cli --version 0.2.130 --bin wasm-bindgen
```

`cargo xtask web` builds the site into `dist/web/` and checks its download
size budget; with `--serve` it then serves it at
<http://127.0.0.1:8000/> (`--port` and `--prefix` change the port and the
sub-folder):

```bash
cargo xtask web --serve
```

The browser smoke tests in [`web/tests/`](web/tests) (Node.js and
Playwright) build and serve the page themselves:

```bash
cd web/tests && npm ci && npx playwright install chromium && npx playwright test --project=chromium
```

Release archives are built with `cargo xtask dist`; making a release is
described in [`docs/releasing.md`](docs/releasing.md).

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the full set of checks, the code
conventions, commit messages and the OpenSpec workflow.

## Developer tooling

`cargo xtask <command>` runs the developer tool in [`xtask/`](xtask);
`cargo xtask --help` lists every command. Assets and recordings are covered
in [`docs/assets.md`](docs/assets.md) and
[`docs/conformance.md`](docs/conformance.md).

**Sound.** `cargo xtask render-sound <script> <out.wav>` renders a sound
command script through the sound engine to a 16-bit mono WAV file, for
listening. A script has one `<sample offset> <command>` entry per line and
ends with `<offset> end` (the format is documented in the `digger_sound::script`
module); the golden scripts in
[`crates/digger-sound/tests/golden/`](crates/digger-sound/tests/golden) are
examples. The defaults are those of the golden tests (44100 Hz, filter off,
volume 100); `--rate`, `--filter on|off` and `--volume` change them.

```bash
cargo xtask render-sound crates/digger-sound/tests/golden/mixed.txt target/sound/mixed.wav
```

The golden test `cargo test -p digger-sound --test golden` compares the hash
of each script's render with `tests/golden/hashes.toml`; after an intended
change, regenerate it with `UPDATE_GOLDEN=1` and review the new renders with
`render-sound` (see `tests/golden/REVIEW.md`).

## Where things are

| Path | Contents |
|---|---|
| [`docs/porting-plan.md`](docs/porting-plan.md) | Roadmap: goals, architecture, conventions, verification strategy, phases |
| [`docs/research/`](docs/research/README.md) | Reference reports on how the original works |
| [`crates/`](crates) | `digger-core` (simulation), `digger-sound` (sound engine), `digger` (the game binary) |
| [`xtask/`](xtask) | Developer tool, run as `cargo xtask <command>` |
| [`web/`](web) | The web page around the WebAssembly build, and its browser tests |
| [`packaging/`](packaging) | Files of the release archives: licence report template, Linux desktop entry, macOS bundle |
| [`docs/manual.md`](docs/manual.md) | The player's manual |
| [`docs/releasing.md`](docs/releasing.md) | How a release is made |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | Toolchain, checks, conventions and workflow for contributors |
| [`openspec/changes/`](openspec/changes) | One OpenSpec change per phase: proposal, design, spec deltas, tasks |
| [`openspec/config.yaml`](openspec/config.yaml) | Project context and rules applied to every change |

## Working with the plan

The plan uses [OpenSpec](https://github.com/Fission-AI/OpenSpec) (spec-driven
schema). With the CLI installed (`npm install -g @fission-ai/openspec`):

```bash
openspec list
```

```bash
openspec show bootstrap-rust-workspace
```

```bash
openspec validate --all --strict
```

In Claude Code, implement a phase with `/opsx:apply <change-name>` and archive
it with `/opsx:archive <change-name>` once its tasks are done. Phases are
applied in the order given in the roadmap.

## Licence

GPL-2.0-or-later, as a derived work of Digger Remastered. The licence text is
in [`LICENSE`](LICENSE); credits and the provenance of the graphics and sound
data are in [`NOTICE`](NOTICE).
