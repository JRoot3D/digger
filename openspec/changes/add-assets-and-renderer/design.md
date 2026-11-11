# Design

## Context

See `proposal.md` for motivation. Phase 1 (`bootstrap-rust-workspace`) left a
workspace with empty crates `digger-core`, `digger-sound`, `digger` and
`xtask`, the toolchain pin, lints and CI. Nothing draws yet and `assets/` does
not exist.

Constraints that shape this design:

- `digger-core` has no I/O, clock, threads or platform dependencies, is
  deterministic and forbids `unsafe`. The simulation will later call into the
  code designed here many times per frame, and `Game::step` must not allocate.
- Upstream's art exists only as C data tables. It is read as data once, by a
  developer tool; no C is compiled, linked or translated.
- Upstream's video layer is a table of function pointers over global state
  (`ddap`, `sprx[]`, `first[]`, `coll[]` …), with pointers hidden inside byte
  buffers. Research 03 documents its behaviour; the structure is not copied.
- The requirements are in `specs/game-assets`, `specs/rendering` and
  `specs/sprite-layer`. This document does not restate them.

## Goals / Non-Goals

**Goals:**

- A small, safe API in `digger-core` that phase 3 can drive without knowing
  about pixels: "move this sprite, tell me what it hit", "draw this text",
  "what does the probe see here".
- Bit-exact picture contents and collision lists, provable without upstream
  code: by construction from research 03, by model-based tests, and by golden
  checksums recorded once and reviewed by eye.
- An importer that later phases can extend with more tables (levels, music)
  without restructuring.

**Non-Goals:**

- No presenter, window, palette animation timing or frame pacing
  (`add-desktop-app`).
- No drawing helpers that know about game entities (digger frames, bag
  frames, tunnel shape offsets, status line): those need simulation state and
  belong to `add-simulation-core`.
- No performance work beyond "no allocation per operation". A full sprite
  move touches a few kilobytes.
- No general-purpose C parser.

## Decisions

### D1. Module layout

```
crates/digger-core/src/
  lib.rs            #![forbid(unsafe_code)], re-exports
  geom.rs           Px (logical point, i32 x/y), align_x(), Rect (logical, inclusive maths)
  assets/
    mod.rs          include_bytes! of the blobs, include! of the generated index
    image.rs        ImageId, ImageSize, Image<'static> { size, pixels, mask }
    font.rs         Glyph lookup: glyph_for(char) -> Option<&'static [u8; 576]>
    title.rs        TitlePicture::decode() -> Result<TitlePicture, AssetError>
  video/
    mod.rs
    palette.rs      Palette, Intensity, Rgb, the four colour tables
    framebuffer.rs  Framebuffer: clear, blit_masked, capture, restore, probe, crc32
    saved.rs        SavedRect (fixed capacity, carries its own size)
    text.rs         TextColor, draw_char, draw_text, erase_text, draw_number
    backdrop.rs     LevelPlan (1..=8), draw_background, draw_title
  sprite/
    mod.rs          SpriteLayer and its operations
    id.rs           SpriteId, SpriteClass
    collisions.rs   Collisions (the five lists)
  error.rs          AssetError, and the crate error enum it joins
xtask/src/
  main.rs           subcommand dispatch (from phase 1)
  import/
    mod.rs          `import-assets` driver: read, validate, plan, write or check
    ctable.rs       reader for C array and pointer-table initialisers
    sprites.rs      sprite table import      font.rs   font import
    title.rs        title stream import      icon.rs   icon import
    manifest.rs     manifest writer and reader
assets/
  sprites.bin  font.bin  title.zz  icon.rgba  index.rs  manifest.txt  README.md
```

`video` knows `assets`; `sprite` knows `video`; nothing knows upward. The
split mirrors the three capabilities, so each spec maps onto one module tree.

*Alternative rejected:* one `gfx.rs` mirroring upstream's `sprite.c` +
`sdl_vid.c`. It would reproduce upstream's file boundaries, not the port's
concepts, and put asset parsing, pixels and collision bookkeeping in one
namespace.

### D2. Asset blobs, generated index, embedding

| File | Content | Size |
|---|---|---|
| `assets/sprites.bin` | Unique upstream arrays, concatenated in order of first reference by image number; de-duplicated by content | about 150 KB |
| `assets/font.bin` | 40 glyphs × 576 bytes in the order A–Z, 0–9, `.`, `:`, `_`, space | 23040 B |
| `assets/title.zz` | Upstream zlib stream, verbatim | 2772 B |
| `assets/icon.rgba` | 64×64, bytes R, G, B, A per pixel, row-major | 16384 B |
| `assets/index.rs` | Generated: `SPRITE_INDEX: [SpriteEntry; 120]` (image offset, mask offset, w, h) and `GLYPH_INDEX: [Option<u8>; 95]` | text |
| `assets/manifest.txt` | One line per file: SHA-256, byte length, name; header line with the upstream commit | text |

`digger-core` embeds `sprites.bin`, `font.bin` and `title.zz` with
`include_bytes!` and pulls in `index.rs` with `include!`, both through
`concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/…")`. The icon is embedded
by the `digger` binary in phase 6; this change only commits and guards it.

Image sizes are not stored upstream. The importer holds the fixed
size-by-number table from research 03 §5.2 and validates each pair against
it: the array that is specific to the image must be exactly `8w × 2h` bytes,
a shared array (the all-zero block, the opaque digger mask) must be at least
that long, and the image number takes its first `8w × 2h` bytes. This keeps
upstream's "shared zero array used at several sizes" semantics by value.

*Alternatives rejected:*
- **Parse the C tables in `build.rs`.** Every build would need the upstream
  tree, against the porting plan.
- **Commit generated Rust arrays instead of blobs.** A 150 KB byte-array
  literal is slow to compile, unreadable in review, and no easier to guard.
- **A binary index.** The index is tiny; as Rust source it is type-checked
  and readable in a diff.
- **PNG files.** Would add a decoder to the core and an encode/decode step
  that has to be proven lossless for palette indices and for the 255
  transparency marker.
- **Pre-inflating the title picture.** Saves `miniz_oxide` in the core but
  commits 256 KB instead of 2.8 KB; the porting plan already names
  `miniz_oxide` for this.

### D3. Typed asset access instead of numbers and pointers

- `ImageId` is a newtype over `u8` whose constructor returns `None` for
  values ≥ 120. Named constants cover the fixed entries phase 3 needs
  (`ImageId::BAG_STILL`, `ImageId::EMERALD`, `ImageId::background(plan)` …);
  offsets (`+31`, `+32`) are methods (`for_second_digger()`), so the magic
  numbers live in one file with the research citation.
- `ImageId::get()` returns `Image<'static>` with `size`, `pixels` and `mask`
  slices cut from the blob through the generated index. Lookup is infallible
  because `ImageId` is valid by construction and the index is checked by a
  test and by `const` assertions on blob lengths.
- **A sprite's size is the size of its image.** Upstream passes width and
  height separately on every call, always equal to the table size (including
  the 4,14 dead hobbin). The port has no separate size parameter, which
  removes a whole class of mismatches and bounds every saved rectangle by the
  largest image (960 bytes).
- Glyph lookup takes a `char` and returns `Option`; the "invalid becomes a
  space" rule lives in `draw_text`, not in the font.
- `TitlePicture::decode()` inflates `title.zz` once into a boxed
  256000-byte array and checks the length. It is called by whoever owns the
  game (phase 3's `Game::new`), so no inflation or allocation happens during
  a frame. It is the only fallible asset operation.

### D4. The framebuffer is a plain owned value

```rust
pub struct Framebuffer { pixels: Box<[u8; 256_000]>, palette: Palette, intensity: Intensity }
```

- `Palette { Zero, One }` and `Intensity { Normal, High }` are enums, so
  upstream's "any other value behaves like 0" cases cannot be expressed.
  `set_palette` also sets `Intensity::Normal`; that rule is one line with a
  quirk comment.
- `colors() -> &'static [Rgb; 16]` selects one of four `const` tables.
  `pixels() -> &[u8; 256_000]` is what a presenter reads. There is no
  `present`, `flush` or callback: upstream's twelve-slot function table and
  its `gflush` calls are presentation, and presentation is the caller's
  business (porting plan §4).
- All pixel operations take logical `Px` and work in `i32`, clip by
  intersecting with `0..640 × 0..400`, and then copy row slices. No operation
  can index out of bounds and none returns `Result`: with typed arguments
  there is no invalid input left.
- The probe is the one place that reads by linear offset. It is written as a
  small function over `(2y + r) * 640 + 2x + i` computed in `i64`/`i32` with
  `pixels.get(offset)` and `unwrap_or(0)`, after the `x > 319 || y > 199`
  test. It returns a `ProbeBits(u8)` newtype with named masks for the callers
  in phase 3 (`UP = 0xC0`, `DOWN = 0x03`).
- `crc32()` is a table-driven CRC-32 implemented in the crate (about 20
  lines, table built in a `const fn`). It is public API because the replay
  runner and golden-frame tests of later phases use it.

*Alternatives rejected:*
- **A `trait Video` / `trait DrawApi` with a headless and a windowed
  implementation** (research 03 §12.2). Every upstream backend re-implements
  the same pixel logic; with one concrete framebuffer there is nothing to
  abstract, and a headless run is simply a run nobody looks at.
- **Storing 320×200 logical pixels.** The art is genuine 640×400 art and the
  probe distinguishes real columns.
- **A `crc32fast` dependency.** A dependency in the deterministic core for 20
  lines of code is not worth it.

### D5. Saved rectangles replace the pointer-in-a-buffer trick

Upstream hides a heap pointer in the first bytes of a caller-supplied byte
buffer, with NULL meaning "nothing saved". The port uses

```rust
pub struct SavedRect { w: u16, h: u16, pixels: [u8; 960] }   // real-pixel size, fixed capacity
```

owned by the sprite that captured it, held as `Option<SavedRect>`. `None`
makes "restore before any capture" a no-op where C would dereference NULL.
The rectangle carries its own size and restore ignores the sprite's current
size, as the reference build does. The fixed capacity (the largest image,
32×30) means capturing never allocates. `Framebuffer::capture` takes an
`ImageSize`, so a larger request cannot be written.

### D6. Sprite layer: one owner, bit sets, no recursion

```rust
pub struct SpriteLayer { sprites: [Sprite; 18], collisions: Collisions }
struct Sprite { enabled: bool, pos: Px, image: ImageId, pending: ImageId, saved: Option<SavedRect> }
pub enum SpriteClass { Bonus, Bag, Monster, Fireball, Digger }
pub struct SpriteId(u8);            // 0..18, constructors per class: SpriteId::bag(i) -> Option<SpriteId>
pub struct Collisions(u32);         // bit n set = slot n collided; iterated per class, ascending
```

- The layer is a field of the future `Game`; operations take
  `&mut Framebuffer` as an argument (`layer.move_to(&mut fb, id, px)`), so the
  borrow checker sees two disjoint fields and no global is needed.
- Operations: `reset(id, image)`, `set_pending(id, image)`,
  `place(fb, id, px)`, `move_to(fb, id, px) -> Collisions`, `erase(fb, id)`,
  `draw_static(fb, image, px)` (protected, the three upstream steps in one
  call), `draw_static_unprotected(fb, image, px)`, `collisions()`,
  `clear_collisions()`.
- The redraw set is a `u32` bit set computed by an explicit worklist over the
  18 slots. Upstream's recursive walk with two flag arrays computes a
  transitive closure; a set has no order, and all later processing is "for
  each set bit ascending", which is exactly upstream's index-ordered loops.
  The move operation unions two closures (old and new rectangle) as upstream
  does by clearing only the visited flags between walks.
- `Collisions` is the same bit set idea. Upstream's `first[5]` / `coll[18]`
  chains are a linked-list encoding of "ascending list per class"; iterating
  the bits of a class range gives the same sequence. `move_to` returns the
  value as well as storing it, so callers hold a copy instead of copying
  globals; the stored copy exists because one upstream caller reads the
  globals later and because "which move produced the lists" must match.
  `clear_collisions()` covers upstream's "invalid digger frame" path, which
  resets only the heads and thereby makes the chains unreachable.
- The static slot (upstream's sprite 18) is not a `Sprite`: it only ever
  supplies a rectangle to the closure, so `draw_static` passes that rectangle
  directly. "The misc sprite is never redrawn or listed" then holds by
  construction.
- Collision borders are not ported. Every upstream call passes zero borders
  (research 03 §7.2), so the bordered test equals the plain one; carrying two
  always-zero fields and a second predicate would be untestable dead code.
  If a recording ever disproves this, the predicate is one function.

*Alternatives rejected:*
- **Transliterating the arrays and the recursion** as research 03 §12.2
  suggests. It works, but keeps index-typed flags and `-1` sentinels that the
  porting plan rules out.
- **Deriving collisions from entity positions in phase 3.** Research 02 §1 is
  explicit that sprite rectangles diverge from entity state (reverted moves,
  erased-but-alive sprites, last-drawn sizes).
- **Full-frame recomposition each move** (redraw background and all sprites).
  Simpler, and equal to upstream under the normal discipline, but it cannot
  reproduce the out-of-discipline cases (quirks S3, S4) that the blinking
  digger exercises, and it would need a separate static-background buffer.

### D7. Error handling

- `digger-core`: one `AssetError` (`TitleInflate`, `TitleLength { got }`)
  joins the crate error enum. Everything else is infallible by typed
  arguments (`ImageId`, `SpriteId`, `LevelPlan`, `TextColor`, `Palette`),
  whose constructors return `Option` for out-of-range numbers. No `unwrap` or
  `expect` on anything derived from input; the blob/index consistency is a
  `const` assertion plus a test, not a runtime check.
- `xtask`: an `ImportError` enum with file, line and table name
  (`MissingFile`, `MissingTable`, `Syntax`, `UnknownSymbol`,
  `Length { expected, got }`, `Count`, `TitleInflate`, `Io`, and `Drift`
  for a failed `--check`, listing the changed, missing and extra files);
  `MissingTable` covers a source file that exists but does not define a
  table the import needs. The importer reads and validates
  everything, builds all outputs in memory, and only then writes, each file
  through a temporary file and rename, so a failure never leaves a
  half-written `assets/`. `--check` stops before the write step and compares.
  `assets/README.md` is written by hand (task 3.4), so `--check` does not
  report it as an extra file; every other file in `--out` that the import
  does not produce is reported.
- Sprite array lengths: an array referenced from `vgatable` with one size
  only must hold exactly `8w × 2h` bytes; an array referenced with several
  sizes (upstream: only `vgazero480`) must hold at least the largest of
  them. `sprites.bin` stores each distinct array content once, so
  `vgazero480` and `vgaediggermask` (both 960 zero bytes) share one copy;
  with upstream 638125e the blob is 136576 bytes.
- The table reader handles exactly the grammar the upstream data files use:
  comments, `static`/`const` qualifiers, `NAME[N] = { number, … };` with
  decimal or hexadecimal numbers, and pointer tables whose elements are
  identifiers, `NULL` or `0`. The size `N` may be empty, a number or a name
  from a `#define NAME <number>` line (`title_gz[CTITLELEN]`); when given it
  must equal the number of initialisers. Upstream data files also contain
  preprocessor lines and, in `title_gz.c`, a function definition: such
  top-level items without an array initialiser are skipped as balanced
  token runs. Anything malformed inside an array declaration or initialiser
  (a non-number where a value is expected, a value too large for the element
  type, an octal-looking literal such as `012`, a missing separator), and
  anything unbalanced or unterminated, is a `Syntax` error with a line
  number. It never panics on malformed text.

### D8. Test strategy

| Kind | Where | What |
|---|---|---|
| Manifest test | `xtask/tests/asset_manifest.rs` | Every file in `assets/` has the length and SHA-256 in `manifest.txt`; no missing or unlisted file |
| Asset shape tests | `digger-core` `assets` unit tests | 120 entries, sizes per number, value sets, mask rules, duplicates (57–61, 88–93), doubled tunnel masks, tile colour sets, 66/29 glyph coverage, title length, value set and landmark pixels |
| Importer tests | `xtask` unit tests | Table reader on small hand-written fixtures; every error variant; determinism; atomic write; check mode |
| Framebuffer unit tests | `digger-core` `video` | One test per spec scenario: blit rule truth table, clipping on all four edges, capture/restore round trip, probe vectors including wrap and out-of-range, palette tables, text recolouring, number drawing, tiling |
| Sprite unit tests | `digger-core` `sprite` | One test per spec scenario, built on a small patterned background |
| Model-based tests | `digger-core/tests/sprite_model.rs` | Long pseudo-random operation sequences from a fixed-seed generator written in the test (no extra dependency), checked after every step against two independent oracles: the picture equals "static background + enabled sprites in ascending order", and the collision lists equal a brute-force rectangle scan |
| Golden checksums | `digger-core/tests/golden_frames.rs` | CRC-32 of: cleared picture (known constant), title picture, title plus top-line text, background for each plan 1–8, a text sheet with all 66 characters in 4 colours, a number sheet, a fixed sprite scene after a scripted sequence |

Golden values other than the cleared picture cannot be taken from upstream
(no C is run). They are recorded from the port once the scenario tests pass,
after a person has looked at the pictures: `cargo xtask render-sheet` writes
PPM files of every golden scene and a contact sheet of all 120 images to
`target/`. A failing golden test prints the actual checksum. The title
picture's checksum is additionally printed by the importer from the inflated
upstream bytes, so that one value is independent of the renderer.

### D9. Idiomatic Rust in place of the original structure

| Original | Port |
|---|---|
| Global function-pointer table `ddap`, rebound at run time for CGA | One concrete `Framebuffer`; no CGA |
| Global sprite arrays and global `first[]` / `coll[]` | `SpriteLayer` owned by the game; `Collisions` returned by value |
| `int16_t ch`, `int n`, `-1` list terminators | `ImageId`, `SpriteId`, `SpriteClass`, iterators |
| Palette and intensity as integers | `Palette`, `Intensity` enums |
| Colour argument 0–3, others silently 0 | `TextColor` enum with four variants |
| Recursive flag-setting walk | Bit-set worklist |
| Pointer stored in a byte buffer, NULL = empty | `Option<SavedRect>` with inline storage |
| NULL dereference, out-of-bounds reads | Defined no-ops and defined zero reads |
| Width and height passed beside the image number | Size is a property of `ImageId` |

### D10. Compat quirks reproduced

This change creates `docs/compat-quirks.md` with the entries below, each with
the named test that pins it. `add-simulation-core` appends to the same file.
Each quirk is isolated in one function or one branch carrying a comment with
the quirk id.

| Id | Quirk | Class | Isolated in | Test |
|---|---|---|---|---|
| R1 | Probe reads an unaligned 8×2 real block, ORs both rows, masks with 0xEE; returns unmasked 0xFF beyond x 319 / y 199 | Simulation | `Framebuffer::probe` | `probe_vectors` |
| R2 | Probe near the right edge runs into the next row; reads outside the buffer are 0 (upstream: undefined behaviour) | Undefined behaviour, given defined result | `Framebuffer::probe` | `probe_row_wrap`, `probe_outside_buffer_reads_zero` |
| R3 | Selecting a palette resets the intensity | Simulation-visible picture | `Framebuffer::set_palette` | `palette_select_resets_intensity` |
| R4 | Masked blit keys on image byte 255 in addition to the mask | Simulation (probe input) | `Framebuffer::blit_masked` | `blit_rule_truth_table` |
| R5 | Text is opaque, a full 12×12 cell; unsupported characters in a string erase their cell; a lone unsupported character draws nothing | Simulation (probe input) | `text::draw_text`, `text::draw_char` | `text_cell_is_opaque`, `text_invalid_char_becomes_space` |
| R6 | Number drawing leaves the leftmost cell untouched when its digit is zero | Simulation (probe input) | `text::draw_number` | `number_leftmost_cell_not_erased` |
| R7 | Background tiling starts at y 14 and its last row is clipped; rows 12–13 are never drawn | Simulation (probe input) | `backdrop::draw_background` | `background_coverage` |
| S1 | x is rounded down to a multiple of 4 by place, move and static draw, but not for the protected area, text or the probe | Simulation | `geom::align_x` and its three call sites | `alignment_sites` |
| S2 | Collision lists are produced only by a move and persist until the next move | Simulation | `SpriteLayer::move_to` | `collisions_only_on_move` |
| S3 | A move restores the sprite's saved background at its recorded position even when the sprite is disabled; saved backgrounds survive erase and reset | Simulation (blinking digger) | `SpriteLayer::move_to` | `move_after_erase_restores_stale_background` |
| S4 | Placing an enabled sprite does not erase it and may paste its old background at the new position | Simulation | `SpriteLayer::place` | `place_enabled_sprite_quirk` |
| S5 | Other sprites collide with their current (last drawn) image size, e.g. 16×14 | Simulation | `Sprite::rect` | `collision_uses_current_size` |
| S6 | Reset forgets an enabled sprite without erasing it | Simulation | `SpriteLayer::reset` | `reset_does_not_erase` |
| S7 | Z-order is slot order; enabled-but-dead sprites collide | Simulation | bit-set iteration | `stacking_order`, `enabled_not_alive` |
| A1 | Image numbers 88–93 duplicate 82–87 and 57–61 duplicate 26–30, kept so that the +31 / +32 offsets hold | Data layout | generated index | `duplicate_image_numbers` |

## Risks / Trade-offs

- [The golden checksums are recorded from the port itself, so a systematic
  misreading of research 03 would be baked in] → The asset blobs are checked
  against upstream by digest (`--check`), the title checksum comes from
  upstream bytes, the model-based tests use oracles that do not share code
  with the sprite layer, a person reviews the rendered sheets, and phase 4's
  replay suite is the final arbiter: the fireball probe makes replays
  sensitive to wrong pixels.
- [Deriving sprite size from the image number could differ from upstream if
  some call passes a size that does not match the table] → Research 03 §5.2
  and §8.4 list every call with matching sizes. If phase 3 finds a mismatch,
  `set_pending` gains an explicit size; the change is local to `Sprite::rect`
  and `SavedRect`.
- [Dropping collision borders] → Same reasoning and same escape hatch; the
  overlap predicate is a single function.
- [The probe's out-of-range rule (linear wrap, zero outside) is a choice,
  not an observation] → Research 03 §4.6 argues that fireballs never probe
  beyond x 316 or above y 0. The rule is one function; if a recording
  disagrees, the recording wins and the quirk entry is updated.
- [The hand-written table reader may not cope with an upstream formatting
  detail the research did not mention] → It fails loudly with file and line;
  the first real import (task group 2) is where this surfaces, before any
  renderer code depends on the blobs.
- [Importing needs a network fetch of upstream once] → Only for the
  developer running the import; the blobs are committed, and CI never
  fetches.
- [`include_bytes!` paths reach outside the crate directory, so
  `digger-core` cannot be published to a registry as is] → Not a goal; if it
  becomes one, `assets/` moves under the crate.
- [`docs/compat-quirks.md` is shared by every later change] → This change
  creates it (porting plan §3); phases 3 to 7 append their own sections and
  use their own id prefixes (`Q` simulation, `P` recording and playback,
  `SQ`/`SF` sound) so ids never collide with `R`, `S` and `A` here.

## Open Questions

- Whether the icon should also be committed as an `.ico`/`.icns` for release
  packaging is left to `add-web-build-and-releases`; `icon.rgba` is enough
  for the window icon.
