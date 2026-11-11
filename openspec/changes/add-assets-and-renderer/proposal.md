# Proposal

## Why

In Digger the picture and the sprite bookkeeping are game state: a fireball
decides whether it hit earth by reading pixels back from the screen, and every
collision in the game comes from the sprite layer's bounding-box lists
(porting plan §3). The simulation in the next phase therefore cannot be
written, let alone replayed against upstream recordings, until a pixel-exact
renderer, the sprite layer and the original art exist. The art itself exists
upstream only as data tables inside C source files, so it has to be imported
once and committed before anything can be drawn.

## What Changes

- Add a one-off developer command, `xtask import-assets`, that reads the
  upstream sprite, font, title-picture and icon tables **as data** from a
  checkout of the pinned upstream commit and writes binary blobs, a generated
  index and a digest manifest into `assets/`.
- Commit the imported blobs: 120 sprite image/mask pairs, 40 font glyphs, the
  2772-byte compressed title picture and the 64x64 window icon. A normal build
  embeds them and never needs the upstream tree or the network.
- Add asset integrity tests: blob lengths, digests, table shapes and the value
  sets measured in research report 03.
- Add to `digger-core` a 640x400 palette-indexed framebuffer that game logic
  addresses as 320x200, with: clear, the two palettes in two intensities
  (exact RGB values), masked image draw with clipping, rectangle capture and
  restore, opaque text and number drawing, the per-level background tiling,
  title-picture drawing, the pixel probe used by fireballs (including a
  defined result where upstream reads out of bounds), and a frame checksum for
  golden tests.
- Add to `digger-core` the sprite layer: 18 movable sprites plus one slot for
  static drawing, fixed z-order, place / move / erase operations with the
  original background save and restore ordering, 4-pixel x alignment, and the
  per-class collision lists computed at exactly the points the original
  computes them.
- Start `docs/compat-quirks.md` with the rendering and sprite-layer quirks
  this change reproduces, and add `docs/assets.md` describing the blob formats
  and the import procedure.

Nothing here is user-visible yet: there is no window and no game loop. The
deliverable is a library that later phases drive, proven by unit tests,
model-based tests and golden framebuffer checksums.

### Out of scope

| Left out | Picked up by |
|---|---|
| Level layouts and their import; the field (tunnel) map; drawing of tunnels, emeralds, bags, lives and the status line; animation counters of Digger, monsters and fireballs; the penalty counter | `add-simulation-core` (it extends `xtask import-assets` with the level tables) |
| The scripted title-screen animation and the high-score table layout | `add-desktop-app` |
| Music and sound-effect tables and their import | `add-sound-engine` (it extends `xtask import-assets` with the music tables) |
| Showing the framebuffer in a window, scaling, fullscreen, frame pacing, using the icon | `add-desktop-app` |
| Recording golden frames of real gameplay (level 1 drawn, status line) | `add-simulation-core` and `add-recording-and-conformance`, using the checksum defined here |
| CGA graphics data and CGA mode | Not ported (porting plan §2) |

## Capabilities

### New Capabilities

- `game-assets`: the imported sprite, font, title-picture and icon data — what
  exists, its sizes and value guarantees, glyph coverage, integrity checks on
  the committed blobs, and the one-off import command that produces them.
- `rendering`: the 640x400 indexed framebuffer addressed as 320x200, palettes
  and intensities, masked image drawing and clipping, rectangle capture and
  restore, text and number drawing, background tiling, title picture, the
  pixel probe and the frame checksum.
- `sprite-layer`: the 18 sprites plus the static-drawing slot, z-order,
  place / move / erase semantics, background save and restore ordering, x
  alignment, and the collision lists with the exact moments they change.

### Modified Capabilities

None. No specs exist yet.

## Impact

- **Code**: new modules in `crates/digger-core` (assets, framebuffer, text,
  sprite layer); new `import-assets` subcommand and its table reader in
  `xtask`. `digger-sound` and `digger` are untouched.
- **Repository contents**: new `assets/` directory with committed binary blobs
  (about 190 KB in total), a generated index and a digest manifest; new
  `docs/assets.md`; `docs/compat-quirks.md` is created here, earlier than the
  porting plan states, and extended by `add-simulation-core`.
- **Dependencies**: `miniz_oxide` in `digger-core` (inflating the title
  picture; pure Rust, already named in the porting plan); `sha2` in `xtask`
  only (asset digests). No platform dependency is added.
- **Interfaces other phases build on**: the framebuffer and sprite-layer API
  of `digger-core`, the image numbering 0..119, the frame checksum, and the
  layout of `assets/`.
- **Licence**: the imported art is derived from GPL-2.0-or-later upstream
  data, consistent with the repository licence.
