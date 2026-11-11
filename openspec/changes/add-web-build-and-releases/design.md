# Design

## Context

See `proposal.md` for motivation and `specs/web-build/spec.md` for the
required behaviour. After phase 6 the workspace has `digger-core` and
`digger-sound` (pure, deterministic, no I/O) and the `digger` binary, which
holds everything platform-specific: window and event loop (`winit` 0.30),
presentation (`softbuffer` 0.4), audio output (`cpal` 0.18), gamepads
(`gilrs` 0.11), configuration and score files, command line and frame
pacing. All four platform crates have a browser backend for
`wasm32-unknown-unknown`, so the web build is the same binary compiled for
another target, not a second frontend.

Constraints that shape the design:

- A browser never lets a program block. The desktop frontend already drives
  `Game::step` from event-loop callbacks (the porting plan turned upstream's
  nested blocking loops into a state machine), so nothing in the game has to
  change; only the driver differs. Upstream needed Asyncify to keep its
  blocking loops alive in the browser (research 05 §8.2); this port does not.
- No files, no command line, no process exit, no threads (without
  cross-origin isolation), and audio only after a user gesture.
- Workspace rules: `#![forbid(unsafe_code)]` in every crate, and `deny.toml`
  bans `cc`, `bindgen` and the SDL crates, which also applies to build tools
  written in `xtask`.
- The framebuffer is game state (porting plan §3). Anything the web page adds
  on screen must live in the page, never in the game's pixels.

## Goals / Non-Goals

**Goals:**

- One `digger` binary crate for desktop and web; code that differs per
  platform is confined to a handful of modules with the same interface.
- A web build produced by one command with pinned tools and checked sizes.
- Release archives produced by the same `xtask dist` command locally and in
  CI, with byte-identical output for identical inputs.
- Automated evidence that the page works in three browser engines.

**Non-Goals:**

- Threads, shared memory or an audio worklet in the browser.
- A JavaScript API for embedding the game in other pages.
- Any change to `digger-core` or `digger-sound`.
- Signed or notarised binaries, installers, and PortMaster packages (see
  proposal, Out of scope).

## Decisions

### 1. Target `wasm32-unknown-unknown` with `wasm-bindgen`

The browser backends of `winit`, `softbuffer`, `cpal` and `gilrs` are all
built on `wasm-bindgen` and `web-sys` for this target, so there is no real
choice of binding layer. `rust-toolchain.toml` gains the target.

*Rejected:* `wasm32-unknown-emscripten` — it is how upstream ships its web
build, but it brings a C toolchain and an SDL-shaped runtime, and the chosen
crates do not support it. WASI targets — no browser window or audio.

### 2. Build with plain `wasm-bindgen-cli`, driven by `xtask`; no trunk

`cargo xtask web` runs three steps: `cargo build -p digger --target
wasm32-unknown-unknown --profile web-release --locked`; `wasm-bindgen --target
web --no-typescript` on the result; copy `web/index.html`, `web/style.css` and
`web/boot.js` beside the generated files into `dist/web/`. It then measures
the output against the size budget. `cargo xtask web --serve` additionally
serves `dist/web/` on `127.0.0.1` with a small static server written on
`std::net` (correct MIME type for `.wasm`), which the browser tests also use.

`wasm-bindgen-cli` must be exactly the version of the `wasm-bindgen` library
in `Cargo.lock`. `xtask` reads that version from `Cargo.lock`, compares it
with `wasm-bindgen --version` and, on mismatch, fails with the exact `cargo
install wasm-bindgen-cli --version <v> --locked` command. CI installs it the
same way and caches it.

The `web-release` profile inherits `release` with `opt-level = "s"`,
`lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`.
`wasm-opt` is not used: it is a C++ tool outside the Rust toolchain, and the
budget is expected to be met without it (see Risks).

*Rejected:* trunk — convenient, but it is a second build system with its own
configuration, downloads tool binaries at build time (bad for reproducible
releases), and duplicates what `xtask` must do anyway for size checks and
archives. `wasm-pack` — aimed at publishing libraries to npm.

### 3. No `#[wasm_bindgen]` macro in our code

`wasm-bindgen` runs a binary crate's `main` when the module is initialised,
so the web build needs no exported functions. All browser calls go through
the safe `web-sys` and `js-sys` APIs and `Closure`. The attribute macro
expands to code containing `unsafe`, which `#![forbid(unsafe_code)]` rejects;
by not using it the rule stays in force for the web code too. For the same
reason `wasm-bindgen-test` is not used; browser-level testing is done from
outside with Playwright (decision 12).

### 4. Platform seam: same module names, two implementations

The `digger` crate keeps its application logic (`app`: the `ApplicationHandler`
that owns the `Game`, the presenter, the input mapper and the sound link) free
of `cfg`. Everything target-specific sits under `platform/`, where `mod.rs`
selects `native` or `web` by `cfg(target_arch = "wasm32")` and re-exports the
same names:

| Seam | Shared contract | Native | Web |
|---|---|---|---|
| `launch` | `fn launch_options() -> (LaunchOptions, Vec<LaunchWarning>)` | `clap` over `argv` | pure `parse_query(&str)` over `location.search` |
| `run` | `fn run(app: App) -> Result<(), RunError>` | `EventLoop::run_app`, returns at exit | `EventLoop::spawn_app`, returns at once |
| `Store` | `load(StoreKey) -> Result<Option<String>, StoreError>`, `save(StoreKey, &str) -> Result<(), StoreError>` | files in the config and data directories | local storage keys `digger:settings`, `digger:scores` |
| `AudioOutput` | state machine below | starts in `Running` | starts in `Locked` |
| `FileTransfer` | `export(name, bytes)`, `request_import()`; results arrive as `AppEvent`s | write to the recordings directory; import path from the command line | download; file picker |
| `Shell` | `set_notice(Notice)`, `set_bindings(&Bindings)`, `set_status(&Status)` | log line / no-op | updates page elements |
| time | `Instant` | `std::time` | `web_time` |

`StoreKey` is an enum (`Settings`, `Scores`), not a string, so a caller cannot
invent a storage key. The stored values are the same text the desktop build
writes to its files, so the serialisation code and its tests are shared. Each
seam has a small trait in shared code and an in-memory fake, so `app` logic
(for example "a failed save shows one notice and play continues") is unit
tested on the host without a browser.

Asynchronous results (a picked file, a visibility change, a storage failure
found late) enter the application as variants of one `AppEvent` enum sent
through the `winit` `EventLoopProxy`. `App` handles them in `user_event`
exactly as it handles the native equivalents, so there are no callbacks
reaching into game state and no shared mutable globals: `App` stays the
single owner of everything.

*Rejected:* a separate `digger-web` crate — it would duplicate the
application logic or force it into a third library crate for little gain.
`Box<dyn Platform>` everywhere — dynamic dispatch buys nothing when exactly
one implementation exists per target. `cfg` blocks spread through `app` —
unreadable and untestable.

### 5. Event loop and pacing

Both targets drive the game from `WindowEvent::RedrawRequested`. On the web,
`winit` ties redraw requests to `requestAnimationFrame`, which is the
animation callback the spec requires; the handler asks for the next redraw
before it returns. On the desktop the existing deadline-based wait stays.

The pacing arithmetic is one pure type shared by both: `Pacer { frame_len,
accumulated, last }` with `fn due(&mut self, now: Instant) -> Steps`, where
`Steps` is `Run(n)` with `n` in `0..=4` or `Unthrottled`. It adds the elapsed
time, returns how many whole frame lengths are due, caps at 4 and drops the
rest. Speed 0 yields `Unthrottled`: the driver then steps repeatedly until 8
ms of the callback have been used, which keeps the page responsive during
fast playback. `Pacer::reset(now)` is called when the page becomes visible
and when speed changes, so hidden time is never caught up. The type takes
`now` as an argument and is tested with synthetic timestamps (60 Hz, 144 Hz,
a 2-second stall).

Visibility comes from the document's `visibilitychange` event, forwarded as
`AppEvent::Visibility(bool)`. On hidden, `App` stops stepping, tells
`AudioOutput` to suspend, and, if a live game is in play, feeds the pause
control into the next step so the game enters its own pause phase; no new
pause mechanism is added to the simulation.

*Rejected:* `setTimeout`/`setInterval` at the frame length — throttled
unpredictably in background tabs and not aligned with display. A fixed "one
game frame per N callbacks" rule — wrong on any display that is not 60 Hz.

### 6. Presentation on a canvas

The scale computation (`fit(viewport, picture) -> Scale`) is the desktop
function, moved unchanged into shared code; it returns an integer factor or,
below 640×400, a reduced size. On the web the canvas backing store is set to
the scaled picture size in device pixels, the `softbuffer` surface is that
size, and the CPU upscale of the 640×400 indexed picture is the shared one.
The page's style sheet centres the canvas on a black background, sets its CSS
size to backing size divided by the device pixel ratio, and sets
`image-rendering: pixelated` for the reduced case. Letterboxing is therefore
done by the page, and the canvas never holds anything but game pixels.
Viewport and pixel-ratio changes arrive as `winit` `Resized` and
`ScaleFactorChanged` events, as on the desktop.

### 7. Audio unlock as an ownership hand-over

```
enum AudioOutput {
    Off,                       // quiet option, or sound switched off
    Locked  { synth: Synth },  // web start state: frontend owns the synth
    Running { stream: Stream },// synth moved into the audio callback
    Failed  { reason: AudioError },
}
```

While `Locked`, the frontend owns the `digger-sound` synthesiser, applies
sound commands to it each frame and renders-and-discards the samples for the
elapsed time, so tunes and effects progress silently. On the first pressed
key, pointer button or gamepad button, `App` calls `unlock()`, which consumes
`Locked`, opens the `cpal` output stream and moves the synthesiser into its
callback, giving `Running`; a refusal gives `Failed` and a page notice. The
same event is then processed as ordinary input. Because the synthesiser is
moved, never shared, there is no lock and no "is audio ready" flag.
Gamepad presses are not always counted as a user gesture by browsers; if the
stream fails to start for that reason the state stays `Locked` and the next
gesture tries again. With `quiet`, the state is `Off` and the sound toggle
key leads to `Locked`-then-`unlock()` in one step, since the key press is
itself a gesture.

The prompt is a page element shown while the state is `Locked`; it is set
through `Shell::set_notice`, never drawn into the framebuffer.

In the browser `cpal` runs its callback on the main thread through Web Audio.
The single-producer queue between game and synthesiser works unchanged with
both ends on one thread.

*Rejected:* an audio worklet with shared memory — needs threads, a nightly
`build-std`, and cross-origin isolation headers that plain static hosts such
as GitHub Pages cannot send. Creating the audio context at load and resuming
it later — browsers log an autoplay warning, which the spec forbids.
Dropping sound commands while locked — music would start from the wrong
place or not at all.

### 8. Recordings in and out of the browser

Export: build a `Blob` from the recording bytes, create an object URL, click
a temporary `<a download="NAME">`, revoke the URL. Import: a hidden `<input
type="file" accept=".drf,.edrf">` triggered by the page button; on change,
the size is checked against 16 MiB before reading, the file's
`array_buffer()` promise is awaited with `wasm-bindgen-futures`, and the
bytes are sent as `AppEvent::RecordingPicked { name, bytes }`. Parsing,
format detection (by content, with the name as a hint) and playback are the
existing desktop code paths; a parse error becomes a `Notice`. The `rfd`
crate was considered and rejected: its Linux backends pull in toolkit
dependencies the desktop build does not need, and the browser part is about
forty lines of `web-sys`.

### 9. Launch options from the query string

`parse_query(&str) -> (LaunchOptions, Vec<LaunchWarning>)` is a pure
function compiled and tested on every target. `LaunchOptions` is the struct
`clap` already fills on the desktop, so mode validation (for example
rejecting combinations the simulation does not support) is one shared
function returning `Result<GameConfig, ConfigError>`. Each rejected parameter
is a `LaunchWarning { name, value, reason }` shown through `Shell`; nothing
panics on user-supplied text. Launch options are layered over stored settings
in memory and are never passed to `Store::save`.

### 10. The page

`web/index.html` is hand-written and small: the canvas, a loading element, a
`<noscript>` sentence, a notice area, the sound prompt, buttons (fullscreen,
play recording, save recording), the controls panel and the footer.
`web/boot.js` (about 30 lines, the only hand-written JavaScript) imports the
generated module, shows the failure message if the import or initialisation
rejects, and installs `error`/`unhandledrejection` listeners that show the
internal-error message; with `panic = "abort"` a Rust panic surfaces as such
an error, and a panic hook first writes the message to the console. The page
root carries `data-` attributes maintained through `Shell::set_status`
(`data-phase`, `data-audio`, `data-scale`, `data-frames`, `data-result`);
they double as the observation points for the browser tests. Version and
revision are compiled in from `CARGO_PKG_VERSION` and an `option_env!` value
that `xtask dist` sets; no build script runs `git`.

### 11. Size budget

`xtask web` sums the sizes of `index.html`, `style.css`, `boot.js`, the
generated `.js` and the `.wasm`, uncompressed and after gzip level 9
(`flate2` with its pure-Rust backend), prints both and fails above 2 MiB or
1 MiB. The limits are constants in `xtask` with a comment pointing to the
spec requirement.

### 12. Browser smoke test with Playwright

`web/tests/` holds a pinned `@playwright/test` project (`package.json` and
lock file) with projects for Chromium, Firefox and WebKit; its `webServer`
setting starts `cargo xtask web --serve`. Tests observe the page through the
`data-` attributes, the canvas pixels, `localStorage`, the download event and
the console:

- load: loading indicator goes away, `data-phase="title"`, canvas is not
  uniformly black, no console errors;
- start: a key press changes the phase to playing and `data-frames` advances
  at 12.5 per second within tolerance;
- audio: `data-audio="locked"` and prompt visible before input, `running`
  after a key press, no autoplay warning; `?quiet` shows no prompt;
- pause: a simulated hidden state stops `data-frames`; visible again shows
  the paused phase;
- storage: a setting survives reload; a corrupt `digger:settings` value gives
  defaults; storage disabled shows the notice;
- recordings: a `.drf` from `tests/recordings/` set through the file input
  with `?speed=0` ends with `data-result` equal to the expected score, level
  and ticks; a played game produces a download that parses as a recording;
- query: an invalid `speed` shows the warning.

Pull requests run the Chromium project; pushes to `main` and release tags run
all three. Playwright is test tooling in Node.js and adds nothing to the
shipped files.

*Rejected:* `wasm-bindgen-test` (decision 3); manual testing only (the
supported-browser requirement asks for an automated check).

### 13. Release workflow

`.github/workflows/release.yml` runs on tags matching `v[0-9]+.[0-9]+.[0-9]+*`.

| Job | Runner | Output |
|---|---|---|
| `verify` | `ubuntu-24.04` | tag equals `v` + workspace version; `cargo xtask ci`; `cargo deny check` |
| `linux-x86_64` | `ubuntu-22.04` | `digger-<version>-linux-x86_64.tar.gz` |
| `linux-aarch64` | `ubuntu-22.04-arm` | `digger-<version>-linux-aarch64.tar.gz` |
| `windows-x86_64` | `windows-2022` | `digger-<version>-windows-x86_64.zip` |
| `macos-universal` | `macos-14` | `digger-<version>-macos-universal.tar.gz` |
| `web` | `ubuntu-24.04` | `digger-<version>-web.tar.gz`; all-engine Playwright run |
| `publish` | `ubuntu-24.04` | `SHA256SUMS`; draft GitHub release; Pages deployment |

Every build job runs `cargo xtask dist --platform <name> --version <from
tag>` and uploads the one archive. `publish` downloads them, writes
`SHA256SUMS` (one `sha256  filename` line each, sorted), creates a **draft**
release, as upstream does (research 05 §8.3), and deploys the unpacked web
archive with the Pages actions. A maintainer reviews and publishes the draft.
Tags with a pre-release suffix skip the Pages deployment.

The version has one source of truth in the tree, `[workspace.package]
version`; the tag selects what is released and `verify` refuses a tag that
disagrees with it. Binaries report `CARGO_PKG_VERSION`.

**Linux aarch64 is built natively, not cross-compiled.** On Linux `cpal`
links the system ALSA library and `gilrs` links libudev, so a cross build
needs those libraries for the target architecture in a sysroot. GitHub's
hosted arm64 Linux runners avoid that entirely: the job is the x86_64 job
with a different runner label. Both Linux jobs install `libasound2-dev` and
`libudev-dev` and use Ubuntu 22.04, which sets the minimum glibc to 2.35;
the manual states this floor.

*Rejected:* `cross` — its crates.io release is stale and its current image
would need a custom Dockerfile for the two libraries (research 07 §C.2); it
stays the documented fallback if arm64 runners are unavailable.
`cargo-zigbuild` — fine for glibc pinning but still needs the target
libraries. QEMU emulation of the whole build — several times slower.

**macOS universal** is built on one arm64 runner: `xtask dist` builds
`aarch64-apple-darwin` and `x86_64-apple-darwin` (the Apple SDK supports both
from either host), joins them with `lipo -create`, and re-applies an ad-hoc
signature with `codesign --force --sign -`, which arm64 macOS requires even
for unsigned software. `MACOSX_DEPLOYMENT_TARGET` is 11.0. The result is
placed in a minimal `Digger.app` bundle (`Info.plist` and an `.icns` made
from the game icon, both under `packaging/macos/`), so starting it does not
open a terminal. The manual explains the first-launch Gatekeeper step.

**Windows** uses the MSVC target with the C runtime linked statically
(`-C target-feature=+crt-static` for that target in `.cargo/config.toml`), so
the archive needs no redistributable.

### 14. Archive layout and reproducible `xtask dist`

Every archive has a single top-level directory named like the archive:

```
digger-<version>-<platform>/
  digger | digger.exe | Digger.app/     the program
  LICENSE                               GNU GPL version 2 text
  NOTICE                                credits and asset provenance
  THIRD-PARTY-LICENSES.html             licences of all linked crates
  MANUAL.md                             docs/manual.md
  README.md
  digger.desktop, digger.png            Linux only
```

The web archive has the same documents next to a `site/` directory that is
exactly what is deployed.

`THIRD-PARTY-LICENSES.html` is generated by `cargo about generate` with the
template in `packaging/about.hbs`, per target, so each archive lists what
that binary links.

`xtask dist` writes archives itself (`tar` + `flate2`, `zip`, `sha2`; all
pure Rust, as `deny.toml` requires) rather than calling system `tar` or
`zip`, so it behaves the same on all three host systems and can be
deterministic: entries sorted by path, modification times set to
`SOURCE_DATE_EPOCH` (the tag's commit time; default the `HEAD` commit time),
owner and group 0, modes fixed at 0755 for the program and 0644 otherwise, no
gzip header timestamp. Builds use `--locked` and
`--remap-path-prefix` for the workspace and cargo home, so paths do not leak
into binaries. "Reproducible" is claimed and tested for the same toolchain
and runner image: a CI job builds the Linux x86_64 and web archives twice in
different directories and compares the SHA-256 values. It is not claimed for
macOS, where the signature step is outside our control.

### 15. Licence gate

`deny.toml` (from phase 1) gains a `[graph] targets` list naming the five
release targets, including `wasm32-unknown-unknown`, so licences and bans are
checked for exactly the crates that ship, including the browser-only ones.
Most dependencies are dual MIT/Apache-2.0 and are taken under MIT. A crate
available only under Apache-2.0 is compatible with GPL version 3 but not
version 2; the allow-list permits it with a comment, and `docs/releasing.md`
records that binaries containing such a crate are conveyed under the "or
later" option. The release `verify` job fails on any licence outside the
allow-list.

### 16. How the design stays idiomatic Rust

- **Ownership instead of globals.** Upstream's browser glue calls exported C
  functions that read global state (`Module._digger_get_keymap`, research 05
  §8.4). Here `App` owns the game and all platform objects; the page is
  updated by `App` pushing typed values through `Shell`, and the browser
  reaches the application only by sending `AppEvent`s.
- **Enums and state machines instead of flags.** `AudioOutput`, `Steps`,
  `AppEvent`, `Notice`, `StoreKey`, `Platform` (the `xtask dist` argument)
  are closed enums; the audio unlock is a consuming state transition.
- **No blocking.** The web driver returns to the browser after every
  callback; there is no Asyncify and no sleep.
- **Errors as values.** `StoreError`, `AudioError`, `TransferError`,
  `LaunchWarning` and the existing recording errors are enums carried to one
  place (`App::report`) that turns them into a page notice or, natively, a
  log line. No `unwrap` on anything from the query string, storage or a
  picked file. `xtask` may use `expect` as agreed in phase 1.
- **Distinct types.** Device pixels and CSS pixels are separate newtypes in
  the web presenter; frame counts and durations are not mixed in `Pacer`.

### 17. Compatibility quirks

This change adds no simulation code, so it has no simulation-affecting quirk
to reproduce. Four documented behaviours touch it and are kept visible:

| Behaviour | Class | Isolation | Test |
|---|---|---|---|
| Speed and pacing never change game results (research 05 §6.3) | Simulation invariant | `Pacer` only decides when `Game::step` is called, never what it is given | Playwright replay of an upstream recording at `speed=0` matches score, level and ticks; `pacer_*` unit tests |
| The picture is game state (porting plan §3) | Simulation invariant | Prompt, notices and letterbox are page elements; the canvas holds only game pixels | Playwright replay with the prompt visible still matches; canvas backing size test |
| Exit from the title screen does not quit in a browser (research 05 §8.4) | Presentation, kept from upstream's web build | `run` on web never exits; the frontend ignores the game's exit outcome and re-enters the title | `web_exit_stays_on_title` (host test with the web policy) and a Playwright check |
| Sound can be switched on after a quiet start (upstream bug, fixed in phase 6) | Presentation fix | `AudioOutput::Off -> Locked -> Running` on the toggle key | `audio_quiet_then_toggle_unlocks` host test; Playwright `?quiet` test |

`docs/compat-quirks.md` gets a short "web build" section recording the third
row and the fact that the hidden-tab pause uses the game's own pause phase.

## Risks / Trade-offs

- [The size budget is missed without `wasm-opt`] → Measure in the first task
  group, before the page is built. Levers in order: trim `web-sys` features,
  `opt-level = "z"`, drop unused `winit`/`cpal` default features. Only if
  those fail, add `wasm-opt` as a pinned release-only step and say so in
  `docs/releasing.md`.
- [A needed browser call has no `web-sys` binding, tempting use of the
  `#[wasm_bindgen]` macro] → Use `js_sys::Reflect` and `js_sys::Function`,
  which are safe, or put the few lines in `boot.js`.
- [Main-thread audio crackles when the tab is busy] → A game step costs well
  under a millisecond; the output buffer is sized at about 100 ms on the web.
  Accepted as a known limit; the worklet route is recorded as rejected above.
- [`requestAnimationFrame` timing differs between engines and is throttled
  for occluded windows] → `Pacer` works from timestamps, not callback counts,
  and caps catch-up; the three-engine smoke test checks frame rate.
- [Gamepad presses do not unlock audio in some browsers] → Retry on every
  gesture while `Locked`; the prompt text mentions key press or click.
- [Hosted arm64 runners become unavailable or change image] → Documented
  `cross` fallback; the job differs from x86_64 only by runner label.
- [Unsigned macOS and Windows downloads trigger operating-system warnings] →
  Documented in the manual; signing is out of scope.
- [Playwright browser downloads make CI slow or flaky] → Cache browsers by
  Playwright version; only Chromium on pull requests; retries limited to 1 so
  real failures stay visible.
- [Local storage is per origin, so moving the site loses settings and scores]
  → Accepted; recordings can be downloaded and kept by the player.
- [Glibc 2.35 floor excludes older Linux systems] → Stated in the manual;
  building from source remains possible.

## Migration Plan

Nothing existing is replaced. Order of introduction: platform seam refactor
with desktop behaviour unchanged (all phase 6 tests stay green), web
implementations, page and tests, then distribution. The first release is cut
from a pre-release tag (`v0.1.0-rc.1`) to exercise the workflow without
deploying the site; the draft release is inspected and deleted or published.
Rollback of a bad release is deleting the draft or, if published, publishing
a new patch tag; the Pages site is redeployed from the previous tag by
re-running that tag's workflow.

## Open Questions

- Whether to request persistent storage from the browser so scores are not
  evicted under storage pressure. It changes no requirement and can be added
  later.
- Whether a custom domain is used for the site. All paths are relative, so
  the choice does not affect the build.
