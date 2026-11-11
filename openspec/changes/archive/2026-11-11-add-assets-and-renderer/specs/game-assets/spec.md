# Spec Delta

## Purpose

Defines the original game art that the program carries inside itself — sprite
images and masks, the font, the title picture and the window icon — together
with the guarantees on its shape and content and the one-off import that
produces the committed data from the upstream tables.

## ADDED Requirements

### Requirement: Sprite image set
The game SHALL carry exactly 120 sprite images, numbered 0 to 119. Each image number MUST provide an image and a mask of identical dimensions, stored uncompressed with one byte per real pixel in row-major order. An image of width w (in units of 4 logical pixels) and height h (in logical pixels) MUST have exactly 8w x 2h bytes in its image and in its mask. (research 03 §5.1, §5.2)

#### Scenario: Count of image numbers
- **WHEN** the sprite image set is enumerated
- **THEN** it contains image numbers 0 through 119 and no others

#### Scenario: Byte size follows from dimensions
- **WHEN** image number 1 (width 4, height 15) is read
- **THEN** its image and its mask each hold 960 bytes, laid out as 30 rows of 32 bytes

#### Scenario: Unknown image number
- **WHEN** an image number of 120 or greater is requested
- **THEN** the request is rejected as invalid and no pixel data is returned

### Requirement: Image dimensions by number
Every image number SHALL have one fixed size (w in 4-pixel units, h in logical pixels): 0–79 and 81 are 4,15; 80 is 4,14; 82–93 and 114–119 are 2,8; 94–101 are 5,4; 102 and 104 are 2,18; 103, 105 and 106 are 6,6; 107 is 6,8; 108 and 109 are 4,10; 110–113 are 4,12. (research 03 §5.1, §5.2)

#### Scenario: Standard creature size
- **WHEN** the size of image number 69 is queried
- **THEN** it is 4,15, that is 16x15 logical pixels and 32x30 real pixels

#### Scenario: Short dead-hobbin image
- **WHEN** the size of image number 80 is queried
- **THEN** it is 4,14 with 896 bytes of image data, one logical row shorter than its neighbours

#### Scenario: Fireball and tile sizes
- **WHEN** the sizes of image numbers 82 and 94 are queried
- **THEN** they are 2,8 (256 bytes) and 5,4 (320 bytes) respectively

#### Scenario: Tunnel shape sizes
- **WHEN** the sizes of image numbers 102, 103 and 107 are queried
- **THEN** they are 2,18 (576 bytes), 6,6 (576 bytes) and 6,8 (768 bytes) respectively

### Requirement: Image number assignments
Image numbers SHALL keep the upstream meanings: 0 blank, 1–24 first digger (right, up, left, down; 3 frames able to fire then 3 unable, per direction), 25 dead digger, 26–30 gravestone; 31–61 the same layout for the second digger; 62–65 bag, 66–68 gold; 69–72 nobbin, 73–76 right hobbin, 77–80 left hobbin; 81 bonus; 82–87 fireball and explosion; 94–101 background tiles; 102–107 tunnel shapes; 108–109 emerald and its eraser; 110–113 life icons; 114–119 second fireball. (research 03 §5.2)

#### Scenario: Second digger offset
- **WHEN** the image for the second digger's counterpart of image number n (1 ≤ n ≤ 30) is needed
- **THEN** it is image number n + 31

#### Scenario: Second fireball offset
- **WHEN** the image for the second digger's counterpart of fireball or explosion image n (82 ≤ n ≤ 87) is needed
- **THEN** it is image number n + 32

#### Scenario: Gravestones are shared
- **WHEN** image numbers 57–61 are compared with 26–30
- **THEN** each pair has byte-identical image and mask data

#### Scenario: Padding entries duplicate the fireball
- **WHEN** image numbers 88–93 are compared with 82–87
- **THEN** each pair has byte-identical image and mask data

### Requirement: Image and mask value guarantees
Image bytes SHALL be either the transparent value 255 or a palette index from the set {0, 2, 3, 4, 6, 7, 8, 10, 12, 14, 15}. Mask bytes SHALL be only 0 (replace) or 255 (keep). Wherever an image byte is neither 0 nor 255, the mask byte at the same position MUST be 0. Palette indices 1, 5, 9, 11 and 13 MUST NOT occur in any image. (research 03 §3, §4.5, §5.1)

#### Scenario: Mask values
- **WHEN** every mask byte of all 120 image numbers is inspected
- **THEN** each byte is 0 or 255

#### Scenario: Coloured pixels are never masked out
- **WHEN** every position of every image is inspected
- **THEN** no position has an image byte other than 0 or 255 together with a mask byte of 255

#### Scenario: Unused palette indices
- **WHEN** every image byte of all 120 image numbers is inspected
- **THEN** none of the values 1, 5, 9, 11 or 13 is found

### Requirement: Eraser and opaque-block images
Image numbers 0, 31, 102–107, 109 and 112 SHALL have an image consisting only of zero bytes, so that drawing them paints black wherever their mask is 0. Image numbers 0, 31, 81 and 94–101 SHALL have a mask consisting only of zero bytes, so that drawing them replaces their whole rectangle except where the image byte is 255. (research 03 §4.5, §5.1, §5.2)

#### Scenario: Blank digger image
- **WHEN** image number 0 is read
- **THEN** its 960 image bytes and its 960 mask bytes are all zero

#### Scenario: Emerald eraser shares the emerald silhouette
- **WHEN** image numbers 108 and 109 are compared
- **THEN** their masks are byte-identical and the image of 109 is all zero

#### Scenario: Background tile is fully opaque
- **WHEN** image number 94 is read
- **THEN** its mask is all zero and its image contains no byte equal to 255

### Requirement: Background tile and tunnel shape content
The eight background tiles (94–101, for level plans 1–8) SHALL use only these palette indices: tile 1 {4, 6}, tile 2 {2, 4}, tile 3 {4, 6}, tile 4 {2, 6}, tile 5 {2, 4, 6}, tile 6 {2, 4}, tile 7 {2, 4, 6}, tile 8 {4, 6}. The six tunnel shape masks (102–107) SHALL be exactly pixel-doubled: every 2x2 block of real pixels holds a single value. (research 03 §2, §5.2)

#### Scenario: Tiles contain no black
- **WHEN** the image bytes of image numbers 94–101 are inspected
- **THEN** no byte is 0, so a tiled area never reads as empty to the pixel probe

#### Scenario: Tile colours for plan 4
- **WHEN** the image bytes of image number 97 are inspected
- **THEN** the set of values found is exactly {2, 6}

#### Scenario: Tunnel masks are doubled
- **WHEN** the mask of image number 102 is divided into 2x2 blocks of real pixels
- **THEN** all four bytes of every block are equal

### Requirement: Font glyph set
The game SHALL carry exactly 40 font glyphs: the letters A–Z, the digits 0–9, full stop, colon, a low underline and space. Each glyph MUST be 576 bytes, 24x24 real pixels (12x12 logical), one palette index per byte, using only the values 0 (background), 10 (body), 12 (shadow) and 15 (highlight). Non-zero bytes MUST lie within real columns 0–19 of the glyph. (research 03 §6)

#### Scenario: Glyph count and size
- **WHEN** the font data is enumerated
- **THEN** it holds 40 glyphs of 576 bytes each, 23040 bytes in total

#### Scenario: Glyph values
- **WHEN** every byte of every glyph is inspected
- **THEN** each byte is 0, 10, 12 or 15

#### Scenario: Built-in spacing
- **WHEN** real columns 20–23 of every glyph row are inspected
- **THEN** every byte is 0

#### Scenario: Space glyph
- **WHEN** the glyph for the space character is read
- **THEN** all 576 bytes are 0

### Requirement: Glyph coverage
Of the 95 character codes 32–126, exactly 66 SHALL have a glyph: space, full stop, the digits 0–9, colon, A–Z, underscore, and a–z. Each lower-case letter MUST map to the same glyph as its upper-case letter. Every other code, including hyphen, exclamation mark and slash, and every code outside 32–126, MUST be reported as having no glyph. (research 03 §6)

#### Scenario: Lower case shares upper case
- **WHEN** the glyphs for "g" and "G" are looked up
- **THEN** both lookups yield the same 576 bytes

#### Scenario: Hyphen is not available
- **WHEN** the glyph for "-" is looked up
- **THEN** the lookup reports that the character has no glyph

#### Scenario: Supported code count
- **WHEN** all codes 32–126 are looked up
- **THEN** exactly 66 have a glyph and 29 do not

#### Scenario: Code outside the table
- **WHEN** the glyph for code 200 or code 10 is looked up
- **THEN** the lookup reports that the character has no glyph

### Requirement: Title picture data
The title picture SHALL be carried as the upstream 2772-byte zlib-format stream (first two bytes 0x78 0xDA) and MUST decompress to exactly 256000 bytes: a 640x400 row-major picture of palette indices using only the values 0, 4, 7, 8 and 15. Decompression failure or a result of any other length MUST be reported as an error, not as a partial picture. (research 03 §9.1)

#### Scenario: Decompressed size
- **WHEN** the committed title stream is decompressed
- **THEN** the result is exactly 256000 bytes

#### Scenario: Title colours
- **WHEN** every byte of the decompressed picture is inspected
- **THEN** the set of values found is exactly {0, 4, 7, 8, 15}

#### Scenario: Corrupt stream
- **WHEN** a truncated copy of the stream (its first 1000 bytes) is decompressed
- **THEN** an error is reported and no picture is produced

### Requirement: Title picture content landmarks
The decompressed title picture SHALL have real rows 0–34 entirely black, and a frame of palette index 4: horizontal bars on real rows 35–38 and 357–360, and vertical bars on real columns 0–5, 320–327 and 634–639 spanning rows 35–360. Real rows 363–398 hold the copyright artwork in indices 7, 8 and 15. (research 03 §9.1)

#### Scenario: Top margin is black
- **WHEN** real rows 0–34 of the picture are inspected
- **THEN** all 22400 bytes are 0

#### Scenario: Frame bars
- **WHEN** the pixels at real (column 3, row 200), (324, 200), (636, 200), (100, 36) and (100, 358) are read
- **THEN** each is palette index 4

### Requirement: Window icon data
The game SHALL carry the upstream window icon as a 64x64 picture of 4096 pixels with 8 bits each of red, green, blue and alpha per pixel, row-major, 16384 bytes in total, with the same colour and alpha values as the upstream table. (research 03 §10)

#### Scenario: Icon size
- **WHEN** the committed icon data is read
- **THEN** it is exactly 16384 bytes, 4 bytes for each of 64x64 pixels

#### Scenario: Channel order is preserved from upstream
- **WHEN** an upstream icon entry has the 32-bit value 0xFF112233 (alpha 0xFF, red 0x11, green 0x22, blue 0x33)
- **THEN** the corresponding pixel in the committed data has red 0x11, green 0x22, blue 0x33 and alpha 0xFF

### Requirement: Committed asset integrity
Every committed asset file SHALL be listed in a committed manifest with its byte length and a SHA-256 digest, and the automated test suite MUST fail when any asset file differs from its manifest entry, when a listed file is missing, or when an unlisted file is present in the asset directory. (research 03 §12.1, §12.3)

#### Scenario: Blob changed by accident
- **WHEN** one byte of the committed sprite data is altered without updating the manifest
- **THEN** the test suite fails and names the file whose digest does not match

#### Scenario: Unlisted file
- **WHEN** a file that the manifest does not list is added to the asset directory
- **THEN** the test suite fails and names the unlisted file

#### Scenario: Untouched assets
- **WHEN** the test suite runs on an unmodified checkout
- **THEN** every asset file matches its manifest length and digest

### Requirement: Assets are embedded and self-contained
The sprite, font and title-picture data SHALL be part of the built program. A normal build and a normal run MUST NOT need the upstream source tree, network access or any asset file at run time. (research 03 §12.1)

#### Scenario: Build without upstream
- **WHEN** the workspace is built and tested on a machine that has no upstream checkout and no network access
- **THEN** the build and all asset and rendering tests succeed

#### Scenario: Run from another directory
- **WHEN** a built program that draws image number 1 is run after the repository's asset directory has been made unavailable
- **THEN** the image is drawn correctly

### Requirement: One-off asset import command
The developer command `xtask import-assets` SHALL take the path of an upstream checkout, read the VGA sprite tables and their 240-entry index table, the 40 VGA glyphs and their 95-entry character table, the title stream and the icon table as data, and write the asset files, the generated index and the manifest. Running it twice on the same input MUST produce byte-identical output. (research 03 §5, §6, §9.1, §10, §12.1)

#### Scenario: Successful import
- **WHEN** the command is run against a checkout of upstream commit 638125e
- **THEN** it writes the sprite, font, title and icon files, the index and the manifest, and exits with status 0

#### Scenario: Deterministic output
- **WHEN** the command is run twice against the same checkout
- **THEN** every written file is byte-identical between the two runs

#### Scenario: Shared upstream arrays
- **WHEN** several image numbers refer to the same upstream array (for example the all-zero 960-byte array used with sizes 4,15, 6,6 and 2,18)
- **THEN** each image number yields the first 8w x 2h bytes of that array for its own size

### Requirement: Import validation and failure
The import command SHALL verify what it reads before writing anything: 120 image/mask pairs whose array lengths fit the fixed sizes, 40 glyphs of 576 bytes, a 95-entry character table, a 2772-byte title stream that decompresses to 256000 bytes, and 4096 icon entries. On any mismatch, missing file or unreadable table it MUST exit with a non-zero status, name the offending file and table, and leave the existing asset files unchanged. (research 03 §5.1, §6, §9.1, §10)

#### Scenario: Wrong checkout
- **WHEN** the command is given a directory that does not contain the upstream sprite source file
- **THEN** it exits with a non-zero status, names the missing file and writes nothing

#### Scenario: Array of unexpected length
- **WHEN** an upstream image array for a 4,15 image holds 959 bytes instead of 960
- **THEN** the command exits with a non-zero status, names the array and the expected length, and leaves the committed assets unchanged

#### Scenario: Malformed table text
- **WHEN** a table contains a token that is not a number where a byte value is expected
- **THEN** the command reports the file and line and exits with a non-zero status without panicking

### Requirement: Import check mode
The import command SHALL offer a check mode that performs the full import in memory and compares the result with the committed asset files without modifying them, exiting with status 0 when all files are identical and with a non-zero status listing each differing file otherwise. (research 03 §12.1)

#### Scenario: Committed assets match upstream
- **WHEN** check mode is run against a checkout of upstream commit 638125e on an unmodified repository
- **THEN** it exits with status 0 and modifies no file

#### Scenario: Drift detected
- **WHEN** check mode is run after the committed font file has been altered
- **THEN** it exits with a non-zero status, names the font file, and leaves it as it was

### Requirement: CGA data is not imported
The import command SHALL ignore the upstream CGA sprite and CGA font tables, and the committed assets MUST NOT contain CGA graphics. (research 03 §1, §5.3)

#### Scenario: CGA tables present upstream
- **WHEN** the command is run against a checkout that contains the CGA sprite source file
- **THEN** no output file contains data from it and the manifest lists no CGA asset
