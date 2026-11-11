# Digger (Rust)

An idiomatic Rust re-creation of [Digger Remastered](https://digger.org/), the
1983 Windmill Software arcade game as reverse-engineered by Andrew Jenner and
extended in [sobomax/digger](https://github.com/sobomax/digger).

**Status: planning.** There is no game code yet. The repository currently holds
the research, the roadmap and the specifications the implementation will follow.

## Where things are

| Path | Contents |
|---|---|
| [`docs/porting-plan.md`](docs/porting-plan.md) | Roadmap: goals, architecture, conventions, verification strategy, phases |
| [`docs/research/`](docs/research/README.md) | Reference reports on how the original works |
| `openspec/changes/` | One OpenSpec change per phase: proposal, design, spec deltas, tasks |
| `openspec/config.yaml` | Project context and rules applied to every change |

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
added by the first change (`bootstrap-rust-workspace`).
