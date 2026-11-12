# Contributing

Thank you for helping re-create Digger in Rust. This file covers setting up
the toolchain, the checks every commit must pass, the code conventions and the
OpenSpec workflow the project is planned with. The roadmap and the reasons
behind these rules are in [`docs/porting-plan.md`](docs/porting-plan.md).

By contributing you agree that your contribution is licensed under the
GNU General Public License, version 2 or (at your option) any later version
(see [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE)).

## Toolchain

The toolchain is pinned to Rust 1.99 in `rust-toolchain.toml`, together with
the `rustfmt` and `clippy` components. Install [rustup](https://rustup.rs/);
the first `cargo` command run inside the repository then installs the pinned
toolchain automatically. To install it explicitly and confirm it is active:

```bash
rustup toolchain install
```

```bash
rustc --version
```

On Linux the desktop frontend added in later phases needs the development
packages for ALSA, libudev, xkbcommon and Wayland (on Debian or Ubuntu:
`libasound2-dev`, `libudev-dev`, `libxkbcommon-dev`, `libwayland-dev`).
Running the game under X11 also needs `libxkbcommon-x11-0`, which winit
loads at run time; without it winit stops with a panic naming the library.
Setting `DIGGER_FRAME_COUNTER=1` prints how many frames have run every ten
seconds, for checking the frame pacing against a stopwatch (750 frames a
minute at the default speed).

## Building and testing

```bash
cargo build --workspace
```

```bash
cargo test --workspace
```

## The checks every commit must pass

`cargo xtask` is a cargo alias (`.cargo/config.toml`) for the developer tool in
`xtask/`. `cargo xtask ci` runs the same checks as continuous integration, in
order, and stops at the first failure:

```bash
cargo xtask ci
```

It runs, in this order:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS=-D warnings`
5. the dependency-direction check: `digger-core` and `digger-sound` must not
   depend, on any target, on a platform crate (`winit`, `softbuffer`, `cpal`,
   `gilrs`, `directories`).

CI additionally runs `cargo deny check` (licences compatible with
GPL-2.0-or-later, banned crates such as SDL bindings, `cc` and `bindgen`,
security advisories; configured in `deny.toml`) and validates the OpenSpec
artifacts. To run them locally:

```bash
cargo install --locked cargo-deny
```

```bash
cargo deny check
```

```bash
npx @fission-ai/openspec validate --all --strict
```

Never weaken, skip or delete a test or lint to get a green build. Where a
lint must be silenced at one site, use `#[expect(lint, reason = "...")]` so
the exemption is reported once it is no longer needed.

## Hard rules

- **No C.** Nothing from the upstream C program is linked, vendored, called
  through FFI, run or mechanically translated, and no C platform library (no
  SDL) is used. Upstream data tables and recordings may be read as data.
- Platform crates are Rust-only: `winit`, `softbuffer`, `cpal`, `gilrs`.
- `digger-core` has no I/O, clock, threads or platform dependencies and is
  deterministic.
- Fidelity is proven by replaying the upstream recordings, not by resembling
  the original code. Simulation-affecting quirks of the original are
  reproduced deliberately and recorded in `docs/compat-quirks.md`, each with a
  test.

## Code conventions

From [`docs/porting-plan.md`](docs/porting-plan.md) §5:

- `#![forbid(unsafe_code)]` in every crate.
- No global mutable state: one owning `Game` value; subsystems borrow from it.
- Closed sets are enums (`Direction`, `DeathStage`, `BagState`, `GameMode`,
  `SoundCommand`); "no direction" is `Option<Direction>` or an explicit
  variant, never `-1`.
- Distinct units are distinct types (cell coordinates versus pixel
  coordinates, ticks versus frames, sprite ids).
- Arithmetic that must wrap or truncate to match the original says so
  (`wrapping_add`, `as i16` behind a named helper with a comment citing the
  quirk); everything else uses checked or plain arithmetic.
- Fallible operations return `Result` with a crate error enum; no `unwrap` or
  `expect` on input-derived data; parsers never panic on malformed files.
- Fixed-size game tables are arrays with typed indices; no heap allocation
  inside `Game::step`.
- Tests sit next to the code (`#[cfg(test)]`) for units and under `tests/` for
  scenarios and replays.
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and
  `cargo test --workspace` pass on every commit.

## Commit messages

Commits follow [Conventional Commits](https://www.conventionalcommits.org/):
`type(scope): summary`, for example `feat(core): carve tunnels behind the
digger`, `test(core): cover the bag fall timer`, `fix(sound): ...`,
`docs: ...`, `ci: ...`, `build: ...`. Scopes are crate names without the
`digger-` prefix (`core`, `sound`, `app`) or `xtask`. Each completed OpenSpec
task group is at least one commit; one commit per coherent step is preferred.

## Planning with OpenSpec

Work is planned with [OpenSpec](https://github.com/Fission-AI/OpenSpec) using
the spec-driven schema. Each phase of the roadmap is one change under
`openspec/changes/<change-name>/` with a `proposal.md`, a `design.md`, spec
deltas under `specs/` and a `tasks.md`. Project-wide context and rules live in
`openspec/config.yaml`. Inspect the plan with the CLI:

```bash
npx @fission-ai/openspec list
```

```bash
npx @fission-ai/openspec show bootstrap-rust-workspace
```

### Applying a change

Changes are applied in the order of the roadmap (`docs/porting-plan.md` §7).
In Claude Code, run `/opsx:apply <change-name>`; by hand, work through
`tasks.md` the same way:

1. Read the change's proposal, design and specs, and the `docs/research/`
   sections they cite.
2. Do the tasks of one group in order. Tick a task (`- [x]`) only after its
   stated verification has actually passed.
3. When a question or spec conflict comes up, re-check the change's specs and
   design, the earlier changes, `docs/research/`, and then the provided
   sources (the upstream sobomax/digger source and recordings, digger.org,
   portmaster.games) to confirm the behaviour; then write the code
   independently. A recording beats a report. Record the resolution in the
   same commit: in `docs/compat-quirks.md` if it is game behaviour, or by
   correcting the change's own artifacts if the plan was wrong.
4. Run `cargo xtask ci` and commit the group.

### Archiving a change

Once every task of a change is ticked, archive it with `/opsx:archive
<change-name>` in Claude Code, or with the CLI:

```bash
npx @fission-ai/openspec archive <change-name>
```

Archiving moves the change to `openspec/changes/archive/` and merges its spec
deltas into `openspec/specs/`. Validate afterwards:

```bash
npx @fission-ai/openspec validate --all --strict
```
