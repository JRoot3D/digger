# Game assets

The sprites, font, title picture and window icon exist upstream only as data
tables in C source files. `cargo xtask import-assets` reads those tables **as
data** from a checkout of
[sobomax/digger](https://github.com/sobomax/digger) at commit
`638125eecc2a36629b886f58d7b75253894cf3d3` and writes the files in
[`assets/`](../assets/README.md). The files are committed, so a normal build
needs neither the upstream tree nor network access; `digger-core` embeds them
at compile time. The requirements are in the `game-assets` spec, the design in
`openspec/changes/add-assets-and-renderer/design.md` (D2, D7), the behavioural
background in `docs/research/03-graphics.md` (§5, §6, §9.1, §10, §12).

## File formats

All multi-byte layouts are row-major. "Real" pixels are those of the 640x400
picture; the game addresses it as 320x200 logical pixels, so one logical pixel
is 2x2 real pixels and a logical row is two real rows.

### `sprites.bin` (136576 bytes)

The 120 VGA sprite images (numbers 0–119) and their masks, one byte per real
pixel. An image of width `w` (in units of 4 logical pixels) and height `h` (in
logical pixels) spans `8w` bytes per real row and `2h` real rows, so its
image and its mask are each `8w x 2h` bytes.

The file is the concatenation of the distinct upstream arrays, de-duplicated
by content, in the order in which image numbers first refer to them (image
0's image, image 0's mask, image 1's image, ...). Several image numbers share
an array; an array shared by images of different sizes (for example the
all-zero block used at sizes 4,15, 6,6 and 2,18) is stored once at its full
length, and each image number uses its first `8w x 2h` bytes. Where each
image and mask starts is given by `SPRITE_INDEX` in `index.rs`.

- Image bytes: 255 is transparent; any other value is a palette index from
  {0, 2, 3, 4, 6, 7, 8, 10, 12, 14, 15}.
- Mask bytes: 0 replaces the destination pixel, 255 keeps it. Wherever an
  image byte is neither 0 nor 255, the mask byte is 0.

Image sizes are not stored upstream; the importer applies the fixed
size-by-number table of research 03 §5.2 and rejects an image-specific array
whose length is not exactly `8w x 2h` (a shared array must be at least that
long).

### `font.bin` (23040 bytes)

40 glyphs of 576 bytes each, 24x24 real pixels (12x12 logical), one palette
index per byte from {0, 10, 12, 15}, in the order `A`–`Z`, `0`–`9`, `.`, `:`,
`_`, space. Glyph `n` starts at byte `576 n`. Real columns 20–23 are always 0
(the built-in letter spacing).

### `title.zz` (2772 bytes)

The upstream title picture, a zlib-format stream (first bytes `0x78 0xDA`)
copied verbatim. It inflates to exactly 256000 bytes: the 640x400 picture as
palette indices from {0, 4, 7, 8, 15}. The importer prints the CRC-32 of the
inflated bytes (`0x7FA63EA5`), which the golden-frame test of the title
picture must reproduce.

### `icon.rgba` (16384 bytes)

The 64x64 window icon, 4 bytes per pixel in the order red, green, blue,
alpha. Upstream stores each pixel as a 32-bit word `0xAARRGGBB`; the word
`0xFF112233` becomes the bytes `11 22 33 FF`. The `digger` binary embeds the
icon; this file is only committed and guarded until then.

### `index.rs`

Generated Rust source that `digger-core` pulls in with `include!`:

- `SPRITE_INDEX: [SpriteEntry; 120]`: for each image number, the byte offset
  of its image and of its mask in `sprites.bin`, and its size `w`, `h`.
- `GLYPH_INDEX: [Option<u8>; 95]`: for character codes 32–126 (entry `i` is
  code `32 + i`), the glyph number in `font.bin`, or `None` for a character
  without a glyph. 66 codes have a glyph; lower-case letters share the glyph
  of their upper-case letter.

### `manifest.txt`

Comment lines starting with `#` (one names the upstream commit), then one
line per generated file, sorted by name:

```text
<SHA-256 as 64 lowercase hex digits>  <length in bytes>  <file name>
```

The manifest does not list itself or the hand-written `assets/README.md`.
The test `xtask/tests/asset_manifest.rs` fails, naming the file, when a file
differs from its line, a listed file is missing or an unlisted file is
present.

## Re-importing and checking

Fetch the pinned upstream commit into a temporary directory outside the
repository (this is the only step that needs network access):

```sh
UPSTREAM="$(mktemp -d)"
git -C "$UPSTREAM" init -q
git -C "$UPSTREAM" fetch -q --depth 1 https://github.com/sobomax/digger.git 638125eecc2a36629b886f58d7b75253894cf3d3
git -C "$UPSTREAM" checkout -q FETCH_HEAD
```

Then, from the repository root, in the same shell:

```sh
cargo xtask import-assets --upstream "$UPSTREAM" --check
cargo xtask import-assets --upstream "$UPSTREAM"
```

The first command performs the whole import in memory and compares it with
`assets/` without modifying anything; it exits 0 when every file is identical
and otherwise exits non-zero listing each changed, missing and extra file.
The second writes the files. The import validates everything before writing
anything; on any error it names the file and table concerned, exits non-zero
and leaves `assets/` untouched. Each file is written through a temporary file
and a rename. Running it twice gives byte-identical output, so on an
unmodified checkout `git status` shows no change afterwards.

`--out <dir>` writes to (or checks) another directory instead of `assets/`.

## Adding tables in later changes

Later changes extend the same command rather than adding new ones:
`add-simulation-core` adds the level layouts (`assets/levels.bin`) and
`add-sound-engine` the music tables (`assets/music.bin`). Each extension:

1. adds a module under `xtask/src/import/` that reads its tables through the
   table reader in `ctable.rs` and validates their shape, returning
   `ImportError` (naming file and table) on any mismatch;
2. adds its output file to the list built by `import::build`, so that it is
   written, checked by `--check` and listed in `manifest.txt` like the
   others;
3. documents the new file's format in this document and its origin in
   `assets/README.md`;
4. re-runs the import, commits the new file together with the regenerated
   `manifest.txt`, and adds the file name to the expected list in the
   `manifest_lists_the_imported_files` test.

The flags (`--upstream`, `--out`, `--check`), the validate-then-write rule and
the manifest format stay as they are.
