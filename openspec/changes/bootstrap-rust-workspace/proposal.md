# Proposal

## Why

The repository holds only research and plans. Every later phase of the Rust
port (see `docs/porting-plan.md` §7) needs a Cargo workspace, a pinned
toolchain, agreed lint and formatting settings, continuous integration and a
licence before any game code can be written and reviewed consistently.

## What Changes

- Add a Cargo workspace (resolver 3, edition 2024) with four empty members:
  libraries `digger-core` and `digger-sound`, binary `digger`, and developer
  tool `xtask`.
- Pin the toolchain to Rust 1.99 with `rust-toolchain.toml` and declare
  `rust-version = "1.99"` for every crate.
- Set workspace-wide lints: `unsafe_code = "forbid"`, a curated Clippy set
  denied in CI, and a `rustfmt.toml`.
- Enforce the dependency direction: `digger-core` and `digger-sound` depend on
  no platform crate; a CI check fails if they gain one.
- Add the `cargo xtask` alias and an `xtask` skeleton with a `ci` subcommand
  that runs the same checks as CI.
- Add GitHub Actions CI on Linux, Windows and macOS: format check, Clippy with
  warnings denied, tests, documentation build, `cargo deny`, and an OpenSpec
  validation job.
- Add `LICENSE` (GPL-2.0-or-later), a `NOTICE` crediting Windmill Software,
  Andrew Jenner and Maksym Sobolyev, `.gitignore`, `.editorconfig`, and a
  `CONTRIBUTING.md` describing the conventions and the OpenSpec workflow.

Out of scope: any game behaviour. Assets and rendering start in
`add-assets-and-renderer`; release builds and packaging are in
`add-web-build-and-releases`.

## Capabilities

### New Capabilities

None. This change is tooling and repository structure only and changes no game
behaviour, so it sets `skip_specs: true`.

### Modified Capabilities

None.

## Impact

- New files at the repository root (`Cargo.toml`, `rust-toolchain.toml`,
  `rustfmt.toml`, `deny.toml`, `LICENSE`, `NOTICE`, `CONTRIBUTING.md`,
  `.gitignore`, `.editorconfig`, `.cargo/config.toml`), `crates/`, `xtask/`
  and `.github/workflows/ci.yml`.
- Contributors need Rust 1.99; `rustup` installs it automatically from
  `rust-toolchain.toml`.
- The repository becomes GPL-2.0-or-later.
- No runtime dependencies are added yet; later changes add them to the
  workspace dependency table introduced here.
