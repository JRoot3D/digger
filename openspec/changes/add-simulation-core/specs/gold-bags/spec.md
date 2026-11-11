# Spec Delta

## Purpose

Gold bags are the movable hazards and prizes of the playfield. This capability defines where bags come from, when they wobble and fall, when they break into gold, how they are pushed, what they crush, and who collects the gold.

## ADDED Requirements

### Requirement: Bag creation
At level start one bag SHALL be created at the corner of every `B` cell of the layout, scanning columns left to right and each column top to bottom, up to a maximum of 7; further `B` cells SHALL be ignored. Bags are numbered in creation order and updated in that order every tick. Each bag is 16×15 pixels, starts at rest, unbroken and never fallen. (research 02 §5.1)

#### Scenario: Bags of level 1
- **WHEN** level 1 starts
- **THEN** seven bags exist, the first at cell (1,2) and the second at cell (4,0)

#### Scenario: Too many bags
- **WHEN** a custom layout contains nine `B` cells
- **THEN** only the first seven in column-then-row order become bags

### Requirement: Bags persist per player across rounds
At the end of every round the bags SHALL be tidied and saved for the player: a bag is removed if it is in cell (7,9), is not exactly on a cell corner, has broken into gold, has a fall in progress or recorded, or is wobbling. The remaining bags SHALL be restored when that player's field is next drawn or the next round on the same level begins. (research 02 §5.1, §5.8)

#### Scenario: Wobbling bag at round end
- **WHEN** a round ends while a bag is wobbling
- **THEN** that bag does not exist in the next round

#### Scenario: Resting bag survives a death
- **WHEN** a digger dies while a bag that was pushed earlier is at rest on the corner of cell (5,0)
- **THEN** the bag is in cell (5,0) at the start of the next round

### Requirement: When a bag starts to wobble
A bag at rest on a cell corner above the bottom row SHALL start wobbling on a tick when the cell below it has a tunnel in it, unless a living, non-invincible digger whose last move was up or down is in the bag's column, in the bag's row or the row below. A resting bag on the bottom row or off a cell corner SHALL NOT wobble. (research 02 §5.3)

#### Scenario: Tunnel dug under a bag
- **WHEN** a digger tunnels horizontally through the cell below a resting bag
- **THEN** the bag starts wobbling on the first bag update after the first slice of that cell is removed

#### Scenario: Digger climbing under a bag
- **WHEN** a digger is moving up the column directly under a bag
- **THEN** the bag does not start wobbling while the digger keeps moving vertically in the row below or in the bag's own row, and starts once the digger stops or moves sideways

### Requirement: Wobble timing
A wobbling bag SHALL count down from 15, one per tick, and begin to fall on the tick after the count reaches 0: the first fall step happens 16 ticks after the wobble began. On each countdown tick where the new count modulo 8 is even, the bag SHALL be redrawn, alternately upright and tilted to one side then the other, and the wobble cue emitted. Once wobbling, a bag falls even if the cause is gone. (research 02 §5.3)

#### Scenario: Wobble then fall
- **WHEN** a bag starts wobbling on tick W
- **THEN** it stays in place through tick W+15 and moves down 6 pixels on tick W+16

### Requirement: Falling
A falling bag SHALL move down 6 pixels per tick. On each fall step it SHALL carve its tunnel shape, remove one vertical earth slice of the cell below, remove without score any emerald in the cell below, and be drawn in its falling look. A bag starting to fall SHALL emit the fall cue. (research 02 §5.3, §5.5)

#### Scenario: Fall speed
- **WHEN** a bag falls through open space
- **THEN** it descends one cell every 3 ticks

### Requirement: Landing
A falling bag on a cell corner SHALL land when it is on the bottom row or when the cell below it is solid earth with no slice removed; otherwise it continues. On landing it comes to rest, is drawn upright, and every other bag overlapping it SHALL be removed. (research 02 §5.3, §5.4)

#### Scenario: Shaft with a floor
- **WHEN** a bag falls down a vertical tunnel that ends above solid earth
- **THEN** it lands on the last tunnelled cell

#### Scenario: Landing on another bag
- **WHEN** a falling bag lands overlapping a resting bag
- **THEN** the resting bag is removed

### Requirement: Breaking after two or more cells
A falling bag SHALL count each cell corner it reaches after leaving its starting cell. A bag that lands with a count of 2 or more SHALL break into gold; a bag that lands after one cell SHALL stay intact with its count reset. (research 02 §5.4)

#### Scenario: One-cell drop
- **WHEN** a bag falls exactly one cell and lands
- **THEN** it is an unbroken bag at rest and can wobble and fall again

#### Scenario: Two-cell drop
- **WHEN** a bag falls two cells and lands on tick T
- **THEN** it is gold from tick T, and the break cue is emitted with the first gold frame on tick T+1

### Requirement: Gold lifetime
Gold SHALL show three opening frames on the 1st, 3rd and 5th ticks after landing and SHALL disappear when its age counter, 1 on the landing tick and rising by 1 per tick, reaches 150 − 10 × difficulty index. (research 02 §5.2, §7)

#### Scenario: Lifetime table
- **WHEN** the difficulty index is 1, 2, 3, 4, 5, 6, 7, 8, 9 or 10
- **THEN** the gold lifetime value is 140, 130, 120, 110, 100, 90, 80, 70, 60 or 50

#### Scenario: Level 1 gold
- **WHEN** a bag breaks on tick T of level 1 and is left alone on undug ground
- **THEN** the gold disappears on tick T+139

### Requirement: Gold over a dug cell vanishes early
If gold lies above the bottom row and the cell below it is no longer untouched, then on any tick its age is more than 10 short of the lifetime the age SHALL jump to lifetime − 10, so the gold disappears 10 ticks later. (research 02 §5.2, §9 item 11)

#### Scenario: Digging under gold
- **WHEN** fresh gold on level 1 has the cell below it fully crossed by a digger on tick D
- **THEN** the gold disappears on tick D+10 at the latest

### Requirement: Collecting gold
Gold SHALL be collected when anything pushes against it in any direction. Every living digger overlapping the gold at that moment SHALL score 500 points and the gold cue SHALL be emitted; if no living digger overlaps it, a monster has taken it, no points are scored and that monster's delay is cleared. The gold is removed and the pusher is not blocked. (research 02 §5.6, §9 item 12)

#### Scenario: Digger walks into gold
- **WHEN** a digger's move brings it to overlap gold
- **THEN** it scores 500, the gold disappears and the digger keeps its new position

#### Scenario: Digger and monster together
- **WHEN** a monster's move touches gold that a living digger also overlaps
- **THEN** the digger scores 500 and the monster receives no delay reset

#### Scenario: Gold from below
- **WHEN** a digger moving up comes to overlap gold
- **THEN** the gold is collected and the digger is not turned back

### Requirement: Horizontal push
A bag at rest or sliding SHALL move 4 pixels when pushed left or right, stop any wobble, and take the push direction as its sliding direction. A push SHALL fail, leaving the bag where it was, if the bag is at x = 292 pushed right or x = 12 pushed left, or if after moving it overlaps a living digger or any monster. Bags SHALL NOT be pushable up or down. (research 02 §5.5)

#### Scenario: Free push
- **WHEN** a digger pushes a resting bag at (92,18) to the right
- **THEN** the bag is at (96,18) and sliding right

#### Scenario: Monster on the far side
- **WHEN** a pushed bag would come to overlap a monster
- **THEN** the bag stays where it was and the pusher is blocked

#### Scenario: Wobbling bag pushed
- **WHEN** a wobbling bag is pushed sideways successfully
- **THEN** it stops wobbling and its wobble count returns to 15

### Requirement: Failed push reverses the bag
After a failed horizontal push the bag's sliding direction SHALL be the opposite of the push. A bag that was between cells therefore rolls back by itself to the cell corner it came from; a bag on a cell corner settles or drops on its next update. (research 02 §5.5, §9 item 9)

#### Scenario: Blocked between cells
- **WHEN** a bag sliding right, 8 pixels past a cell corner, is pushed right against a monster
- **THEN** on the following ticks it moves left 4 pixels per tick until it is back on the cell corner

### Requirement: Push chains
A pushed bag that comes to overlap other bags SHALL push every one of them in the same direction, attempting all of them even after one fails. If any of them fails, the pushed bag is returned to where it was and its own push fails. A chain therefore moves as a whole or not at all at its head. (research 02 §5.5)

#### Scenario: Two bags in a row
- **WHEN** a digger pushes a bag that touches a second free bag
- **THEN** both bags move 4 pixels

#### Scenario: Chain against the wall
- **WHEN** the second bag is at x = 292 and the chain is pushed right
- **THEN** the first bag stays in place, the pusher is blocked, and the first bag's sliding direction is left

### Requirement: Sliding continues to the cell corner
A sliding bag SHALL keep moving 4 pixels per tick in its sliding direction during the bag update until it is on a cell corner. A single push counter shared by all bags is set by every horizontal push; the first sliding bag updated while it is set clears it and skips its own move for that update. (research 02 §5.3, §9 item 9)

#### Scenario: One push, one cell
- **WHEN** a resting bag on a cell corner is pushed right once on tick N by a digger and not touched again
- **THEN** it is 4 pixels along after tick N, moves 4 pixels on each of ticks N+1 to N+4, and is on the next cell corner after tick N+4

#### Scenario: Shared counter with two sliding bags
- **WHEN** two bags are pushed in a chain on the same tick
- **THEN** in that tick's bag update the lower-numbered sliding bag skips its move and the other one moves a further 4 pixels

### Requirement: End of a slide
A sliding bag found on a cell corner at its update SHALL start falling at once, with no wobble and with the fall cue, if it is above the bottom row and the cell below has a tunnel in it; otherwise it SHALL come to rest with its wobble count reset. (research 02 §5.3)

#### Scenario: Pushed over a shaft
- **WHEN** a bag is pushed onto a cell corner above a vertical tunnel
- **THEN** it starts falling on the update that finds it there, without the 16-tick wobble

#### Scenario: Pushed onto solid ground
- **WHEN** a bag slides onto a cell corner above untouched earth
- **THEN** it comes to rest

### Requirement: Pushing against a falling bag
A horizontal push against a falling bag SHALL NOT move it and SHALL NOT block the pusher. The bag is redrawn in place and crushes, under the normal crushing rule, the diggers and monsters it then overlaps. (research 02 §5.5, §9 item 10)

#### Scenario: Walking into a falling bag
- **WHEN** a non-invincible digger moves sideways into a bag that is falling and the digger's top is at or below the bag's top
- **THEN** the bag does not move sideways and the digger is killed by the bag

### Requirement: Crushing
On every fall step, after the bag is drawn at its new position, each overlapped digger whose top is at or below the bag's top SHALL be killed by the bag, and each overlapped monster whose top is at or below the bag's top SHALL be squashed. Invincible diggers are immune. A crushed digger scores nothing; a squashed monster scores when the bag has stopped, as defined in `monsters` and `scoring`. (research 02 §5.5, §5.7)

#### Scenario: Bag falls on a digger
- **WHEN** a falling bag's step brings it to overlap a living digger below it
- **THEN** the digger's death by bag begins on that tick

#### Scenario: Digger above the bag
- **WHEN** a falling bag overlaps a digger whose top is above the bag's top
- **THEN** the digger is not harmed by that step

### Requirement: Moving-bag count for the wind-down
A bag SHALL count as "still moving" for the purpose of ending a round while it exists and is either wobbling (including the fall that follows a wobble) or gold younger than 10 ticks. A bag that dropped at the end of a slide without wobbling does not count. (research 02 §5.8)

#### Scenario: Wobbling bag delays the round end
- **WHEN** the digger has died and a bag is still wobbling
- **THEN** the wind-down continues at least until that bag has landed and, if it broke, its gold is 10 ticks old

### Requirement: Bag sound cues
The fall cue SHALL be emitted when a bag starts falling, the wobble cue on each wobble redraw, the break cue with the first gold frame, and the gold cue when a digger collects gold. After all bags have been updated on a tick, the fall cue SHALL be stopped unless a bag is falling and the wobble cue stopped unless a bag is wobbling; both SHALL be stopped at the end of a round. (research 02 §5.2, §5.3, §5.6, research 04 §2)

#### Scenario: Bag lands
- **WHEN** the only falling bag lands
- **THEN** the fall cue is stopped during that tick's bag update
