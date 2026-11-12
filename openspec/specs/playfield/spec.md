# playfield Specification

## Purpose
The playfield is the world the game is played in: a 15×10 grid of earth cells with tunnels, emeralds and a status line. This capability defines how levels are laid out and chosen, how earth is dug and remembered, and how the field appears on screen.

## Requirements

### Requirement: Grid geometry and pixel mapping
The playfield SHALL be a grid of 15 columns (0..14) by 10 rows (0..9) on a 320×200 logical screen. Cell (column h, row v) SHALL have its top-left corner at logical pixel x = h×20+12, y = v×18+18. Every moving entity's position SHALL stay within x 12..292 and y 18..180. A position's cell and remainder SHALL be h = (x−12) div 20, v = (y−18) div 18, with remainders (x−12) mod 20 and (y−18) mod 18. (research 01 §5.1, research 02 §0)

#### Scenario: Corner cells
- **WHEN** the pixel position of cell (0,0) and of cell (14,9) is computed
- **THEN** the results are (12,18) and (292,180)

#### Scenario: Position between cells
- **WHEN** an entity is at pixel (60,27)
- **THEN** it is in cell (2,0) with horizontal remainder 8 and vertical remainder 9

### Requirement: Level layout alphabet
A level layout SHALL be 10 rows of 15 characters. A space is solid earth; `S` is a cell tunnelled both horizontally and vertically; `H` is a horizontal tunnel; `V` is a vertical tunnel; `B` is solid earth holding a gold bag; `C` is solid earth holding an emerald. Any other character SHALL be treated as solid earth. The game SHALL ship eight built-in layouts numbered 1..8, taken unchanged from the game assets. (research 01 §5.5)

#### Scenario: Built-in layout 1
- **WHEN** built-in layout 1 is read row by row from the top
- **THEN** its rows are `S   B     HHHHS`, `V  CC  C  V B  `, `VB CC  C  V    `, `V  CCB CB V CCC`, `V  CC  C  V CCC`, `HH CC  C  V CCC`, ` V    B B V    `, ` HHHH     V    `, `C   V     V   C`, `CC  HHHHHHH  CC`

#### Scenario: Unknown character
- **WHEN** a layout contains the character `x` in a cell
- **THEN** that cell starts as solid earth with no bag and no emerald

### Requirement: Level number to layout sequence
Level numbers SHALL run from 1 to 1000. Levels 1..8 SHALL use layouts 1..8. Every level above 8 SHALL use layout (level mod 4) + 5, giving the sequence 1-8, then 6, 7, 8, then 5, 6, 7, 8 repeating. A level number that would exceed 1000 SHALL stay at 1000. (research 01 §5.5, §10 item 4)

#### Scenario: Levels after the eighth
- **WHEN** the layouts for levels 9, 10, 11, 12 and 13 are selected
- **THEN** they are layouts 6, 7, 8, 5 and 6

#### Scenario: Level cap
- **WHEN** level 1000 is completed
- **THEN** the next level is again level 1000 and uses layout 5

### Requirement: Difficulty index
Every level-dependent game parameter SHALL be computed from the difficulty index min(level, 10) of the player whose turn it is, so that difficulty stops rising after level 10 while layouts continue to cycle. (research 01 §5.5, research 02 §7)

#### Scenario: Difficulty frozen after level 10
- **WHEN** the level is 10, 11 or 250
- **THEN** the difficulty index is 10 in each case

#### Scenario: Low levels
- **WHEN** the level is 3
- **THEN** the difficulty index is 3

### Requirement: Opened start cells for two simultaneous diggers
When two diggers play simultaneously on the built-in layouts, layouts 3 and 4 SHALL be read as if row 9, columns 6 and 8 held `H`, so both diggers start in a tunnel. This substitution SHALL NOT apply to one-digger games, to custom level data, or when the game is configured to use its layouts verbatim. (research 01 §5.5, §10 item 7)

#### Scenario: Two diggers on level 3
- **WHEN** a two-digger game on built-in levels draws level 3
- **THEN** cells (6,9) and (8,9) start with a horizontal tunnel and hold neither bag nor emerald

#### Scenario: One digger on level 3
- **WHEN** a one-digger game draws level 3
- **THEN** cells (6,9) and (8,9) are as written in layout 3

### Requirement: Initial earth and tunnel state
Each cell's earth SHALL consist of five horizontal slices 4 pixels wide and six vertical slices 3 pixels high, plus an "untouched" mark. At level start a cell from ` `, `B` or `C` SHALL have all slices and be untouched; `H` SHALL have no horizontal slices; `V` SHALL have no vertical slices; `S` SHALL have neither. Cells from `S`, `H` or `V` SHALL NOT be untouched. A level number below 1 SHALL give an all-earth field with no bags and no emeralds. (research 01 §5.3, research 02 §0)

#### Scenario: Horizontal tunnel cell
- **WHEN** a level whose layout has `H` at a cell starts
- **THEN** that cell has all six vertical slices, no horizontal slices, and is not untouched

#### Scenario: Junction cell
- **WHEN** a level whose layout has `S` at a cell starts
- **THEN** that cell has no slices of either kind and is not untouched

### Requirement: Digging removes one slice per step
A digging move from pixel (x,y) SHALL remove exactly one slice. With h, v the mover's cell, xs = ((x−12) mod 20) div 4 and ys = ((y−18) mod 18) div 3: moving right removes horizontal slice xs of cell (h+1,v); moving down removes vertical slice ys of cell (h,v+1); moving left removes horizontal slice xs−1 of (h,v), or slice 4 of (h−1,v) when xs is 0; moving up removes vertical slice ys−1 of (h,v), or slice 5 of (h,v−1) when ys is 0. (research 01 §5.3, research 02 §0)

#### Scenario: First step right from a cell corner
- **WHEN** a digger at pixel (12,18) digs one step to the right
- **THEN** horizontal slice 0 of cell (1,0) is removed and no other slice changes

#### Scenario: First step left from a cell corner
- **WHEN** a digger at pixel (52,18) digs one step to the left
- **THEN** horizontal slice 4 of cell (1,0) is removed

#### Scenario: Step up inside a cell
- **WHEN** a digger at pixel (12,42) digs one step up
- **THEN** vertical slice 1 of cell (0,1) is removed

### Requirement: Untouched mark is cleared when a cell is fully crossed
A cell SHALL lose its untouched mark at the moment a digging move leaves it with no horizontal slices (for a horizontal move) or no vertical slices (for a vertical move). A cell with only some slices removed SHALL remain untouched. A cell "has a tunnel in it" when at least one of its eleven slices is missing. (research 01 §5.3, research 02 §0)

#### Scenario: Crossing a cell horizontally
- **WHEN** a digger digs five consecutive steps to the right starting at a cell corner
- **THEN** the cell entered has no horizontal slices, still has all six vertical slices, and is no longer untouched

#### Scenario: Partly dug cell
- **WHEN** only two horizontal slices of a cell have been removed
- **THEN** the cell has a tunnel in it and is still untouched

### Requirement: Dig steps never leave the grid
A dig step that would address a cell outside the 15×10 grid SHALL change no cell. Movers are stopped at the playfield limits before digging, so this case does not arise in normal play; it is given defined behaviour instead of the original's unchecked access. (research 01 §5.3, §9)

#### Scenario: Dig request beyond the right edge
- **WHEN** a dig step to the right is requested from pixel (292,18)
- **THEN** no cell of the field changes

### Requirement: Redrawing a level fully opens partly dug cells
Whenever the playfield is drawn in full (a new level, or the turn passing to the other player), every cell that is not untouched SHALL be changed before play resumes: if any vertical slice is missing, all six are removed; if any horizontal slice is missing, all five are removed. This changes game state, not only the picture, and SHALL happen even when nothing is displayed. Untouched cells SHALL be left as they are. (research 01 §5.3, §10 item 8, research 03 §8.2)

#### Scenario: Turn passes back in a two-player game
- **WHEN** player 1 dies leaving a not-untouched cell with one vertical slice missing, player 2 takes a turn, and the field is then redrawn for player 1
- **THEN** that cell has no vertical slices and is drawn as a full vertical tunnel

#### Scenario: Death without a redraw
- **WHEN** a one-player game continues on the same level after a death
- **THEN** partly dug cells keep exactly the slices they had

### Requirement: Per-player field memory
Each player of an alternating two-player game SHALL have their own level number, earth state, emeralds and bags. The field in play SHALL be saved for its player at the end of every round and restored when that player's field is next drawn in full. Two simultaneous diggers SHALL share one field. (research 01 §2.1, §5.3)

#### Scenario: Players do not share tunnels
- **WHEN** player 1 digs a tunnel and dies, and player 2's first turn begins
- **THEN** player 2's field shows the level's initial layout without player 1's tunnel

#### Scenario: Field restored for the returning player
- **WHEN** the turn returns to player 1
- **THEN** the tunnels, remaining emeralds and remaining bags are those saved at the end of player 1's last round

### Requirement: Emerald placement and counting
At level start every `C` cell of the layout SHALL hold one emerald for that player and no other cell SHALL. The number of emeralds remaining SHALL be the count of cells still holding one. An emerald SHALL leave the field when a digger eats it, when a hobbin destroys it, or when a falling bag enters its cell. (research 01 §5.4, research 02 §3.5)

#### Scenario: Emerald count of level 1
- **WHEN** level 1 starts
- **THEN** 30 emeralds remain

#### Scenario: Emerald under a falling bag
- **WHEN** a bag begins a fall step into the cell below it and that cell holds an emerald
- **THEN** the emerald is removed without any score and the remaining count drops by one

### Requirement: Background drawing
A full draw SHALL reset the palette to normal intensity and tile the area below the status line with the background tile of the level's layout number (1..8): 20×4-pixel tiles at every x from 0 to 300 in steps of 20 and every y from 14 to 198 in steps of 4, the last row clipped at the bottom of the screen. (research 03 §8.2)

#### Scenario: Background follows the layout number
- **WHEN** level 9 is drawn
- **THEN** the background uses the tile of layout 6

#### Scenario: Tile coverage
- **WHEN** a level has been drawn
- **THEN** every logical pixel from y = 14 to y = 199 not covered by a tunnel, emerald or sprite shows the background tile pattern

### Requirement: Tunnel drawing on a full draw
After the background, every cell that is not untouched SHALL be drawn open, column by column from the left and top to bottom within a column: a vertical opening clears the cell's full height with rounded ends, a horizontal opening its full width, and a join SHALL be carved toward the right neighbour and toward the lower neighbour when that neighbour has a tunnel in it. Openings SHALL be black. (research 03 §8.2, §8.3)

#### Scenario: Horizontal corridor
- **WHEN** a row of adjacent `H` cells is drawn
- **THEN** a continuous black corridor 18 pixels high spans those cells with no background showing between them

#### Scenario: Solid neighbours
- **WHEN** an `H` cell has untouched earth to its right
- **THEN** no opening is carved into the right neighbour

### Requirement: Tunnel carving shapes during play
When an entity at pixel (x,y) carves earth while moving, a black shape SHALL be drawn before it moves: right cap 8×18 at (x+16, y−1); left cap 8×18 at (x−8, y−1); top cap 24×6 at (x−4, y−6); bottom cap 24×6 at (x−4, y+15). A falling bag carves a toothed 24×6 shape at (x−4, y+17) together with a top cap on its first fall step, and a toothed 24×8 shape at (x−4, y+15) on later steps. Shapes SHALL be clipped at the screen edge. (research 03 §8.3, research 02 §0)

#### Scenario: Digger moving right
- **WHEN** a digger at (12,18) moves right
- **THEN** an 8×18 black cap is drawn with its top-left at (28,17)

#### Scenario: Carving at the bottom row
- **WHEN** a carving shape's rectangle extends below logical y = 199
- **THEN** only the part on the screen is drawn and no error occurs

### Requirement: Emerald drawing
An emerald in cell (h,v) SHALL be drawn as a 16×10-pixel image at x = h×20+12, y = v×18+21, and erased with the matching 16×10 blank image when removed. On a full draw all remaining emeralds SHALL be drawn after the tunnels and bags. (research 01 §5.1, research 03 §8.4)

#### Scenario: Emerald position
- **WHEN** the emerald of cell (3,1) is drawn
- **THEN** its image occupies logical pixels x 72..87, y 39..48

### Requirement: Progressive level draw
A full draw SHALL be revealed progressively: each opened cell and each emerald counts as one item and three items SHALL be drawn per frame. No game tick SHALL elapse during the draw, and the earth, emerald and bag state after the draw SHALL be the same as if it had been drawn at once. (research 01 §4, research 03 §8.2)

#### Scenario: Draw takes frames but no ticks
- **WHEN** level 1 (33 opened cells and 30 emeralds) is drawn
- **THEN** the draw occupies 21 frames and the tick counter does not change

### Requirement: Lives display
Outside gauntlet mode the status line SHALL show spare lives (lives − 1). For the first player or digger: up to four life icons at x = 80, 100, 120, 140, blank where no spare life remains; with more than four spare lives, one icon at x = 80 followed by `X` and the count at x = 100. For the second player or digger the display is mirrored: icons at x = 224, 204, 184, 164, or the count followed by `X` ending at x = 220 with one icon at 224. (research 01 §6.5, research 03 §8.5)

#### Scenario: Three lives
- **WHEN** a one-player game starts with 3 lives
- **THEN** life icons are shown at x = 80 and x = 100 and the slots at 120 and 140 are blank

#### Scenario: Many spare lives
- **WHEN** a player with unlimited-lives enabled holds 7 lives
- **THEN** one icon is shown at x = 80 followed by the text `X6` at x = 100

### Requirement: Gauntlet clock display
In gauntlet mode the lives display SHALL be replaced by the remaining time shown as minutes (right-aligned in three characters), a colon and two-digit seconds at x = 124 on the status line, where the displayed seconds are the clock value divided by 1,193,181, rounded down. It SHALL be redrawn every tick. (research 01 §6.5, §7.4)

#### Scenario: Two-minute gauntlet start
- **WHEN** a gauntlet game of 120 seconds starts
- **THEN** the status line shows `  2:00` at x = 124 and no life icons

### Requirement: Custom level data
The game SHALL accept custom level data of at least 1202 bytes: a 16-bit little-endian bonus-life score step followed by eight layouts, each 10 rows of exactly 15 characters with no separators. Bytes beyond the first 1202 SHALL be ignored. Shorter data SHALL be rejected with an error and SHALL NOT start a game. With custom data in use the two-digger start-cell substitution SHALL NOT apply. (research 01 §5.5)

#### Scenario: Valid custom levels
- **WHEN** 1202 bytes are supplied whose first two bytes are 0x10 0x27 and whose layout 1 differs from the built-in one
- **THEN** level 1 uses the supplied layout and the bonus-life step is 10000 points

#### Scenario: Truncated data
- **WHEN** 900 bytes are supplied as custom level data
- **THEN** the data is rejected with an error that states the expected and actual lengths

#### Scenario: Rows are fifteen characters wide
- **WHEN** custom data is read
- **THEN** byte 2 + (p×150) + (r×15) + c supplies column c of row r of layout p+1, for p in 0..7, r in 0..9, c in 0..14
