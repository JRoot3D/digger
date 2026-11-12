# Proposal

## Why

After phase 6 the game is playable only by people who build it from source on
a desktop. The porting plan (`docs/porting-plan.md` §2 goal 4 and §7 phase 7)
ends with a browser build and downloadable releases, so that the game can be
tried from a link and installed without a Rust toolchain. Upstream already
ships a browser build and tagged release archives (research 05 §8.3–§8.4);
the port should not offer less.

## What Changes

- Add a browser build of the `digger` binary for the WebAssembly target. The
  simulation, renderer and sound crates are used unchanged; the binary's
  platform modules (event loop driver, storage, audio output, file transfer,
  launch options) gain web implementations next to the desktop ones.
- Add a static page (`web/`) that hosts the game: canvas, loading and error
  states, a sound prompt, fullscreen and recording buttons, a controls panel,
  and a footer with version, licence and source link.
- Browser-specific behaviour, specified in the new `web-build` capability:
  integer-scaled canvas, keyboard and gamepad input, audio that starts only
  after a user gesture, pacing from the browser's animation callback at the
  game's own frame length, pause when the tab is hidden, settings and high
  scores in local storage, recordings saved as downloads and loaded through a
  file picker, start-up options from URL query parameters, a size budget, and
  a stated set of supported browsers.
- Add `cargo xtask web` (build and serve the page locally) and
  `cargo xtask dist` (build one release archive reproducibly, for a named
  platform or for the web).
- Add a release workflow that runs on version tags and produces archives for
  Linux x86_64, Linux aarch64, Windows x86_64 and macOS universal, a web
  archive, and a `SHA256SUMS` file, attaches them to a draft GitHub release
  and deploys the web build as a static site. Each archive contains the
  licence, the notice file, third-party licence texts and the manual. The
  version comes from the tag and must equal the workspace version.
- Make the licence check a release gate: `cargo deny check` covers the
  dependency graphs of every release target, including the WebAssembly one.
- Add a headless-browser smoke test (Playwright) to CI, and a WebAssembly
  build with the size-budget check to the pull-request workflow.
- Write the player manual (`docs/manual.md`) and release instructions
  (`docs/releasing.md`).

No existing behaviour changes; nothing is **BREAKING**.

### Out of scope

This is the last change of the plan, so nothing below is picked up by a later
change. Each item would need a new proposal.

- **PortMaster handheld packaging.** It is a non-goal of the porting plan
  (§2). Those devices give a program only SDL2 on KMS/DRM, with no X11 or
  Wayland session (research 07 §B.2, §B.7, §C.2); the Rust-only window stack
  this port uses needs one of those and cannot open a display there. A later
  proposal would have to add a second frontend that links the firmware's
  SDL2 (which means amending the "no C platform libraries" decision), or a
  new Rust frontend on DRM/KMS, evdev and ALSA directly; build for aarch64
  against glibc 2.30; run with no arguments, full screen, on screens from
  480×320 upwards with D-pad-only control; keep its files in the port folder;
  and supply the PortMaster package files (research 07 §B.6–§B.7). The
  frontend boundary makes this possible without touching game code.
- Touch controls and phone-sized layouts; the web build needs a keyboard or a
  gamepad.
- Offline installation of the web build (service worker, web app manifest).
- Code signing and notarisation, installers (`.msi`, `.pkg`, `.deb`, Flatpak)
  and distribution through package managers or crates.io.
- Windows on aarch64 and 32-bit targets.
- Networked play, which stays out of the whole plan.

## Capabilities

### New Capabilities

- `web-build`: how the game behaves when it runs in a browser from a static
  page — presentation, input, audio unlock, pacing, pause on hidden tab,
  persistence, recording transfer, start-up options, size budget and
  supported browsers.

### Modified Capabilities

None. The desktop capabilities (`desktop-app`, `input-controls`,
`configuration`, `high-scores`) keep their requirements; the web build reuses
their rules and `web-build` states only what differs in a browser. Release
distribution is tooling with no game behaviour and therefore has no spec.

## Impact

- `crates/digger`: platform modules split into native and web
  implementations; new dependencies for the WebAssembly target only
  (`wasm-bindgen`, `web-sys`, `js-sys`, `wasm-bindgen-futures`, `web-time`);
  `clap` and `directories` become native-only dependencies. `digger-core` and
  `digger-sound` are not changed.
- `xtask`: new `web` and `dist` subcommands and their pure-Rust dependencies
  (`tar`, `flate2`, `zip`, `sha2`).
- New directories and files: `web/` (page, style sheet, loader script,
  Playwright tests), `packaging/` (macOS bundle metadata, Linux desktop
  entry, licence-report template), `docs/manual.md`, `docs/releasing.md`,
  `.github/workflows/release.yml`.
- Changed files: `.github/workflows/ci.yml` (web build, size budget, browser
  smoke test), `deny.toml` (target list), root `Cargo.toml` (web release
  profile, dependencies), `rust-toolchain.toml` (extra targets), `README.md`.
- Build tools needed by CI and by anyone building the web version:
  `wasm-bindgen-cli` (same version as the library in `Cargo.lock`),
  `cargo-deny`, `cargo-about`, and Node.js with Playwright for the smoke test.
- The repository gains a public static site (GitHub Pages) and release
  downloads; both are produced only from version tags.
