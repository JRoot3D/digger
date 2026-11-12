# Digger (Rust)

An idiomatic Rust re-creation of [Digger Remastered](https://digger.org/), the
1983 Windmill Software arcade game as reverse-engineered by Andrew Jenner and
extended in [sobomax/digger](https://github.com/sobomax/digger).

**Status: early development.** The Cargo workspace, toolchain pin, lints, CI
and developer tooling are in place; the game itself is built phase by phase
following the roadmap in [`docs/porting-plan.md`](docs/porting-plan.md). There
is nothing playable yet.

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
