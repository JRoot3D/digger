# Upstream exploration reports

Reference material gathered before planning the Rust port. Each report was
written by reading the upstream C source of Digger Remastered
([sobomax/digger](https://github.com/sobomax/digger)) at commit
`638125eecc2a36629b886f58d7b75253894cf3d3` (2026-10-14), or the public web
pages named in report 07. Nothing in the reports was compiled or run, so every
behavioural claim is from code reading and should be confirmed by the
conformance suite as the port progresses.

`file:line` references point into the upstream tree at that commit.

| Report | Area |
|---|---|
| [01-shell-world.md](01-shell-world.md) | Startup, options, game loop, timing, levels, playfield, scoring, RNG, state hash |
| [02-entities.md](02-entities.md) | Digger, monsters, gold bags, fireballs, collisions, per-tick update order |
| [03-graphics.md](03-graphics.md) | Video contract, sprite and font data, sprite engine, palettes, title screen |
| [04-sound.md](04-sound.md) | Sound effects, music, sample synthesis, timing |
| [05-input-record-build.md](05-input-record-build.md) | Input, INI, DRF and eDRF recordings, timers, tests, build, CI |
| [06-netsim.md](06-netsim.md) | Networked play (not part of the port; kept for a possible later proposal) |
| [07-web-research.md](07-web-research.md) | digger.org, the PortMaster port, Rust crate survey, licensing |

## How to read them

The reports describe **what the original does**. They are the behavioural
reference for the port, not a structure to copy.

Each report ends with a "suggested Rust design" section. Those sections were
written before the project decisions were taken and are superseded wherever
they disagree with [`docs/porting-plan.md`](../porting-plan.md) and the
OpenSpec changes under `openspec/changes/`. In particular, the port:

- is idiomatic Rust written from the behavioural description, with no C code
  linked, vendored or mechanically translated;
- uses Rust-only platform libraries (no SDL);
- leaves networked play out.
