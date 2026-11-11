# Tasks

## 1. Workspace skeleton

- [x] 1.1 Add `rust-toolchain.toml` pinning channel `1.99` with components `rustfmt` and `clippy`; verify `rustc --version` in the repository reports 1.99.x
- [x] 1.2 Add the root `Cargo.toml` with `[workspace]` members `crates/digger-core`, `crates/digger-sound`, `crates/digger`, `xtask`, and `[workspace.package]` (edition 2024, rust-version 1.99, license GPL-2.0-or-later, repository); verify `cargo metadata --no-deps` lists four packages (`xtask` starts as a dependency-free placeholder binary so the member exists; 3.1 turns it into the `clap` tool)
- [x] 1.3 Create library crate `digger-core` with `#![forbid(unsafe_code)]`, a crate-level doc comment stating its no-I/O rule, and one placeholder test; verify `cargo test -p digger-core` passes
- [x] 1.4 Create library crate `digger-sound` with `#![forbid(unsafe_code)]`, a crate-level doc comment, and one placeholder test; verify `cargo test -p digger-sound` passes
- [x] 1.5 Create binary crate `digger` (binary name `digger`) that prints its name and version and exits 0; verify `cargo run -p digger` prints `digger 0.1.0`
- [x] 1.6 Add `.gitignore` (`target/`, editor files) and `.editorconfig` (UTF-8, LF, 4 spaces for Rust, 2 for YAML/TOML/Markdown); verify `git status` is clean after `cargo build`
- [x] 1.7 Commit `Cargo.lock`; verify `cargo build --workspace --locked` succeeds, then run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace`

## 2. Lints and formatting

- [x] 2.1 Add `[workspace.lints]` (rust and clippy tables from design.md) to the root manifest and `[lints] workspace = true` to every member; verify `cargo clippy --workspace --all-targets -- -D warnings` passes
- [x] 2.2 Add `rustfmt.toml` (`edition = "2024"`, `max_width = 100`, `use_field_init_shorthand = true`); verify `cargo fmt --all --check` passes
- [ ] 2.3 Prove the unsafe ban works: temporarily add an `unsafe {}` block to `digger-core`, confirm the build fails with the `unsafe_code` lint, and remove it; record the observed error text in the commit message
- [ ] 2.4 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace`; verify all pass

## 3. xtask

- [ ] 3.1 Create the `xtask` binary crate with `clap` and add `.cargo/config.toml` with the alias `xtask = "run --package xtask --"`; verify `cargo xtask --help` lists subcommands
- [ ] 3.2 Implement `cargo xtask ci` running format check, Clippy with `-D warnings`, tests and `cargo doc` with warnings denied, stopping at the first failure with a non-zero exit; verify it passes on the clean tree and fails when a formatting error is introduced
- [ ] 3.3 Add the dependency-direction check to `cargo xtask ci`: fail if `cargo tree` for `digger-core` or `digger-sound` contains a crate from the platform deny-list; verify with a unit test over sample `cargo tree` output for both a clean and a violating tree
- [ ] 3.4 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace`; verify all pass

## 4. Licence and contributor documentation

- [ ] 4.1 Add `LICENSE` with the unmodified GNU GPL version 2 text; verify its SHA-256 matches the canonical text published by the FSF
- [ ] 4.2 Add `NOTICE` crediting Windmill Software, Andrew Jenner and Maksym Sobolyev and stating that graphics and sound data derive from Digger Remastered (basis: research 07); verify it is referenced from `README.md`
- [ ] 4.3 Add `deny.toml` allowing licences compatible with GPL-2.0-or-later and banning `sdl2`, `sdl3`, `sdl2-sys`, `sdl3-sys`, `cc` and `bindgen`; verify `cargo deny check` passes
- [ ] 4.4 Write `CONTRIBUTING.md`: toolchain setup, `cargo xtask ci`, the conventions from `docs/porting-plan.md` §5, Conventional Commits, and the OpenSpec apply/archive workflow; verify every command in it runs as written
- [ ] 4.5 Update `README.md`: replace the planning-only status with build and test instructions and link `LICENSE`, `NOTICE` and `CONTRIBUTING.md`; verify all links resolve
- [ ] 4.6 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace`; verify all pass

## 5. Continuous integration

- [ ] 5.1 Add `.github/workflows/ci.yml` with the `check` job (format, Clippy, tests, docs) on Ubuntu, Windows and macOS using the pinned toolchain and a cargo cache; verify the workflow file passes `actionlint`
- [ ] 5.2 Add the `deny` job running `cargo deny check`; verify it passes locally with the same command
- [ ] 5.3 Add the `openspec` job running `npx @fission-ai/openspec validate --all --strict`; verify the same command passes locally
- [ ] 5.4 Push the branch and verify all three jobs are green on the first run; fix any platform-specific failure
- [ ] 5.5 Run `cargo xtask ci` locally as the final integration check; verify it exits 0
