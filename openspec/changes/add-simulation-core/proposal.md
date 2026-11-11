# Proposal

## Why

After `add-assets-and-renderer` the workspace can draw sprites and report their collisions, but there is no game: nothing moves, scores or ends. This change adds the whole of Digger's gameplay to `digger-core` as a headless, deterministic state machine, which is the piece every later phase consumes — recordings replay it, the sound engine listens to it, and the desktop and web frontends drive and display it.

It is the largest phase because the original's behaviour is tightly coupled (the picture, the sprite collision lists, a "penalty" counter and the random-number stream all feed one another), so the six capabilities below have to land together to be testable at all.

## What Changes

- Add the playfield: the 15×10 cell grid, the eight built-in level layouts and the level-number-to-layout sequence, custom level data, the earth/tunnel slices of each cell, emeralds, and the drawing of background, tunnels, emeralds and the status line into the existing framebuffer.
- Add the Digger: movement and turning, digging, emerald eating and the eight-in-a-row bonus, firing with a per-level recharge, fireball flight decided by reading pixels back from the framebuffer, the death sequences, lives, respawn with invincibility, and bonus (cherry) mode.
- Add monsters: nobbins and hobbins, the per-level spawn schedule, the chase and flee decisions, transformation timers, hobbin digging, deaths, and the exact pattern in which random numbers are drawn.
- Add gold bags: wobble, fall, breaking, gold lifetime, pushing and push chains, crushing, and collection by diggers and monsters.
- Add scoring: every score event, bonus lives, the monster-eating progression, the score display, and the penalty counter that turns drawing and scoring work into monster delays.
- Add game flow: the game modes (one player, two players alternating, two diggers simultaneously, gauntlet), rounds, the per-tick update order and the different wind-down order, level completion, death, game over, pause, the frame-versus-tick distinction, the speed-to-tick-length mapping, the random-number generator and per-round seeding, and the determinism guarantee.
- Emit typed sound commands at the game events that have a sound cue. No sound is synthesised here.
- Define the frame-stepping interface other phases use: one owning game value advanced one frame per call with that frame's control inputs, returning what kind of frame it was and whether the game has finished; read-only access to the framebuffer, the queued sound commands and the simulation state.
- Create `docs/compat-quirks.md` with every simulation-affecting quirk reproduced by this change, the decisions taken for the original's undefined-behaviour cases, and the test that covers each.
- Reject mode combinations the original accepted by accident (two alternating players together with two simultaneous diggers; a start level outside 1..1000) when the game is configured, instead of misbehaving later.

Nothing is removed and nothing existing changes behaviour; there are no breaking changes.

### Out of scope

| Not in this change | Picked up by |
|---|---|
| DRF/eDRF recording files, record and playback, the state hash and its checkpoints, replaying upstream recordings as a test suite | `add-recording-and-conformance` |
| Sound-effect and music state machines, sample synthesis, the audio device | `add-sound-engine` |
| Window, scaling, frame pacing against a real clock, keyboard and gamepad devices, key redefinition, title screen, high-score table, initials entry and its storage, configuration files, command line, reading level files from disk | `add-desktop-app` |
| Browser build and release archives | `add-web-build-and-releases` |
| Framebuffer, palettes, text, sprite layer and collision lists (used here, not changed) | `add-assets-and-renderer` (already applied) |
| Networked play, CGA mode, the pre-1998 recording timing kludge | Not ported (see `docs/porting-plan.md`) |

Replay conformance against upstream recordings is the final proof of this change's behaviour, but it needs the recording codecs, so it arrives in `add-recording-and-conformance`. This change is verified by unit tests and scripted scenario tests derived from the spec scenarios.

## Capabilities

### New Capabilities

- `playfield`: The cell grid and its pixel mapping, level layouts and their sequence, custom level data, the earth/tunnel state of cells as it is dug and redrawn, emeralds, and the on-screen drawing of background, tunnels, emeralds and the status line.
- `digger`: The player's character: control resolution, movement, digging, emerald eating, firing and fireballs, death and lives, respawn invincibility, bonus mode, and the differences when two diggers play at once.
- `monsters`: Nobbins and hobbins: spawning, speed, path decisions, transformations, interactions with bags and diggers, deaths, and the random-number draw pattern.
- `gold-bags`: Gold bags and gold: wobbling, falling, breaking, pushing, crushing, landing and collection.
- `scoring`: Score events and values, bonus lives, score display, and the penalty counter coupling between scoring/drawing work and monster timing.
- `game-flow`: Game modes, rounds and levels, the per-tick update order, level completion, death and game over, the gauntlet clock, pause, frames versus ticks, speed mapping, random numbers, seeding, sound cues for flow events, and determinism.

### Modified Capabilities

None. `game-assets`, `rendering` and `sprite-layer` are used as specified.

## Impact

- **Code**: new simulation modules in `crates/digger-core` (no new crate). `crates/digger-core` gains its public gameplay interface: game configuration and mode types, the game value with its frame step, per-frame control input, step outcome, sound command type, read-only state view, and an optional observer hook through which recordings will supply seeds and movement samples.
- **Dependencies**: none added. `digger-core` keeps having no I/O, clock, thread or platform dependency.
- **Documentation**: new `docs/compat-quirks.md`; crate-level documentation of the stepping interface.
- **Downstream phases**: `add-recording-and-conformance` consumes the observer hook, the state view and the result summary; `add-sound-engine` consumes the sound command type; `add-desktop-app` consumes the stepping interface, the speed mapping, the pause and quit requests and the per-player results used for high scores.
- **Tests**: unit tests beside each module and scenario tests under `crates/digger-core/tests/`; the whole suite runs headless.
