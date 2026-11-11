# Design

## Context

See `proposal.md` for motivation and `docs/porting-plan.md` §4–§5 for the
architecture and conventions this change puts in place. The repository
currently has no Rust code. The sandbox and CI runners have `rustup`, so a
pinned toolchain file is enough to select Rust 1.99.

## Goals / Non-Goals

**Goals:**

- A workspace where `cargo build`, `cargo test`, `cargo fmt --check` and
  `cargo clippy` succeed on an empty tree, so later changes start from green.
- Conventions enforced by tools rather than by review.
- The "core has no platform dependencies" rule checked mechanically.

**Non-Goals:**

- Choosing versions of game dependencies (each later change adds its own).
- Release automation and cross-compilation.

## Decisions

### Workspace layout

```
Cargo.toml                 [workspace] members, [workspace.package],
                           [workspace.dependencies], [workspace.lints]
rust-toolchain.toml        channel = "1.99", components = rustfmt, clippy
rustfmt.toml
deny.toml
.cargo/config.toml         [alias] xtask = "run --package xtask --"
crates/digger-core/        lib; #![forbid(unsafe_code)]; no dependencies
crates/digger-sound/       lib; #![forbid(unsafe_code)]; no dependencies
crates/digger/             bin "digger"; prints version for now
xtask/                     bin; subcommand `ci`
```

`[workspace.package]` carries `edition = "2024"`, `rust-version = "1.99"`,
`license = "GPL-2.0-or-later"`, `repository` and `authors`; members inherit
with `field.workspace = true`. Resolver 3 is implied by edition 2024 and makes
dependency resolution respect `rust-version`.

*Alternative considered:* a single crate with modules. Rejected: the
"simulation has no I/O" rule is the central architectural constraint and a
crate boundary is the only thing the compiler enforces.

### Toolchain pin

`rust-toolchain.toml` pins `channel = "1.99"` (any 1.99.x patch release).
`rust-version = "1.99"` documents the minimum and lets resolver 3 refuse
dependencies that need something newer.

*Alternative considered:* tracking `stable`. Rejected: the target was given as
1.99, and an unpinned channel makes CI results depend on the day they run.

### Lints

`[workspace.lints]` in the root manifest, inherited by every member with
`[lints] workspace = true`:

- `rust`: `unsafe_code = "forbid"`, `missing_debug_implementations = "warn"`,
  `unreachable_pub = "warn"`.
- `clippy`: `all` and `pedantic` at warn with a short allow-list
  (`module_name_repetitions`, `must_use_candidate`, `missing_errors_doc` until
  public APIs exist), plus `unwrap_used`, `expect_used`, `panic`,
  `indexing_slicing` and `cast_possible_truncation` at warn. The last two
  matter here: the port has many deliberate 16-bit truncations and table
  lookups, and each must be visible and justified at its site with an
  `#[expect(..., reason = "...")]` attribute.
- CI runs Clippy with `-D warnings`, so warn means deny on merge while local
  builds stay usable.

`#![forbid(unsafe_code)]` is also written in each crate root so the rule is
visible where the code is read.

### Dependency-direction check

`cargo xtask ci` runs `cargo tree -p digger-core -e normal --prefix none --target all`
(every target, so a platform crate behind a `cfg(windows)` table is caught on
any host) and
the same for `digger-sound`, and fails if any crate from a deny-list of
platform crates (`winit`, `softbuffer`, `cpal`, `gilrs`, `directories`)
appears. The deny-list lives in one constant in `xtask`. Direct use of
`std::fs`, `std::time` or `std::thread` inside the two libraries cannot be
caught this way and is left to review. `deny.toml` additionally bans `sdl2`, `sdl3`, `cc` and
`bindgen` workspace-wide, which mechanically enforces "no C".

*Alternative considered:* making the core `#![no_std]`. Rejected for now: it
would enforce the rule perfectly but costs ergonomics (`Vec` for recordings,
error traits) for no user-visible benefit. It can be revisited once the core
exists.

### xtask instead of shell scripts

Developer commands are Rust (`cargo xtask <cmd>`), so they run identically on
Linux, Windows and macOS and later changes can add `import-assets`,
`conformance` and `dist` beside `ci`. `xtask` uses `clap` for argument parsing
and `std::process::Command` to call cargo; it is the only crate allowed to
use `expect` freely.

### Continuous integration

One workflow, `.github/workflows/ci.yml`, triggered on push and pull request:

| Job | Runner | Steps |
|---|---|---|
| `check` | ubuntu-latest, windows-latest, macos-latest | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`; `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS=-D warnings` |
| `deny` | ubuntu-latest | `cargo deny check` (licences compatible with GPL-2.0-or-later, bans, advisories) |
| `openspec` | ubuntu-latest | `npx @fission-ai/openspec validate --all --strict` |

The toolchain comes from `rust-toolchain.toml`; a cargo cache action keyed on
`Cargo.lock` keeps runs short. `Cargo.lock` is committed because the workspace
ships a binary.

### Licence

`LICENSE` is the unmodified GNU GPL version 2 text. Crate manifests carry the
SPDX expression `GPL-2.0-or-later`. `NOTICE` records the lineage (Windmill
Software 1983; Andrew Jenner's Digger Remastered; Maksym Sobolyev's
continuation) and that graphics and sound data are derived from Digger
Remastered. Research report 07 documents the basis.

## Risks / Trade-offs

- [Pedantic Clippy produces noise on game-table code] → The allow-list is in
  one place and may grow by pull request with a stated reason; individual
  sites use `#[expect]` so stale exemptions are reported.
- [Rust 1.99 is not preinstalled everywhere] → `rustup` reads the toolchain
  file and installs it; CONTRIBUTING documents this.
- [Three-OS CI is slow for an empty workspace] → Accepted; the cost is seconds
  now and the matrix catches platform issues as soon as the frontend lands.
- [The `openspec` CI job depends on npm availability] → It is a separate job
  and is not a required check for code changes.
