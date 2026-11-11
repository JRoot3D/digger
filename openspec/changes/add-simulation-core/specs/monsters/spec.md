# Spec Delta

## Purpose

Monsters are the Digger's opponents: nobbins, which follow tunnels, and hobbins, which dig their own. This capability defines when they appear, how fast they move, how they choose their way, when they change kind, how they die, and the exact pattern of random-number draws that makes a game reproducible.

## ADDED Requirements

### Requirement: Monster slots and spawn state
There SHALL be six monster slots, used lowest free slot first. A new monster SHALL be a nobbin in the top-right cell at pixel (292,18), heading left, with no delay, a transformation counter of 0 and a spawn standstill of 5: for its first five move decisions it is drawn but its position does not change. A slot stays occupied until its monster is removed; a dead monster still being carried by a bag occupies its slot. (research 02 §4.1, §4.2)

#### Scenario: New monster stands still
- **WHEN** a monster spawns and is never delayed
- **THEN** it remains at (292,18) for its first five move decisions and first changes position on the sixth

### Requirement: Level parameters
At the start of every round the monster parameters SHALL be set from the difficulty index L: spawn gap 45 − 2L ticks, monsters per round L + 5, and at most 3 monsters on screen at L = 1, 4 at L = 2..7 and 5 at L ≥ 8. The spawn timer SHALL start at 10 and the count of monsters spawned at 0. (research 02 §4.2, §7)

#### Scenario: Parameter table
- **WHEN** the difficulty index is 1, 2, 3, 4, 5, 6, 7, 8, 9 or 10
- **THEN** the spawn gap is 43, 41, 39, 37, 35, 33, 31, 29, 27 or 25; the monsters per round are 6, 7, 8, 9, 10, 11, 12, 13, 14 or 15; and the on-screen limit is 3, 4, 4, 4, 4, 4, 4, 5, 5 or 5

#### Scenario: Schedule restarts after a death
- **WHEN** a new round begins on the same level after a death
- **THEN** all slots are empty, the spawned count is 0 and the full number of monsters for the level is due again

### Requirement: Spawn schedule
At the start of each tick's monster update the spawn timer SHALL count down if above 0. Otherwise a monster SHALL spawn if fewer than the round's total have spawned, fewer slots are occupied than the on-screen limit, a digger is alive and bonus mode is off; spawning resets the timer to the spawn gap. So the first monster appears on the 11th tick of a round and later ones at least gap + 1 ticks apart. (research 02 §4.2)

#### Scenario: First two monsters on level 1
- **WHEN** a level-1 round runs with a living digger and no bonus mode
- **THEN** monsters spawn on tick 11 and tick 55

#### Scenario: On-screen limit reached
- **WHEN** three monsters occupy slots on level 1 and the spawn timer is 0
- **THEN** no monster spawns until a slot is freed, and one spawns on the first tick after that

#### Scenario: No spawning in bonus mode
- **WHEN** the spawn timer is 0 during bonus mode
- **THEN** no monster spawns

### Requirement: Monsters remaining
The number of monsters remaining in a round SHALL be the occupied slots plus the round's total minus the number spawned. When a monster is removed during bonus mode, by any cause, the round's total SHALL increase by one, so that monster returns after bonus mode. (research 02 §3.8, §4.2)

#### Scenario: Eaten monster comes back
- **WHEN** a monster is eaten in bonus mode on level 1 after all 6 have spawned
- **THEN** the round's total becomes 7 and a further monster spawns after bonus mode ends

### Requirement: Movement step and move scheduling
On each tick, slots SHALL be processed in order 0..5. A living monster with a delay above 0 only decrements its delay; with a delay of 0 it makes one move: 4 pixels horizontally or 3 pixels vertically, the same speed as a digger. A monster SHALL NOT move right at x = 292, left at x = 12, down at y = 180 or up at y = 18; its direction becomes "none" there. (research 02 §4.3, §4.4)

#### Scenario: Undelayed nobbin in a corridor
- **WHEN** a nobbin past its standstill moves left along an open corridor with no delays
- **THEN** it advances 4 pixels on every tick

#### Scenario: Delayed monster
- **WHEN** a monster has a delay of 2
- **THEN** it does not move for two ticks and moves on the third

### Requirement: Random double move for nobbins
After the move of every living monster whose delay was 0, one random number SHALL be drawn with modulus 15 − L. If the result is 0 and the monster is a nobbin and at least one digger is alive, the monster SHALL make a second move in the same tick. Hobbins never move twice. (research 02 §4.3, §7)

#### Scenario: Double move
- **WHEN** a nobbin moves on a tick and the draw that follows returns 0
- **THEN** the nobbin makes a second complete move on that tick

#### Scenario: Hobbin still draws
- **WHEN** a hobbin moves on a tick
- **THEN** one random number with modulus 15 − L is drawn and the hobbin does not move again whatever the result

### Requirement: Sources of delay
A monster's delay SHALL increase by 1 whenever a decision changes its direction, by 1 when its move overlaps another monster, by 1 when its move touches a bag and by 1 more if it was moving horizontally, and SHALL be set to 0 when it collects gold. The penalty counter adds further delay as defined in the `scoring` capability. (research 02 §4.3, §4.4)

#### Scenario: Turning costs a tick
- **WHEN** a nobbin at a cell corner chooses a direction different from its current one
- **THEN** its delay is 1 after the move and it skips the next tick

#### Scenario: Pushing a bag
- **WHEN** a nobbin moving left touches a bag
- **THEN** its delay rises by 2

### Requirement: Chase target
Each monster SHALL chase one digger. With two simultaneous diggers the target alternates with each spawn, starting with digger 1; whenever its target is not alive a monster chases the other digger instead. In one-digger games the target is the current player's digger. (research 02 §3.10, §4.2, §4.4)

#### Scenario: Alternating targets
- **WHEN** three monsters spawn in a two-digger round
- **THEN** they target digger 1, digger 2 and digger 1

#### Scenario: Target dead
- **WHEN** a monster's target digger is dying and the other digger is alive
- **THEN** the monster's decisions use the other digger's position

### Requirement: Direction priorities at cell corners
A monster SHALL decide only when its previous move ended exactly on a cell corner. With dx, dy the target's offset: if |dy| > |dx| the priorities are vertical toward, horizontal toward, horizontal away, vertical away; otherwise horizontal toward, vertical toward, vertical away, horizontal away. "Toward" on equal coordinates means down and right. In bonus mode the list is reversed. Then the reverse of the current direction SHALL be moved to last place. (research 02 §4.4)

#### Scenario: Target mostly below
- **WHEN** the target is 40 pixels right of and 54 pixels below a monster heading right
- **THEN** the priorities are down, right, left, up by position and down, right, up, left after the no-reverse rule

#### Scenario: No reversing by preference
- **WHEN** the target is directly behind a monster heading left along a corridor
- **THEN** right, although first by position, is tried last

#### Scenario: Fleeing
- **WHEN** the same position is evaluated in bonus mode
- **THEN** the priorities before the no-reverse rule are the normal list in reverse order

### Requirement: Random element in the choice
At every decision one random number SHALL be drawn with modulus L + 5, at every level. If the result is 1 and L < 6, the first and third priorities SHALL be exchanged. For L ≥ 6 the number is drawn and discarded. (research 02 §4.4, §7)

#### Scenario: Level 3 noise
- **WHEN** a decision at L = 3 draws 1 with priorities down, right, left, up
- **THEN** the priorities become left, right, down, up

#### Scenario: Level 8 draw without effect
- **WHEN** a decision at L = 8 draws 1
- **THEN** the priorities are unchanged and exactly one number has been consumed

### Requirement: Nobbin passability and choice
A nobbin SHALL take the first priority whose neighbouring cell exists, is not untouched, and is connected: the neighbour's earth slice nearest the nobbin is gone or the nobbin's own cell's slice nearest the neighbour is gone. If no priority qualifies, its direction becomes the reverse of its current direction. (research 02 §0, §4.4)

#### Scenario: Open corridor ahead
- **WHEN** a nobbin's first priority leads into a cell joined to its own by a tunnel
- **THEN** it takes that direction

#### Scenario: Dead end
- **WHEN** a nobbin heading left reaches the corner of a dead-end cell
- **THEN** its direction becomes right

#### Scenario: Adjacent but unconnected tunnel
- **WHEN** the neighbouring cell has a vertical tunnel only and both cells still have their facing horizontal slices
- **THEN** that direction does not qualify

### Requirement: Hobbin movement
A hobbin SHALL always take its first priority regardless of earth. On every move it SHALL dig the slice ahead of it and draw the carving shape, and after moving it SHALL destroy an emerald it reaches under the same position rule as a digger, with no score. A hobbin whose first priority points off the playfield stays where it is. Hobbins face left or right according to their last horizontal move, initially left. (research 02 §4.4, §4.5)

#### Scenario: Digging through earth
- **WHEN** a hobbin's first priority is down through untouched earth
- **THEN** it moves down 3 pixels per move, removing vertical slices as a digger would

#### Scenario: Hobbin eats an emerald
- **WHEN** a hobbin's move satisfies the emerald-eating position rule for a cell holding an emerald
- **THEN** the emerald is removed, no points are scored and the remaining emerald count drops by one

### Requirement: Side effects persist while a monster is held in place
During the spawn standstill, and whenever no digger is alive, a monster's move SHALL be undone so that its position does not change, but everything else its move does SHALL still happen: the decision and its random draw, hobbin digging and emerald destruction, delay changes and collisions evaluated at the attempted position. (research 02 §4.4)

#### Scenario: Monsters freeze when the digger dies
- **WHEN** the only digger is in its death sequence
- **THEN** monsters stay where they are while random numbers continue to be drawn on each of their moves

### Requirement: Nobbin to hobbin
A nobbin's counter SHALL rise by 1 on each move in which it overlaps another monster while a digger is alive, and by 1 on each move blocked by a bag. On every tick, any nobbin (including a dead one still being carried by a bag) whose counter exceeds 10 − L SHALL become a hobbin with its counter reset to 0. (research 02 §4.3, §4.5, §9 item 24)

#### Scenario: Thresholds
- **WHEN** the difficulty index is 1, 5 or 10
- **THEN** a nobbin becomes a hobbin once its counter reaches 10, 6 or 1

#### Scenario: Two nobbins crossing
- **WHEN** two nobbins on level 9 overlap during two of one nobbin's moves
- **THEN** that nobbin is a hobbin on the following tick

### Requirement: Hobbin to nobbin
A hobbin's counter SHALL rise by 1 on each of its moves while below 100. At a decision, a hobbin whose counter exceeds 30 + 2L SHALL turn back into a nobbin with its counter reset to 0 before choosing its direction. (research 02 §4.4, §4.5, §7)

#### Scenario: Thresholds
- **WHEN** the difficulty index is 1, 5 or 10
- **THEN** a hobbin reverts at the first cell corner reached with a counter above 32, 40 or 50

### Requirement: Monsters meeting monsters
When a monster's move leaves it overlapping other monsters, then for each overlapped monster that is heading in the same direction as the mover, with neither in its spawn standstill, the overlapped monster's direction SHALL be reversed, even between cells. (research 02 §4.4)

#### Scenario: Catching up from behind
- **WHEN** a nobbin moving left overlaps another nobbin that is also moving left, both past their standstill
- **THEN** the other nobbin's direction becomes right and the mover's delay rises by 1

#### Scenario: Head-on
- **WHEN** two nobbins moving in opposite directions overlap
- **THEN** neither direction is changed by the collision

### Requirement: Monsters meeting bags
A monster moving horizontally into a bag SHALL push it under the same rules as a digger. A monster moving vertically, or not moving, is blocked by any unbroken bag it touches. A blocked monster SHALL be returned to its previous position; a blocked nobbin moving vertically SHALL reverse direction. Gold touched by a monster with no living digger on it is collected without score. A hobbin whose counter exceeds 1 SHALL remove every bag and gold pile it touches. (research 02 §4.4, §4.6, §5.6)

#### Scenario: Nobbin pushes a bag
- **WHEN** a nobbin moving right touches a free bag
- **THEN** the bag moves 4 pixels right and the nobbin keeps its new position

#### Scenario: Hobbin eats a bag
- **WHEN** a hobbin on its second or a later move as a hobbin touches a bag
- **THEN** the bag disappears

#### Scenario: Nobbin under a bag
- **WHEN** a nobbin moving up touches a resting bag
- **THEN** it returns to its previous position, heads down, and its counter rises by 1

### Requirement: Monsters killing diggers
When a monster's move leaves it overlapping one or more diggers outside bonus mode, every overlapped living digger SHALL be killed with a death by monster; invincible diggers are unaffected and tombstones are ignored. The overlap is evaluated at the position the monster attempted, even if the move was then undone. (research 02 §4.4, §3.7)

#### Scenario: Catching the digger
- **WHEN** a nobbin's move overlaps a living, non-invincible digger outside bonus mode
- **THEN** the digger's death sequence begins and the monster is unharmed

### Requirement: Being eaten in bonus mode
In bonus mode, a monster whose own move leaves it overlapping one or more living diggers SHALL be removed once, each of those diggers SHALL receive the next step of its eating progression, and the eat cue SHALL be emitted once. (research 02 §4.4, §3.8, §3.10)

#### Scenario: Monster runs into a digger
- **WHEN** a monster's move overlaps a living digger during bonus mode
- **THEN** the monster is removed and that digger scores its next eating step

### Requirement: Death under a falling bag
A monster overlapped by a falling bag whose top is at or above the monster's top SHALL die: it shows its dead image, keeps its slot, and on each tick is set level with the bag's top if its own top is less than 6 pixels below the bag's top. On the tick the bag is found stopped or gone a 1-tick delay starts; after it the monster is removed and the kill is scored. A dead left-facing hobbin is 14 pixels tall instead of 15 for collision purposes. (research 02 §4.7, §9 items 6, 7)

#### Scenario: Squashed nobbin
- **WHEN** a falling bag overlaps a nobbin below it and comes to rest, being found stopped on tick A
- **THEN** the nobbin is removed and 250 points are scored on tick A+2

#### Scenario: Dead monster still counts
- **WHEN** a dead monster is being carried by a bag on level 1 with two other monsters on screen
- **THEN** no further monster spawns until it is removed

### Requirement: Frightened by falling bags
On every tick a bag is falling, each monster in the bag's column that is heading up SHALL have its direction changed to down. (research 02 §4.6, §5.3)

#### Scenario: Nobbin climbing under a falling bag
- **WHEN** a nobbin heading up is in column 4 while a bag falls in column 4
- **THEN** the nobbin's direction becomes down on that tick

### Requirement: Random-number draw pattern
Random numbers SHALL be drawn only by monsters, and only as follows, slot by slot in ascending order, for each living monster whose delay is 0: one draw with modulus L + 5 inside its move if that move makes a decision; then one draw with modulus 15 − L; then, if a second move follows, one more draw with modulus L + 5 if that move makes a decision. Waiting, dead and empty slots draw nothing. The same applies during the wind-down. (research 02 §8, research 01 §7.1)

#### Scenario: One undelayed monster on a cell corner
- **WHEN** a single living nobbin with delay 0 stands on a cell corner and the second draw is not 0
- **THEN** exactly two numbers are drawn on that tick, the first with modulus L + 5 and the second with modulus 15 − L

#### Scenario: Monster between cells
- **WHEN** a single living monster with delay 0 is between cell corners
- **THEN** exactly one number is drawn on that tick, with modulus 15 − L

#### Scenario: Nothing else draws
- **WHEN** a tick passes with no living monster at delay 0
- **THEN** the random-number state is unchanged, whatever diggers, fireballs and bags did
