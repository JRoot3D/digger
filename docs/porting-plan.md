# Digger in Rust — porting plan

This is the roadmap for re-creating Digger Remastered as an idiomatic Rust
program. It records the project-wide decisions and the order of work. The
detailed, reviewable units of work are the OpenSpec changes under
`openspec/changes/`; each has its own proposal, design, spec deltas and task
list and is implemented with `/opsx:apply <change>`.

## 1. What is being ported

| | |
|---|---|
| Reference implementation | [sobomax/digger](https://github.com/sobomax/digger) at commit `638125eecc2a36629b886f58d7b75253894cf3d3` (2026-10-14), about 35,000 lines of C |
| Lineage | Windmill Software's 1983 DOS game, reverse-engineered by Andrew Jenner as Digger Remastered ([digger.org](https://digger.org/)), then extended by Maksym Sobolyev |
| Behavioural reference | The reports in [`docs/research/`](research/README.md) |
| Target toolchain | Rust 1.99 (stable), edition 2024 |

The reference build's live game mode is "VGA": a 640×400 16-colour picture
addressed by the game logic as 320×200. The CGA mode is dead code upstream
(every surviving backend draws nothing for it) and is not ported.

## 2. Goals and non-goals

**Goals**

1. A complete, playable Digger: one player, two players alternating, two
   diggers simultaneously on one keyboard, gauntlet mode, all eight level
   layouts, bonus mode, high scores, sound effects and music, pause, key
   redefinition, gamepad support.
2. Behavioural fidelity that can be proven: the port replays the upstream
   `.drf` and `.edrf` recordings and arrives at the same scores, level, tick
   count and intermediate state checkpoints.
3. Idiomatic Rust throughout: ownership instead of global state, enums and
   state machines instead of flag integers and blocking loops, `Result`
   instead of sentinel values, no `unsafe`.
4. Runs on Linux, Windows and macOS desktops, with a browser build as the
   last phase.

**Non-goals**

- **No C.** No upstream C is linked, vendored, called through FFI or
  mechanically translated. The code is written from the behavioural
  description.
- **No C platform libraries.** Window, input and audio use Rust-only crates;
  SDL is not used.
- **No networked play.** Upstream's NetSim subsystem is left out. Report 06 is
  kept so it can be proposed later.
- **No CGA mode**, no X11 window embedding, no FreeBSD console backend.
- **No compatibility with upstream's configuration or score files.** The port
  has its own formats. Recordings are the one upstream format it reads and
  writes.
- **PortMaster handhelds are not a target of this plan.** Those devices offer
  only SDL2 on KMS/DRM, which the Rust-only window stack cannot drive. The
  frontend boundary (section 4) is drawn so that another frontend could be
  added later without touching game code.

## 3. Guiding principle: same behaviour, new structure

The original is a 1983 design: global variables, blocking loops nested several
deep, drawing calls that double as game logic. The port keeps **what the game
does** and replaces **how the program is organised**.

Three facts about the original make "same behaviour" stricter than it sounds,
and shape the architecture:

1. **The picture is game state.** A fireball decides whether it has hit earth
   by reading pixels back from the screen. The simulation therefore owns a
   pixel-exact indexed framebuffer and renders into it even when nothing is
   displayed.
2. **Sprite bookkeeping is game state.** All collisions come from the sprite
   layer's bounding-box lists, computed at specific draw calls. The sprite
   layer is part of the simulation, not of the display.
3. **One counter couples everything.** A "penalty" counter, bumped by drawing
   and scoring calls, delays monsters; monster timing decides how many random
   numbers are drawn. A single missed increment desynchronises a replay.

Fidelity is therefore tested from the outside, by replaying recordings, and
never by resemblance of the code.

### Quirk policy

Every deviation from "what a clean implementation would do" is listed in
`docs/compat-quirks.md` (created by the simulation change) under one of these
classes:

| Class | Examples | Policy |
|---|---|---|
| Affects the simulation | Penalty counter sites; monster 0 never delayed; score argument truncating to 16 bits; gauntlet clock wrap; player-two level state initialised in one-player games; bag pushing reversing direction on failure | Reproduce exactly, named in code, covered by a test |
| Undefined behaviour in C | Pixel probe reading past the row end; out-of-bounds player index with an invalid mode combination; `abort()` when firing with no facing | Give it defined, safe behaviour; match what recordings observe where they exercise it; reject invalid mode combinations at configuration time |
| Presentation or usability bug | Sound cannot be re-enabled after starting quiet; arrow keys type letters in initials entry; sound volume setting ignored; level-done jingle silent before any music has played | Fix |
| Dropped feature | CGA, NetSim, X11 embed, pre-1998 recording kludge | Not ported; rejected with a clear message where a user could ask for it |

## 4. Architecture

A Cargo workspace with a strict dependency direction: the simulation knows
nothing about windows, files, clocks or threads.

```
crates/
  digger-core/    library  Simulation, framebuffer rendering, sprite layer,
                           levels, scoring, recording codecs, state hash.
                           No I/O, no clock, no threads, forbid(unsafe_code).
  digger-sound/   library  Sound-effect and music state machines and sample
                           synthesis. Pure: commands in, PCM samples out.
  digger/         binary   Desktop and web frontend: window, presentation,
                           keyboard and gamepad, audio device, configuration,
                           high-score storage, command line, frame pacing.
xtask/            binary   Developer tooling: one-off asset import,
                           conformance runner.
assets/                    Binary art and data blobs embedded at build time.
tests/recordings/          Upstream .drf/.edrf recordings and expected results.
```

### The simulation interface

The original blocks inside nested loops and calls "wait for next frame" from
many places. The port turns this inside out: the game is an explicit state
machine advanced one frame at a time by its caller.

```rust
let mut game = Game::new(config, seed_source);
loop {
    let outcome = game.step(controls);      // exactly one frame
    for cmd in game.drain_sound() { .. }    // sound commands produced this frame
    present(game.framebuffer());            // 640x400 indices + palette
    if outcome.is_finished() { break; }
}
```

- `step` is deterministic: the same configuration, seeds and control inputs
  give the same framebuffer, sound commands and state on every platform.
- Everything the original does while "waiting" (flashing the player number,
  the death sequence, the level-complete pause, redraw delays) is a phase of
  the state machine with a frame count, so tick and frame counts match the
  original.
- The frontend owns time. It decides when to call `step` (frame pacing) and
  may run it as fast as possible for replays and tests.

### Rendering

`digger-core` owns a 640×400 buffer of palette indices and all pixel logic:
masked sprite blits, background tiles, tunnel carving, text, background
save/restore, the pixel probe. A frontend only expands indices through the
current 16-colour palette and shows the result. A headless run is simply a
run where nobody looks at the buffer.

### Sound

The simulation emits typed `SoundCommand`s. `digger-sound` consumes them and
renders samples on the audio thread; the two sides are joined by a lock-free
single-producer queue. Sound never feeds back into the simulation: the two
places where the original waits for a tune to finish are modelled as fixed
frame counts.

Sound is held to **audible** fidelity (same notes, durations, effect shapes,
priorities), verified against the documented tick-level sequences and the
port's own golden sample hashes. It is not required to be sample-identical to
the C build.

### Frontend and platform crates

| Concern | Crate | Notes |
|---|---|---|
| Window and events | `winit` 0.30 | X11, Wayland, Windows, macOS, web |
| Presenting the picture | `softbuffer` 0.4 | CPU framebuffer; integer scaling with letterboxing |
| Audio device | `cpal` 0.18 | Callback model; pulls samples from `digger-sound` |
| Gamepads | `gilrs` 0.11 | Replaces upstream's dead joystick code |
| Command line | `clap` 4 | Idiomatic long options; upstream's `/X:value` forms accepted as aliases |
| Configuration and paths | `serde` + `toml`, `directories` | Platform config and data directories |
| Inflate (title picture) | `miniz_oxide` | Pure Rust |
| State hash | `md-5` | Checkpoint compatibility with `.edrf` recordings |

Versions are pinned in `Cargo.lock`; all have a minimum Rust version at or
below 1.88.

### Assets

Sprites, the font, the title picture, the icon, the level layouts and the
music tables exist upstream only as data tables in C source. A one-off
`xtask import-assets` command (written in Rust) reads those tables **as data**
from an upstream checkout and writes binary blobs plus a generated index into
`assets/`. The blobs are committed; a normal build never needs the upstream
tree. Hash tests guard the blobs against accidental change.

### Licence

The upstream code and Jenner's redrawn graphics are GPL-2.0-or-later, and this
port is a derived work of both, so the repository is licensed
**GPL-2.0-or-later**. Report 07 lists the remaining caveats (older wording in
one upstream text file, no separate statement for the sound tables).

## 5. Idiomatic Rust conventions

These apply to every change and are restated as rules in
`openspec/config.yaml` so that generated plans follow them.

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
  `cargo test --workspace` pass on every commit; commits use Conventional
  Commits.

## 6. Verification strategy

| Layer | What is checked | Source of truth |
|---|---|---|
| Asset integrity | Blob sizes, hashes, table shapes | Values measured from upstream data (report 03) |
| Unit tests | Level decoding, field carving, RNG sequence, scoring thresholds, per-level parameter formulas, bag state machine, direction resolution | Reports 01, 02, 05 |
| Scenario tests | Small scripted situations per spec scenario (push a bag, eat eight emeralds, kill by fireball) | Spec scenarios |
| **Replay conformance** | For each of upstream's 23 `.drf` and 27 `.edrf` recordings: final `score`, `score2`, `level`, `ticks`, and every `.edrf` state-hash checkpoint (every 256 ticks, each round end, game end) | `tests/data` and `tests/results` upstream |
| Golden frames | CRC of the framebuffer at fixed points (title, level 1 drawn, status line) | Recorded from the port once conformance passes |
| Sound | Tick-level divisor sequences per effect; golden sample hashes | Report 04; the port's own output |

Replay conformance is the gate for calling the simulation done. Known gaps in
upstream's recordings (no alternating two-player game, no non-default start
level) are covered by scenario tests and by recordings made with the port and
reviewed by hand.

## 7. Phases

Each phase is one OpenSpec change. They are applied in order; each leaves the
workspace building and its tests green.

| # | Change | Delivers | Capabilities specified |
|---|---|---|---|
| 1 | `bootstrap-rust-workspace` | Workspace, toolchain pin, lints, CI, licence, `xtask` skeleton | none (tooling only) |
| 2 | `add-assets-and-renderer` | Imported assets; indexed framebuffer; palettes; text; sprite layer with collisions | `game-assets`, `rendering`, `sprite-layer` |
| 3 | `add-simulation-core` | Levels and playfield, Digger, monsters, bags, scoring, rounds and modes, as a headless deterministic state machine | `playfield`, `digger`, `monsters`, `gold-bags`, `scoring`, `game-flow` |
| 4 | `add-recording-and-conformance` | DRF and eDRF codecs, record and playback, state hash, replay runner, upstream recordings as a test suite | `game-recording`, `replay-verification` |
| 5 | `add-sound-engine` | Effects, four tunes, synthesis, golden tests | `sound` |
| 6 | `add-desktop-app` | Playable game: window, scaling, pacing, keyboard and gamepad, title screen, high scores, configuration, command line | `desktop-app`, `input-controls`, `configuration`, `high-scores` |
| 7 | `add-web-build-and-releases` | Browser build; release archives for Linux, Windows, macOS | `web-build` |

Phases 5 and 6 depend only on phase 3's interfaces, so phase 5 can proceed in
parallel with phase 4.

```
1 bootstrap ── 2 assets+renderer ── 3 simulation ─┬─ 4 recording+conformance ─┐
                                                  └─ 5 sound ─────────────────┴─ 6 desktop app ── 7 web+releases
```

## 8. Main risks

| Risk | Mitigation |
|---|---|
| Replay divergence that is hard to localise | `.edrf` checkpoints bound a divergence to 256 ticks; the replay runner reports the first failing checkpoint and can dump per-tick state; phase 4 budgets explicit triage tasks |
| The state hash depends on C field order and integer widths | Treated as a file format: one module, table-driven, documented field by field from report 01, never derived from Rust struct layout |
| Restructuring blocking loops into a state machine changes frame counts | Frame and tick counts are asserted by the replay suite; each waiting phase carries its original frame count as a named constant |
| The reports were produced by reading code, not running it | Every behavioural claim that matters is exercised by a recording or a scenario test; disagreements are resolved in favour of the recordings and noted in `docs/compat-quirks.md` |
| Rust-only window stack misbehaves on some platform | The frontend is thin and isolated in one crate; core and sound are testable without it |
| Upstream keeps changing | The port targets one pinned commit; recordings and expected results are copied into this repository |
