# sprite-layer Specification

## Purpose
Defines the layer that keeps the game's moving objects on the picture: 18
sprites and one slot for static drawing, their fixed stacking order, how
backgrounds are saved and restored when sprites are placed, moved and erased,
and the collision lists that are the game's only source of collisions.

## Requirements

### Requirement: Sprite slots and classes
The layer SHALL have exactly 18 sprite slots numbered 0–17, grouped into five classes in this order: class 0 bonus (slot 0), class 1 bags (slots 1–7), class 2 monsters (slots 8–13), class 3 fireballs (slots 14–15), class 4 diggers (slots 16–17). A nineteenth slot is reserved for static drawing; it is never enabled, never redrawn and never appears in a collision list. (research 03 §7.1; research 02 §0)

#### Scenario: Class of a slot
- **WHEN** the classes of slots 0, 7, 8, 13, 14 and 17 are queried
- **THEN** they are bonus, bag, monster, monster, fireball and digger

#### Scenario: Slot count per class
- **WHEN** the slots are counted by class
- **THEN** there are 1 bonus, 7 bag, 6 monster, 2 fireball and 2 digger slots

### Requirement: Initial sprite state
At start every slot SHALL be disabled, positioned at logical (0, 0), with no saved background, and all five collision lists SHALL be empty. A sprite's rectangle is its position together with the size of its current image: 4 × w logical pixels wide and h logical pixels high. (research 03 §7.2)

#### Scenario: Nothing enabled at start
- **WHEN** a new sprite layer is inspected
- **THEN** no slot is enabled and every collision list is empty

#### Scenario: Rectangle from image size
- **WHEN** a sprite showing a 4,15 image is at logical (112, 54)
- **THEN** its rectangle covers logical columns 112–127 and rows 54–68

### Requirement: Current and pending image
Each sprite SHALL have a current image, which defines what is on the picture and its rectangle, and a pending image. Setting the pending image MUST NOT change the picture, the current image, the rectangle or the collision lists; the pending image becomes current only at the sprite's next place or move operation. (research 03 §7.2, §7.3, §13 item 4)

#### Scenario: Pending image does not show yet
- **WHEN** an enabled sprite showing image 69 has its pending image set to 70
- **THEN** the picture is unchanged and the sprite's rectangle and current image are still those of image 69

#### Scenario: Latch on move
- **WHEN** that sprite is then moved
- **THEN** it is drawn with image 70 and image 70 is its current image

#### Scenario: Other sprites see the current size
- **WHEN** a monster sprite currently shows image 80 (4,14) and has a pending 4,15 image, and another sprite is moved to a position 14 rows below it
- **THEN** the collision test uses the 14-row rectangle and reports no collision

### Requirement: Sprite reset
Resetting a sprite with an image SHALL set both its current and its pending image to that image and mark it disabled, without changing the picture, the sprite's recorded position, its saved background or the collision lists. A sprite that was enabled is thereby forgotten without being erased. (research 03 §7.3, §13 item 15)

#### Scenario: Reset leaves pixels alone
- **WHEN** an enabled, visible sprite is reset
- **THEN** its image is still on the picture, the sprite is disabled and the frame checksum is unchanged

#### Scenario: Reset sprite no longer collides
- **WHEN** a sprite is reset and another sprite is then moved onto its old rectangle
- **THEN** the reset sprite is not in any collision list and is neither restored nor redrawn

### Requirement: Horizontal alignment
The place, move and static-draw operations SHALL round the given x coordinate down to a multiple of 4 before use (for negative values towards minus infinity). The y coordinate is never altered. The protected area declared for static drawing uses its x as given. (research 03 §2, §7.5, §13 item 8)

#### Scenario: Rounding down
- **WHEN** a sprite is placed at x = 115
- **THEN** its recorded position and its image are at x = 112

#### Scenario: Negative x
- **WHEN** a sprite is moved to x = -3
- **THEN** its recorded x is -4

#### Scenario: Already aligned
- **WHEN** a sprite is moved to x = 292, y = 181
- **THEN** its recorded position is (292, 181)

### Requirement: Rectangle overlap test
Two sprites overlap when their rectangles share at least one logical pixel: with left edges ax and bx, widths aw and bw, top edges ay and by and heights ah and bh, they overlap exactly when ax ≤ bx + bw − 1, bx ≤ ax + aw − 1, ay ≤ by + bh − 1 and by ≤ ay + ah − 1. The same test SHALL be used for redraw decisions and for collision lists. (research 03 §7.3, §7.4; research 02 §1)

#### Scenario: Two 16x15 sprites
- **WHEN** two sprites with 4,15 images are offset by dx and dy, dx a multiple of 4
- **THEN** they overlap exactly when |dx| ≤ 12 and |dy| ≤ 14

#### Scenario: Touching edges do not overlap
- **WHEN** two 4,15 sprites are at x = 100 and x = 116 on the same row
- **THEN** they do not overlap

#### Scenario: Fireball against a 16x15 sprite
- **WHEN** a fireball (2,8 image) is at (fx, fy) and a 4,15 sprite is at (sx, sy)
- **THEN** they overlap exactly when −4 ≤ fx − sx ≤ 12 and sy − 7 ≤ fy ≤ sy + 14

### Requirement: Redraw set
For a given starting rectangle, the redraw set SHALL contain every enabled sprite, other than the starting slot, that overlaps the starting rectangle, plus every enabled sprite that overlaps a sprite already in the set, repeated until nothing more is added. The starting slot's rectangle is used whether or not that slot is enabled; an enabled starting slot enters the set when it overlaps a member. Disabled sprites never enter the set. (research 03 §7.3)

#### Scenario: Chain of overlaps
- **WHEN** sprite A overlaps B, B overlaps C, A does not overlap C, and the set is built from A
- **THEN** the set contains both B and C

#### Scenario: Disabled sprites break the chain
- **WHEN** in the same arrangement B is disabled
- **THEN** the set contains neither B nor C

#### Scenario: Isolated sprite
- **WHEN** the set is built from a sprite that overlaps no enabled sprite
- **THEN** the set is empty

### Requirement: Fixed stacking order
Whenever several sprites are restored, drawn or have their backgrounds captured in one operation, they SHALL be processed in ascending slot number. Where sprites overlap, the higher slot therefore appears on top: bonus below bags, bags below monsters, monsters below fireballs, fireballs below diggers. (research 03 §7.3)

#### Scenario: Digger over bag
- **WHEN** a digger (slot 16) and a bag (slot 1) overlap and either of them is moved
- **THEN** in the overlapping area the digger's opaque pixels cover the bag's

#### Scenario: Order within a class
- **WHEN** monsters in slots 8 and 9 overlap
- **THEN** the monster in slot 9 is drawn over the monster in slot 8, whichever of the two was moved last

### Requirement: Placing a sprite
Placing sprite n at (x, y) SHALL, in this order: set its position to the aligned (x, y) and make its pending image current; build the redraw set from its new rectangle; restore the saved backgrounds of the set; capture the background under n's new rectangle; enable n; draw the set together with n. Placing MUST NOT change the collision lists and MUST NOT erase an image that n already had on the picture elsewhere. (research 03 §7.4, §13 item 5)

#### Scenario: First appearance
- **WHEN** a disabled sprite is placed on an empty area of the background
- **THEN** its image appears there, it is enabled, and its saved background holds the background pixels that were under it

#### Scenario: Placed under a higher sprite
- **WHEN** a bag (slot 1) is placed overlapping an enabled digger (slot 16)
- **THEN** the bag's saved background contains no digger pixels and the digger is drawn over the bag

#### Scenario: No collision output
- **WHEN** a sprite is placed overlapping other enabled sprites
- **THEN** the collision lists are exactly what they were before the placement

### Requirement: Moving a sprite
Moving sprite n to (x, y) SHALL, in this order: build the redraw set from n's old rectangle, add the set built from its new rectangle (aligned position, pending image size), and add n itself; restore the saved backgrounds of the set, n at its old position; enable n at the new position with its pending image made current; capture the background under n's new rectangle; draw the set; then compute the collision lists for n. (research 03 §7.4, §13 item 3)

#### Scenario: Old image removed
- **WHEN** an enabled sprite alone on the background is moved 4 pixels to the right
- **THEN** its old rectangle shows the background again and its image appears at the new position

#### Scenario: Neighbour at the old position is repaired
- **WHEN** sprite A overlaps sprite B and A is moved away so that they no longer overlap
- **THEN** B is completely drawn and no pixel of A remains in B's rectangle

#### Scenario: Neighbour at the new position is included
- **WHEN** sprite A is moved from a free area onto sprite B
- **THEN** A's saved background contains no pixels of B, and A and B are drawn in ascending slot order

#### Scenario: Animation in place
- **WHEN** a sprite's pending image is changed and it is moved to its current position
- **THEN** the new image replaces the old one, and pixels that the new image leaves transparent show the background, not the old image

### Requirement: Erasing a sprite
Erasing an enabled sprite n SHALL restore its saved background at its position, mark it disabled, and then draw (without restoring or re-capturing) every sprite in the redraw set built from n's rectangle. Erasing a disabled sprite MUST do nothing. Erasing never changes the collision lists and never discards n's saved background. (research 03 §7.4; research 02 §1)

#### Scenario: Erase a lone sprite
- **WHEN** a sprite alone on the background is erased
- **THEN** the picture equals the picture before the sprite was placed, and the sprite is disabled

#### Scenario: Erase under an overlapping sprite
- **WHEN** a bag overlapped by a monster is erased
- **THEN** the monster is still completely drawn and no bag pixel remains

#### Scenario: Erase twice
- **WHEN** an already erased sprite is erased again
- **THEN** the picture and all sprite state are unchanged

#### Scenario: Lists survive an erase
- **WHEN** a move has produced non-empty collision lists and another sprite is then erased
- **THEN** the collision lists are unchanged, even if they name the erased sprite

### Requirement: Saved backgrounds hold clean background
When sprites are used in the normal discipline (a disabled sprite is always placed before it is moved, and an enabled sprite is never placed), every saved background SHALL contain only static background pixels, never another sprite's pixels. Consequently the picture always equals the static background with the enabled sprites drawn over it in ascending slot order, and erasing all sprites leaves exactly the static background. (research 03 §7.6, §12.3)

#### Scenario: Erase everything
- **WHEN** an arbitrary sequence of place, move, erase and pending-image changes following the discipline is executed and all sprites are then erased
- **THEN** the picture is byte-identical to the picture before the first sprite was placed

#### Scenario: Picture equals composite
- **WHEN** such a sequence has been executed
- **THEN** the picture is byte-identical to a copy of the static background on which each enabled sprite's current image has been drawn at its position, in ascending slot order

### Requirement: Saved background lifetime
A sprite's saved background, once captured, SHALL be kept until the sprite's next capture; erase and reset do not discard it. A move always restores the moved sprite's saved background at the sprite's recorded position, even when the sprite is disabled. Restoring for a sprite that has never had a capture MUST do nothing. A saved background keeps the size it was captured with. (research 03 §4.4, §7.4, §7.6, §13 items 5–6)

#### Scenario: Move without ever placing
- **WHEN** a sprite that has never been placed is moved to (100, 50)
- **THEN** no restore happens, the sprite appears at (100, 50) and is enabled

#### Scenario: Move after erase
- **WHEN** a sprite is placed at (100, 50), erased, and then moved to (120, 50) without being placed again
- **THEN** its old saved background is first restored at (100, 50), and the sprite then appears at (120, 50)

#### Scenario: Size change between capture and restore
- **WHEN** a sprite captured with a 4,15 image has a 4,14 pending image and is moved
- **THEN** the full 4,15 saved background is restored at the old position before the 4,14 image is drawn

### Requirement: Placing an already enabled sprite
Placing a sprite that is already enabled SHALL follow the same steps as any placement. Its old image is not removed, and if at its new rectangle it overlaps another enabled sprite, its previous saved background is restored at the new position before the new capture. (research 03 §7.4)

#### Scenario: Old image stays
- **WHEN** an enabled sprite at (100, 50) is placed at (200, 50), away from all other sprites
- **THEN** its image is visible at both positions and only the one at (200, 50) belongs to the sprite

#### Scenario: Stale background pasted
- **WHEN** an enabled sprite at (100, 50) is placed at (200, 50) where it overlaps another enabled sprite
- **THEN** the background it had saved at (100, 50) is written at (200, 50) before its new background is captured there

### Requirement: Collision lists
The move operation SHALL, as its last step, replace all five collision lists: for each class, the list holds every enabled sprite of that class, other than the moved sprite, whose current rectangle overlaps the moved sprite's new rectangle, in ascending slot order. Game logic reads, per class, the first colliding slot and each following one. (research 03 §7.4; research 02 §1)

#### Scenario: One of each
- **WHEN** a digger (slot 16) is moved onto a spot where it overlaps the bag in slot 3 and the monster in slot 9
- **THEN** the bag list is [3], the monster list is [9], and the bonus, fireball and digger lists are empty

#### Scenario: Several of one class
- **WHEN** a fireball (slot 14) is moved so that it overlaps the monsters in slots 12, 8 and 10
- **THEN** the monster list is [8, 10, 12]

#### Scenario: Moved sprite is not listed
- **WHEN** a digger in slot 16 is moved and overlaps the digger in slot 17
- **THEN** the digger list is [17] and does not contain 16

#### Scenario: Disabled sprites are not listed
- **WHEN** a sprite is moved onto the rectangle of a sprite that was erased earlier
- **THEN** the erased sprite is in no list

### Requirement: Collision lists change only on a move
Only a move SHALL compute collision lists. Place, erase, reset, pending-image changes and static drawing MUST leave them untouched, so the lists always describe the most recent move, with every other sprite at the rectangle it had on the picture at that instant. The layer SHALL also allow game logic to empty all five lists without any drawing. (research 03 §7.4, §8.4, §13 item 3; research 02 §1)

#### Scenario: Lists are overwritten by the next move
- **WHEN** sprite A is moved and collides with a bag, and sprite B is then moved and collides with nothing
- **THEN** all five lists are empty

#### Scenario: Positions at the instant of the move
- **WHEN** monster 8 is moved onto the digger's rectangle and the digger is afterwards moved away
- **THEN** the lists after monster 8's move name the digger, and the lists after the digger's move reflect only the digger's new rectangle

#### Scenario: Emptying the lists
- **WHEN** game logic empties the lists after a move that found collisions
- **THEN** all five lists are empty and the picture is unchanged

### Requirement: Enabled is not the same as alive
Collision lists and redraw sets SHALL depend only on which sprites are enabled and on their rectangles. A sprite that game logic regards as dead or exploding but that is still enabled MUST be listed; a sprite that has been erased MUST NOT be listed even if game logic regards it as alive. (research 02 §1)

#### Scenario: Gravestone is listed
- **WHEN** a digger sprite showing a gravestone image is enabled and a monster is moved onto it
- **THEN** the digger list contains that digger's slot

#### Scenario: Blinking digger is not listed
- **WHEN** a digger sprite has just been erased for a blink frame and a monster is moved onto its former rectangle
- **THEN** the digger list is empty

### Requirement: Static drawing under sprites
Drawing a static image of size w,h at (x, y) with sprite protection SHALL, in this order: build the redraw set from the area at the unaligned x; restore the saved backgrounds of the set; draw the image, masked, at the aligned x; capture the backgrounds of the set again; draw the set. The static image thereby becomes part of the background beneath every sprite. Collision lists are not changed. (research 03 §7.5)

#### Scenario: Tunnel carved under a digger
- **WHEN** a tunnel shape is drawn with protection in an area overlapped by an enabled digger
- **THEN** the digger is still completely drawn on top, and when the digger is later erased the carved shape is visible where the digger stood

#### Scenario: No sprite nearby
- **WHEN** a static image is drawn with protection in an area that no enabled sprite overlaps
- **THEN** the result is identical to drawing the image directly

#### Scenario: Sprites outside the area are untouched
- **WHEN** a static image is drawn with protection and an enabled sprite overlaps neither the area nor any sprite in the redraw set
- **THEN** that sprite is neither restored, re-captured nor redrawn

### Requirement: Unprotected static drawing
The layer SHALL also offer static drawing without sprite protection: the image is drawn, masked, at the aligned x and the given y, and no sprite is restored, re-captured or redrawn. It is intended for areas no sprite can overlap, such as background tiles and the top line. (research 03 §7.5, §8.2)

#### Scenario: Tile drawing
- **WHEN** background tile image 94 is drawn without protection at (20, 14)
- **THEN** the picture changes only inside logical columns 20–39, rows 14–17, and no sprite state changes

#### Scenario: Alignment applies
- **WHEN** an image is drawn without protection at x = 82
- **THEN** it is drawn at x = 80

### Requirement: Sprite state is independent of display
The sprite layer SHALL behave identically whether or not the picture is ever presented: positions, enabled flags, saved backgrounds, the picture contents and the collision lists are fully determined by the sequence of operations. (research 02 §1; research 03 §13 item 1)

#### Scenario: Headless collisions
- **WHEN** a sequence of sprite operations is executed in an automated test without a display
- **THEN** the collision lists and the frame checksum after each operation equal those of any other run of the same sequence
