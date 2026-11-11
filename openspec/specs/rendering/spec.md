# rendering Specification

## Purpose
Defines the game's picture: a 640x400 buffer of palette indices that game
logic addresses as 320x200, and every pixel operation on it. The picture is
part of the game state — fireballs read it back — so its contents are
specified exactly and do not depend on whether anything is displayed.

## Requirements

### Requirement: Framebuffer geometry and logical addressing
The picture SHALL be a 640x400 row-major buffer of 256000 palette indices. All drawing operations MUST take logical coordinates in a 320x200 space: logical (x, y) maps to real column 2x and real row 2y. Widths are given in units of 4 logical pixels and heights in logical pixels, so a rectangle of width w and height h covers 8w x 2h real pixels. (research 03 §2, §4)

#### Scenario: Logical to real mapping
- **WHEN** a 4,15 image is drawn at logical (12, 18)
- **THEN** it covers real columns 24–55 and real rows 36–65

#### Scenario: Buffer size
- **WHEN** the picture is handed to a presenter
- **THEN** it consists of exactly 256000 index values, 400 rows of 640

### Requirement: Initial state and clearing
A new picture SHALL have every pixel at index 0, palette 0 selected and normal intensity. Clearing MUST set all 256000 pixels to index 0 and MUST NOT change the selected palette, the intensity or any sprite bookkeeping. (research 03 §3, §4.2)

#### Scenario: Fresh picture
- **WHEN** a picture is created and nothing has been drawn
- **THEN** all pixels are 0 and the active colours are those of palette 0 at normal intensity

#### Scenario: Clear keeps the palette
- **WHEN** palette 1 is selected, text is drawn and the picture is cleared
- **THEN** all pixels are 0 and palette 1 at normal intensity is still active

### Requirement: Palette colour values
There SHALL be two 16-entry palettes, each in a normal and an intense variant, with exactly the red, green, blue values (0–255) listed in the scenarios below. Entries 0 (0,0,0) and 15 (255,255,255) are the same in all four tables. (research 03 §3)

#### Scenario: Palette 0, normal intensity
- **WHEN** palette 0 is active at normal intensity
- **THEN** entries 0–15 are, in order: (0,0,0), (0,0,128), (0,128,0), (0,128,128), (128,0,0), (128,0,128), (128,64,0), (128,128,128), (64,64,64), (0,0,255), (0,255,0), (0,255,255), (255,0,0), (255,0,255), (255,255,0), (255,255,255)

#### Scenario: Palette 0, intense
- **WHEN** palette 0 is active at high intensity
- **THEN** entries 0–15 are, in order: (0,0,0), (0,0,255), (0,255,0), (0,255,255), (255,0,0), (255,0,255), (255,128,0), (196,196,196), (128,128,128), (128,128,255), (128,255,128), (128,255,255), (255,128,128), (255,128,255), (255,255,128), (255,255,255)

#### Scenario: Palette 1, normal intensity
- **WHEN** palette 1 is active at normal intensity
- **THEN** entries 0–15 are, in order: (0,0,0), (0,128,0), (128,0,0), (128,64,0), (0,0,128), (0,128,128), (128,0,128), (128,128,128), (64,64,64), (0,255,0), (255,0,0), (255,255,0), (0,0,255), (0,255,255), (255,0,255), (255,255,255)

#### Scenario: Palette 1, intense
- **WHEN** palette 1 is active at high intensity
- **THEN** entries 0–15 are, in order: (0,0,0), (0,255,0), (255,0,0), (255,128,0), (0,0,255), (0,255,255), (255,0,255), (196,196,196), (128,128,128), (128,255,128), (255,128,128), (255,255,128), (128,128,255), (128,255,255), (255,128,255), (255,255,255)

### Requirement: Palette and intensity selection
Selecting a palette (0 or 1) SHALL also reset the intensity to normal. Selecting an intensity (normal or high) MUST keep the currently selected palette. Neither selection may change any pixel; the change affects only how indices are translated to colours from then on. (research 03 §3, §13 item 10)

#### Scenario: Palette selection resets intensity
- **WHEN** high intensity is active on palette 0 and palette 0 is selected again
- **THEN** the active colours are those of palette 0 at normal intensity

#### Scenario: Intensity keeps the palette
- **WHEN** palette 1 is selected and then high intensity is selected
- **THEN** the active colours are those of palette 1, intense

#### Scenario: Pixels untouched
- **WHEN** the palette or the intensity is changed
- **THEN** the frame checksum of the picture is the same before and after

### Requirement: Presentation output
At any moment a presenter SHALL be able to read the 256000 indices together with the 16 active colours, and expanding every index through those colours MUST give the picture to show. Only indices 0–15 ever appear in the picture. Reading the picture MUST NOT change it. (research 03 §3, §11)

#### Scenario: Whole frame follows the palette
- **WHEN** a picture containing index 10 is read under palette 0 normal, and again after palette 1 is selected
- **THEN** those pixels expand to (0,255,0) the first time and (255,0,0) the second time

#### Scenario: Nothing displayed
- **WHEN** drawing operations are performed and no presenter ever reads the picture
- **THEN** the pixel contents are identical to a run in which every frame was presented

### Requirement: Masked image drawing
Drawing image number n at logical (x, y) SHALL process each byte of the image with the mask byte at the same position: if the image byte is 255 the destination pixel is left unchanged; otherwise the destination becomes (destination AND mask) OR image. The x coordinate is used as given, without alignment. (research 03 §4.5, §13 item 9)

#### Scenario: Transparent pixel
- **WHEN** an image byte is 255 and the destination pixel is 6
- **THEN** the destination pixel stays 6

#### Scenario: Opaque coloured pixel
- **WHEN** an image byte is 14 with mask 0 and the destination pixel is 6
- **THEN** the destination pixel becomes 14

#### Scenario: Black outline pixel
- **WHEN** an image byte is 0 with mask 0 and the destination pixel is 6
- **THEN** the destination pixel becomes 0

#### Scenario: Zero image under a keep mask
- **WHEN** an image byte is 0 with mask 255 and the destination pixel is 6
- **THEN** the destination pixel stays 6

### Requirement: Clipping of drawing operations
Every drawing operation SHALL affect only pixels inside the 640x400 picture. Parts of an image, glyph or restored rectangle that fall outside any edge MUST be discarded, the in-bounds part MUST land at its correct position, and no out-of-bounds coordinate may cause a failure. (research 03 §4.3–§4.5, §13 item 7)

#### Scenario: Bottom edge
- **WHEN** a 6,6 tunnel shape is drawn at logical (8, 195)
- **THEN** logical rows 195–199 of the shape are drawn and the sixth row is discarded

#### Scenario: Fully outside
- **WHEN** an image is drawn at logical (400, 300) or at (-40, -40)
- **THEN** the picture is unchanged and no error occurs

#### Scenario: Partly left of the picture
- **WHEN** a 4,15 image is drawn at logical (-8, 20)
- **THEN** only its right 8 logical columns appear, at logical columns 0–7

### Requirement: Rectangle capture
Capturing a rectangle of width w and height h at logical (x, y) SHALL produce a saved copy of 8w x 2h real pixels holding the picture's indices at that place. Any part of the rectangle lying outside the picture MUST read as index 0 in the saved copy, with the in-bounds part at its correct offset. Capturing MUST NOT change the picture. (research 03 §4.3)

#### Scenario: In-bounds capture
- **WHEN** a 4,15 rectangle is captured at logical (12, 18)
- **THEN** the saved copy holds the 32x30 real pixels starting at real (24, 36)

#### Scenario: Capture across the bottom edge
- **WHEN** a 6,6 rectangle is captured at logical (8, 195)
- **THEN** the first 10 real rows of the saved copy hold picture rows 390–399 and the last 2 real rows are 0

### Requirement: Rectangle restore
Restoring a saved copy at logical (x, y) SHALL overwrite the picture with every pixel of the copy, index 0 included, using the size recorded in the copy itself, clipped to the picture. A capture followed by a restore at the same position MUST leave the picture exactly as it was at the time of capture. (research 03 §4.4, §13 item 6)

#### Scenario: Round trip
- **WHEN** a rectangle is captured, the same area is drawn over, and the copy is restored at the same position
- **THEN** the picture is byte-identical to its state at the time of capture

#### Scenario: Restore is opaque
- **WHEN** a saved copy containing index 0 pixels is restored over a non-black area
- **THEN** those pixels become 0

#### Scenario: Copy keeps its own size
- **WHEN** a copy captured as 4,15 is restored
- **THEN** exactly 32x30 real pixels are written regardless of what is currently drawn at that place

### Requirement: Glyph drawing
Drawing a character at logical (x, y) SHALL overwrite a full 12x12 logical cell (24x24 real pixels) with the character's glyph, background pixels included, subject to clipping. The x coordinate is used as given, without alignment. Drawing a single character that has no glyph MUST leave the picture unchanged. (research 03 §4.7, §6)

#### Scenario: Cell is overwritten
- **WHEN** the letter "A" is drawn at logical (100, 0) over a non-black area
- **THEN** all 576 real pixels of the cell at real (200, 0) hold glyph values and none of the previous content remains

#### Scenario: Unaligned position
- **WHEN** a character is drawn at logical (101, 3)
- **THEN** its cell starts at real column 202 and real row 6

#### Scenario: Character without a glyph
- **WHEN** the single character "-" is drawn
- **THEN** the picture is unchanged

### Requirement: Text colours
Text SHALL be drawn in one of four colours, numbered 0–3, by recolouring glyph bytes: body value 10 becomes 10, 10, 12 or 14 for colours 0, 1, 2 and 3; shadow value 12 becomes 12, 2, 4 or 6. Glyph values 0 and 15 MUST be drawn unchanged in every colour. (research 03 §4.7)

#### Scenario: Colour 0
- **WHEN** a glyph is drawn in colour 0
- **THEN** its pixels are exactly the stored glyph bytes (body 10, shadow 12, highlight 15, background 0)

#### Scenario: Colour 3
- **WHEN** a glyph is drawn in colour 3
- **THEN** body pixels are 14, shadow pixels are 6, highlight pixels are 15 and background pixels are 0

#### Scenario: Colours 1 and 2
- **WHEN** a glyph is drawn in colour 1 and then in colour 2
- **THEN** body and shadow are 10 and 2 for colour 1, and 12 and 4 for colour 2

### Requirement: String drawing
Drawing a string at logical (x, y) SHALL draw its characters left to right in cells 12 logical pixels apart, all in the given colour. A character without a glyph MUST be drawn as a space, erasing its cell. Lower-case letters appear as upper case. Erasing n text cells at (x, y) MUST be identical to drawing a string of n spaces there. (research 03 §6)

#### Scenario: Cell advance
- **WHEN** "D I G G E R" (11 characters) is drawn at logical (100, 0)
- **THEN** its cells start at x = 100, 112, 124, … 220 and it covers logical columns 100–231 of rows 0–11

#### Scenario: Unsupported character in a string
- **WHEN** "A-B" is drawn at logical (0, 0) over a non-black area
- **THEN** the cell at x = 12 is entirely index 0, and "A" and "B" appear at x = 0 and x = 24

#### Scenario: Lower case
- **WHEN** "abc" and "ABC" are drawn at the same position on two cleared pictures
- **THEN** the two pictures are identical

#### Scenario: Clearing the top line
- **WHEN** 26 cells are erased at logical (0, 0) and 1 cell at (308, 0)
- **THEN** logical rows 0–11 are index 0 across columns 0–319

### Requirement: Number drawing
Drawing a non-negative number n in a field of w cells at logical (x, y) SHALL write decimal digits from the right: the digit for 10^k goes in the cell at x + (w − 1 − k) × 12 for k = 0 … w − 1. The w − 1 rightmost cells are always drawn, zero included. The leftmost cell is drawn only when its digit is not zero and is otherwise left untouched, not erased. Digits above 10^(w−1) are not shown. (research 03 §6)

#### Scenario: Zero score
- **WHEN** 0 is drawn with w = 6 at logical (0, 0)
- **THEN** "00000" appears in the cells at x = 12 … 60 (columns 12–71) and the cell at x = 0 is unchanged

#### Scenario: Six-digit value
- **WHEN** 123456 is drawn with w = 6 at logical (0, 0)
- **THEN** the digits 1, 2, 3, 4, 5, 6 appear in the cells at x = 0, 12, 24, 36, 48, 60

#### Scenario: Leftmost cell is not erased
- **WHEN** 100000 is drawn with w = 6 at (0, 0) and then 99999 is drawn in the same field
- **THEN** the cell at x = 0 still shows "1" and the other five cells show "99999"

#### Scenario: Single-cell field
- **WHEN** 0 is drawn with w = 1
- **THEN** nothing is drawn

### Requirement: Background tiling
Drawing the background for level plan p (1–8) SHALL draw image number 93 + p, a 20x4 logical-pixel opaque tile, at every position x = 0, 20, … 300 and y = 14, 18, … 198: 16 columns by 47 rows, 752 tiles. The last row is clipped to logical rows 198–199. Logical rows 0–13 MUST NOT be touched. (research 03 §2, §8.2)

#### Scenario: Coverage
- **WHEN** the background for plan 1 is drawn on a cleared picture
- **THEN** every pixel of logical rows 14–199 is non-zero and every pixel of logical rows 0–13 is 0

#### Scenario: Plan selects the tile
- **WHEN** the background for plan 4 is drawn
- **THEN** the real pixels of logical rows 14–17, columns 0–19 equal the image of number 97, and the picture contains only the indices 0, 2 and 6

#### Scenario: Top line is preserved
- **WHEN** text is drawn on logical rows 0–11 and the background is then drawn
- **THEN** the text is unchanged

#### Scenario: Plan out of range
- **WHEN** a background is requested for plan 0 or plan 9
- **THEN** the request is rejected as invalid and the picture is unchanged

### Requirement: Pixel probe
Probing logical (x, y) with 0 ≤ x ≤ 319 and 0 ≤ y ≤ 199 SHALL examine the 8x2 block of real pixels whose top-left corner is real (2x, 2y), without aligning x. Bit (0x80 >> i) of the result is set when real column i (0–7) of the block is non-zero in either row; the result is then ANDed with 0xEE, so columns 3 and 7 never contribute. The probe sees everything in the picture: background, sprites and text alike. (research 03 §4.6; research 02 §3.6)

#### Scenario: Black area
- **WHEN** all 16 real pixels of the block are 0
- **THEN** the result is 0x00

#### Scenario: Fully covered area
- **WHEN** all 16 real pixels of the block are non-zero
- **THEN** the result is 0xEE

#### Scenario: Either row counts
- **WHEN** only the real pixel in column 0 of the block's second row is non-zero
- **THEN** the result is 0x80

#### Scenario: Masked columns
- **WHEN** only the real pixels in columns 3 and 7 of the block are non-zero
- **THEN** the result is 0x00

### Requirement: Pixel probe outside the picture
A probe with x > 319 or y > 199 SHALL return 0xFF, unmasked. Otherwise the block is located by position in the row-major sequence of 256000 pixels, at offset (2y + r) × 640 + 2x + i for row r and column i: positions past a row's end fall on the start of the next row, and positions below 0 or above 255999 read as 0. A probe MUST NOT fail for any coordinates. (research 03 §4.6, §13 item 2)

#### Scenario: Beyond the right or bottom edge
- **WHEN** logical (320, 50) or logical (10, 200) is probed
- **THEN** the result is 0xFF

#### Scenario: Wrap at the right edge
- **WHEN** logical (318, 10) is probed and the only non-zero pixel in the picture is at real column 0 of real row 22
- **THEN** the result is 0x08

#### Scenario: Past the end of the buffer
- **WHEN** logical (318, 199) is probed on a picture whose only non-zero pixels are real columns 636–639 of real row 399
- **THEN** the result is 0xE0: block columns 0–3 of the second row are read, columns 4–7 lie past the end of the buffer and read as 0, and no failure occurs

#### Scenario: Negative coordinates
- **WHEN** logical (-1, 0) is probed and the only non-zero pixel is at real (0, 0)
- **THEN** the result is 0x20, and probing (0, -1) on the same picture returns 0x00 without failure, because both rows of that block lie before the start of the buffer

### Requirement: Title picture drawing
Drawing the title picture SHALL replace all 256000 pixels of the picture with the decompressed title picture, whatever was there before. It MUST NOT change the selected palette or intensity. (research 03 §4.8, §9.1)

#### Scenario: Whole picture replaced
- **WHEN** the title picture is drawn over a picture full of sprites and text
- **THEN** the picture equals the decompressed title data byte for byte

#### Scenario: Room for the top text line
- **WHEN** the title picture has been drawn and "D I G G E R" is drawn at logical (100, 0) in colour 3
- **THEN** the text lies entirely within the black rows 0–34 of the title picture and the frame is untouched

### Requirement: Frame checksum
The picture SHALL provide a 32-bit checksum of its 256000 indices in row-major order, computed as CRC-32 with the IEEE 802.3 polynomial (reflected, initial value and final XOR 0xFFFFFFFF). Two pictures have the same pixels whenever golden tests compare equal checksums; the palette and intensity are not part of the checksum. (research 03 §12.3)

#### Scenario: Cleared picture
- **WHEN** the checksum of a cleared picture (256000 zero bytes) is taken
- **THEN** it equals the CRC-32 of 256000 zero bytes, and is the same on every platform

#### Scenario: Sensitive to one pixel
- **WHEN** a single pixel of the picture changes
- **THEN** the checksum changes

### Requirement: Deterministic rendering
Every drawing operation SHALL be a pure function of the picture's previous contents and the operation's arguments. The same sequence of operations MUST give byte-identical pictures on every supported platform and in every build configuration, with or without a display. (research 03 §11, §13 item 1)

#### Scenario: Repeated run
- **WHEN** the same sequence of clear, tile, image, text and restore operations is executed twice
- **THEN** both runs end with the same frame checksum

#### Scenario: Headless run
- **WHEN** the sequence is executed in an automated test with no window or display available
- **THEN** all operations succeed and the frame checksum equals the recorded golden value
