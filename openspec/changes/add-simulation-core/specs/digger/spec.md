# Spec Delta

## Purpose

The Digger is the character the player steers. This capability defines how control input becomes movement and digging, how emeralds are eaten, how fireballs are fired and stopped, how a digger dies and returns, how bonus mode works, and what differs when two diggers play at once.

## ADDED Requirements

### Requirement: Start position and facing
At the start of every round each digger with lives left SHALL be placed on row 9 (y = 180) with no pending push pause, a ready fireball, no emerald run and an eat multiplier of 1. In a one-digger game it starts in column 7 (x = 152); player 1 faces right and player 2 of an alternating game faces left. With two simultaneous diggers, digger 1 starts in column 8 (x = 172) facing right and digger 2 in column 6 (x = 132) facing left. (research 02 §3.2)

#### Scenario: One-player start
- **WHEN** a one-player round starts
- **THEN** the digger is at pixel (152,180) facing right

#### Scenario: Two simultaneous diggers
- **WHEN** a two-digger round starts
- **THEN** digger 1 is at (172,180) facing right and digger 2 is at (132,180) facing left

#### Scenario: Second alternating player
- **WHEN** player 2's round starts in an alternating two-player game
- **THEN** the digger is at (152,180) facing left

### Requirement: Direction resolution from held controls
Once per tick, for each digger's control slot, the four direction controls SHALL be resolved to one direction or none. The most recently pressed direction wins; presses that begin on the same tick are ranked up, down, left, right with the last winning. When the winning control is released, a still-held control takes over with priority right, left, down, up. A control pressed and released between ticks SHALL still count as held for exactly one tick. (research 05 §1.6)

#### Scenario: Last pressed wins
- **WHEN** right is held and then up is also pressed
- **THEN** the resolved direction on that tick is up

#### Scenario: Fallback after release
- **WHEN** up, down and left are held, left was pressed last, and left is released
- **THEN** the resolved direction becomes down, the higher-priority of the two controls still held

#### Scenario: Short tap
- **WHEN** a direction is reported pressed on one tick's input only
- **THEN** the resolved direction is that direction for that tick and none on the next

#### Scenario: Releasing a non-winning control
- **WHEN** right is held, up is pressed afterwards and held, and right is released
- **THEN** the resolved direction stays up

### Requirement: Controls are reset at round start
At the start of every round all remembered control state SHALL be cleared, including the fire state, so a control held across the round boundary counts as newly pressed on the first tick. In an alternating two-player game both players SHALL use control slot 1; with two simultaneous diggers, digger 2 SHALL use control slot 2. (research 05 §1.6, §1.7)

#### Scenario: Held key across a death
- **WHEN** left is held continuously through a death and the next round starts
- **THEN** on the first tick of the new round the resolved direction is left

### Requirement: Movement samples are taken only when a digger can move
A digger SHALL consume one movement sample (resolved direction plus that tick's fire control state) on each tick in which it is alive, visible and not in a push pause, and on no other tick. With two diggers the samples of one tick are taken in digger order. No sample SHALL be consumed while a digger is dying, during a push pause or during the wind-down at the end of a round. This sampling pattern is part of the determinism contract. (research 02 §3.4, research 05 §1.6, §4.3)

#### Scenario: Push pause consumes no sample
- **WHEN** a digger pushes a bag on tick N
- **THEN** it consumes a movement sample on tick N, none on tick N+1, and one again on tick N+2

#### Scenario: Dying digger
- **WHEN** a digger has been killed and its death sequence is running
- **THEN** no movement sample is consumed for it on any tick of the sequence

### Requirement: Turning rules
A digger SHALL adopt an up or down input as its facing only when its horizontal remainder is 0, and a left or right input only when its vertical remainder is 0. With any direction input it moves along its facing that tick; with no input it does not move, even between cells. Hence a perpendicular input given between cells keeps the digger moving along its current facing until it is aligned, and reversal along the current axis is immediate. (research 02 §3.4)

#### Scenario: Turn requested between cells
- **WHEN** a digger facing right at (20,18) receives up on consecutive ticks
- **THEN** it moves right to (24,18), (28,18), (32,18), then turns and moves up on the tick after reaching (32,18)

#### Scenario: No input
- **WHEN** a digger at (20,18) receives no direction on a tick
- **THEN** it stays at (20,18) and keeps its facing

#### Scenario: Immediate reversal
- **WHEN** a digger facing right at (20,18) receives left
- **THEN** it moves to (16,18) facing left on that tick

### Requirement: Step size and playfield limits
A moving digger SHALL advance 4 pixels per tick horizontally or 3 pixels per tick vertically, so a cell takes 5 ticks to cross horizontally and 6 vertically. A digger SHALL NOT move right at x = 292, left at x = 12, down at y = 180 or up at y = 18; it keeps its facing and stays in place. (research 02 §0, §3.4)

#### Scenario: Crossing one cell downward
- **WHEN** a digger at (12,18) holds down for 6 ticks
- **THEN** it is at (12,36)

#### Scenario: At the left limit
- **WHEN** a digger at (12,180) receives left
- **THEN** it stays at (12,180) facing left and digs nothing

### Requirement: Digging while moving
On every tick a digger actually moves, it SHALL remove the earth slice ahead of its previous position, draw the tunnel carving shape for its direction, and then move. Movement is never slowed by earth: a digger moves at the same speed through earth and through tunnels. (research 02 §3.4)

#### Scenario: Tunnelling through solid earth
- **WHEN** a digger at a cell corner moves right for 5 ticks through an untouched cell
- **THEN** it arrives at the next cell corner and the cell behind it has no horizontal slices

### Requirement: Eating emeralds
After each move the cell being entered SHALL be tested: for right and down moves with a non-zero remainder, the next cell right or below; otherwise the digger's own cell. If it holds an emerald, the emerald SHALL be redrawn when the remainder along the direction of travel is 8 (right), 12 (up), 16 (left) or 6 (down), and eaten when it is 12 (right), 9 (up), 12 (left) or 9 (down). Eating scores 25 points and emits the emerald click and emerald note cues. (research 02 §3.5)

#### Scenario: Eating while moving right
- **WHEN** a digger moves right from (52,36) toward an emerald in cell (3,1)
- **THEN** the emerald is eaten on the move that brings the digger to x = 64, and the remaining emerald count drops by one

#### Scenario: Eating while moving up
- **WHEN** a digger moves up into a cell holding an emerald
- **THEN** the emerald is eaten on the move after which the digger's vertical remainder is 9

### Requirement: Eight emeralds in a row
Each digger SHALL keep a run counter and a 9-tick window that is restarted by every emerald it eats and counts down once at the end of every tick. An emerald eaten after the window has expired starts a new run at note 0. The emerald note cue SHALL carry the run position 0..7. The eighth emerald of a run SHALL score 250 points in addition to its 25 and reset the run. (research 02 §3.5)

#### Scenario: Unbroken run
- **WHEN** a digger eats eight emeralds with never more than 8 ticks between consecutive ones
- **THEN** the note cues carry positions 0,1,2,3,4,5,6,7 and the total scored is 8×25 + 250 = 450

#### Scenario: Run broken by a pause
- **WHEN** a digger eats three emeralds, then eats the next one 9 ticks after the third
- **THEN** that emerald's note cue carries position 0 and the run restarts

### Requirement: Meeting a bag while moving
If, after moving, a digger overlaps a bag (other than a falling bag while the digger is invincible): for a left or right move the bag or bags SHALL be pushed and the digger SHALL pause for the next tick whether or not the push succeeded; for any other case the move is blocked unless every touched bag is gold, which is collected. A blocked or failed move SHALL restore the digger's previous position and reverse its facing. (research 02 §3.4)

#### Scenario: Successful push
- **WHEN** a digger moving right comes to overlap a free bag on tick N
- **THEN** the bag moves 4 pixels right on tick N, the digger keeps its new position, and the digger does not move on tick N+1

#### Scenario: Bag above blocks upward movement
- **WHEN** a digger moving up comes to overlap a resting, unbroken bag
- **THEN** it is returned to its previous position and faces down

#### Scenario: Blocked horizontal push
- **WHEN** a digger moving right pushes a bag that is against the right playfield limit
- **THEN** the digger returns to its previous position, faces left, and pauses for the next tick

### Requirement: Digger left without a facing
If a digger that did not move this tick overlaps an unbroken bag, its facing SHALL become "none": it is not drawn, touches nothing, and cannot fire until a direction input gives it a facing again under the turning rules. A fire request while the facing is "none" SHALL be ignored without using up the fireball. This replaces the original's program abort with defined behaviour. (research 02 §3.4, §9 item 18)

#### Scenario: Fire with no facing
- **WHEN** a digger whose facing is "none" has fire active
- **THEN** no fireball appears, the fireball stays ready, and the game continues

#### Scenario: Facing recovered
- **WHEN** such a digger, aligned on a cell corner, receives right
- **THEN** it faces right, is drawn again and moves normally

### Requirement: Fire control is latched one tick late
The fire control state read on tick N SHALL take effect on tick N+1: the decision to fire on a tick uses the fire state latched from the previous tick. The latch SHALL be cleared at round start. (research 05 §1.7)

#### Scenario: Fire pressed on one tick
- **WHEN** fire is first reported pressed in the input of tick 20 and the fireball is ready
- **THEN** the fireball is launched on tick 21

### Requirement: Launching a fireball
When fire is active for a living digger whose fireball is ready, an 8×8 fireball SHALL be launched in the digger's facing from the digger's position (x,y): right at (x+8, y+4), up at (x+4, y), left at (x, y+4), down at (x+4, y+8). The launch SHALL happen before the digger's own move on that tick, emit the fire cue for that fireball, and perform no collision test on the launch tick. Each digger has at most one fireball. (research 02 §3.6)

#### Scenario: Firing to the right
- **WHEN** a digger at (152,180) facing right fires
- **THEN** a fireball appears at (160,184) travelling right and the digger is shown without its fire-ready look

#### Scenario: Fire held while not ready
- **WHEN** fire is active while the digger's fireball is flying, exploding or recharging
- **THEN** no new fireball is launched

### Requirement: Recharge time by level
After its fireball has been removed, a digger's recharge SHALL count down from 3 × difficulty index + 60, one per tick; the digger becomes ready on the tick the count reaches 0 and can fire from the following tick. The count does not run while the fireball is flying or exploding. (research 02 §3.6, §7)

#### Scenario: Recharge table
- **WHEN** the difficulty index is 1, 2, 3, 4, 5, 6, 7, 8, 9 or 10
- **THEN** the recharge count starts at 63, 66, 69, 72, 75, 78, 81, 84, 87 or 90 respectively

#### Scenario: Earliest second shot on level 1
- **WHEN** a fireball is removed on tick R of level 1 and fire is held throughout
- **THEN** the digger shows as ready on tick R+63 and the next fireball is launched on tick R+64

### Requirement: Fireball flight
On each tick after launch a flying fireball SHALL move 8 pixels horizontally or 7 pixels vertically, probe the picture for earth at its new position, be drawn there, and then be tested against sprites. Digger 1's fire and movement are fully processed before digger 2's. (research 02 §3.6)

#### Scenario: Flight along an open tunnel
- **WHEN** a fireball is launched to the right at (160,184) into an open tunnel with nothing in the way
- **THEN** on the following ticks it is at x = 168, 176, 184 and so on

### Requirement: Earth probe for fireballs
Earth SHALL be detected by reading non-black pixels from the game picture after the move and before the fireball is redrawn. Horizontal shots read the row y+4 across logical x..x+7; upward shots read column x+4 over rows y..y+6; downward shots read the left half of logical pixel x+3 over rows y..y+6. Any non-black pixel there (earth, emerald, text) counts; positions past the right or bottom edge count as earth; others outside the picture read as the `rendering` probe defines. (research 02 §3.6)

#### Scenario: Background counts as earth
- **WHEN** a fireball moving right is moved so that row y+4 of its new position lies on background pixels
- **THEN** the probe reports earth

#### Scenario: Probe outside the screen
- **WHEN** a probe position lies beyond the right or bottom edge of the 320×200 logical screen
- **THEN** the probe reports earth and no error occurs

### Requirement: Earth stops a fireball only when it touches no sprite
A flying fireball whose probe found earth SHALL be stopped by it only if, after being drawn, it overlaps no sprite of any kind, its own digger included. It SHALL then be drawn back at its previous position and start exploding there. If it overlaps any sprite, the probe result is ignored for that tick. (research 02 §3.6)

#### Scenario: Shot reaches earth
- **WHEN** a fireball moving right in a tunnel moves onto background pixels and touches no sprite
- **THEN** it is drawn back at its previous position and starts exploding there

#### Scenario: Probe ignored while touching a sprite
- **WHEN** a fireball's new position has earth pixels and also overlaps the tombstone of a dead digger
- **THEN** the earth probe does not stop it on that tick

### Requirement: What a fireball hits
After being drawn, a flying fireball SHALL: kill every monster it overlaps, scoring 250 points each to its owner, and explode; kill every other living, non-invincible digger it overlaps and explode; explode if it overlaps the bonus, a bag or another fireball, and make every overlapped flying fireball explode too. It SHALL then explode if it has passed x > 296 (right), x < 16 (left), y < 15 (up) or y > 183 (down). (research 02 §3.6)

#### Scenario: Shooting a monster
- **WHEN** a flying fireball overlaps a living nobbin
- **THEN** the nobbin is removed at once, the owner scores 250 and the fireball starts exploding

#### Scenario: Shooting a bag
- **WHEN** a flying fireball overlaps a resting bag
- **THEN** the fireball starts exploding and the bag is unaffected

#### Scenario: Leaving the playfield
- **WHEN** a fireball travelling up is moved to y = 11
- **THEN** it starts exploding

### Requirement: Fireball explosion
An exploding fireball SHALL show explosion frames 1, 2 and 3 on the three ticks after it starts exploding and be removed on the fourth, when the fire cue for it stops and its owner's recharge begins. The explode cue SHALL be emitted when frame 1 is drawn. While exploding it remains an 8×8 obstacle that makes other fireballs explode; a fireball already exploding that is hit again restarts at frame 1. All fireballs SHALL be removed at the end of a round. (research 02 §3.6)

#### Scenario: Explosion length
- **WHEN** a fireball starts exploding on tick E
- **THEN** explosion frames are drawn on ticks E+1, E+2 and E+3, and on tick E+4 the fireball is gone

### Requirement: Who can be killed
A kill SHALL be ignored for an invincible digger and for a digger already in its grave sequence (waiting, growing or resting tombstone). A digger being carried down by a bag or flying in the death arc can be killed again, which restarts its death in the new manner. Outside bonus mode a digger moving into a monster is not harmed by its own move; it dies when the monster's move finds them overlapping. (research 02 §3.4, §3.7, §9 items 13, 15)

#### Scenario: Digger walks into a monster
- **WHEN** a digger's move brings it to overlap a living monster outside bonus mode
- **THEN** nothing happens on the digger's move and the digger is killed during that monster's next move in which they still overlap

#### Scenario: Crushed digger hit by a monster
- **WHEN** a digger being carried down by a bag is touched by a monster's move
- **THEN** its death restarts as a death by monster

### Requirement: Death by monster or fireball
A digger killed by a monster or a fireball SHALL spend one tick undrawn, then show the dead-digger image for 7 ticks at heights 3, 5, 6, 6, 5, 3 and 0 pixels above its position, emitting the death cue on the first of these ticks and stopping the music on the last if no digger is alive. The grave sequence SHALL follow. (research 02 §3.7)

#### Scenario: Death arc
- **WHEN** a digger at y = 100 is killed by a monster on tick T
- **THEN** on ticks T+2 to T+8 the dead-digger image is drawn at y = 97, 95, 94, 94, 95, 97 and 100

### Requirement: Death by falling bag
A digger killed by a falling bag SHALL be shown with the dead-digger image and carried down with the bag, kept no higher than 6 pixels below the bag's top, for as long as the bag keeps falling. On the tick the bag has stopped or no longer exists the death cue SHALL be emitted, the digger SHALL be raised 6 pixels, and the grave sequence SHALL follow. No points are scored. (research 02 §3.7, §5.7)

#### Scenario: Carried to the floor
- **WHEN** a bag falling down a shaft crushes a digger and comes to rest with its top at y = 180
- **THEN** the digger is drawn at y = 186 while carried and its grave sequence takes place at y = 180

### Requirement: Grave sequence
The grave sequence SHALL wait 5 ticks, then draw five growing tombstone frames at ticks G, G+3, G+6, G+9 and G+12, then rest. On tick G the dirge SHALL be requested (at the standard tempo; at the quick tempo with two diggers) and every monster overlapping the new grave SHALL be removed without score. The death SHALL be resolved on tick G+73, except in a one-digger game, where it is resolved on tick G+89 to leave time for the dirge. (research 02 §3.7, §9 items 14, 23)

#### Scenario: One-digger timing
- **WHEN** the first tombstone frame of a one-digger game is drawn on tick G
- **THEN** the round's main phase ends on tick G+89

#### Scenario: Monster on the grave
- **WHEN** a monster overlaps the digger on the tick the first tombstone frame is drawn
- **THEN** the monster is removed and no points are scored

### Requirement: Lives
Every digger SHALL start a game with 3 lives. Outside gauntlet mode a death costs one life: when a round ends with all diggers dead, each digger that still has lives loses one; with two diggers, a digger whose death is resolved while its partner is alive loses its life at that moment. In gauntlet mode deaths never cost lives. A digger with no lives takes no further part in the game. (research 01 §3.2, research 02 §3.7, §3.9)

#### Scenario: One-player death
- **WHEN** the digger of a one-player game with 3 lives dies and the round ends
- **THEN** 2 lives remain and the status line shows one spare life

#### Scenario: Gauntlet death
- **WHEN** a digger dies in gauntlet mode
- **THEN** its life count is unchanged and play resumes with a new round

### Requirement: Respawn with invincibility in two-digger games
With two simultaneous diggers, a digger whose death is resolved while its partner is alive and which still has a life after losing one SHALL reappear at its start cell and facing, able to fire, and be invincible. The music in effect SHALL be restarted. (research 02 §3.7, §3.10)

#### Scenario: Respawn
- **WHEN** digger 2 dies with 3 lives while digger 1 is alive
- **THEN** on the tick its death is resolved it has 2 lives, stands at (132,180) facing left and blinks

#### Scenario: Last life lost
- **WHEN** a digger with 1 life dies while its partner is alive
- **THEN** it has 0 lives, does not respawn, and its tombstone is removed on the following tick

### Requirement: Invincibility
Invincibility SHALL last for 50 draws of the digger's sprite, and the sprite SHALL be hidden on draws where the remaining count modulo 10 is below 5. While invincible a digger cannot be killed by monsters, fireballs or bags, is not blocked by falling bags, and does not stop bags above it from wobbling. (research 02 §3.4, §3.7, §5.3)

#### Scenario: Monster touches an invincible digger
- **WHEN** a monster's move overlaps an invincible digger outside bonus mode
- **THEN** the digger is not killed

#### Scenario: Invincibility runs out faster after a blocked push
- **WHEN** an invincible digger's move is blocked by a bag
- **THEN** its sprite is drawn twice on that tick and the invincibility count drops by 2

#### Scenario: Normal duration
- **WHEN** a respawned digger moves freely without being blocked
- **THEN** it is invincible for 50 ticks

### Requirement: Bonus appearance
Once per round, the bonus (cherry) SHALL appear in the top-right cell at (292,18) on the first tick on which all of the round's monsters have been spawned, the spawn timer has run out and a digger is alive. It SHALL be removed when collected and at the end of the round. (research 02 §3.8)

#### Scenario: Cherry after the last monster
- **WHEN** the last monster of a level-1 round spawns on tick S and a digger stays alive
- **THEN** the bonus appears on tick S+44

### Requirement: Entering bonus mode
A digger whose move brings it to overlap the bonus SHALL score 1000 points, remove the bonus and start bonus mode with a duration of 250 − 20 × difficulty index ticks, resetting every digger's eat multiplier. The duration counts down from the pickup tick itself while a digger is alive; bonus mode ends on the tick after the count reaches 0, or at once when no digger is alive. Ending SHALL restore normal intensity and request the main music. (research 02 §3.8, §7)

#### Scenario: Duration table
- **WHEN** the difficulty index is 1, 2, 3, 4, 5, 6, 7, 8, 9 or 10
- **THEN** the bonus duration is 230, 210, 190, 170, 150, 130, 110, 90, 70 or 50

#### Scenario: Bonus mode length on level 1
- **WHEN** the bonus is collected on tick P of level 1 and a digger stays alive
- **THEN** bonus mode is in effect from tick P through tick P+229 and has ended after tick P+230

### Requirement: Bonus mode signalling
For the first 20 ticks of bonus mode and whenever fewer than 20 ticks remain, the palette intensity SHALL alternate every tick with the parity of the remaining count and the bonus pulse cue SHALL sound. When the first 20 ticks are over the bonus music SHALL be requested, the pulse cue stopped and the picture held at high intensity until the final flashing. (research 02 §3.8)

#### Scenario: Start of bonus mode
- **WHEN** bonus mode begins on tick P
- **THEN** the pulse cue is on from tick P, and on tick P+19 it is stopped and the bonus music is requested

### Requirement: Eating monsters by moving into them
In bonus mode, a living digger whose move brings it to overlap one or more monsters SHALL remove every one of them, emitting the eat cue and awarding the next step of the eating progression for each. (research 02 §3.4, §3.8)

#### Scenario: Two monsters eaten in one move
- **WHEN** a digger in bonus mode with a multiplier of 1 moves to overlap two monsters
- **THEN** both are removed and the digger scores 200 and then 400

### Requirement: Two-digger interaction rules
With two simultaneous diggers: both SHALL be updated every tick, digger 1 first; a fireball SHALL kill the other digger and collide with the other fireball; a round SHALL end only when both are dead; every living digger overlapping gold scores for it; and the dirge is requested at the quick tempo without the extended one-digger wait. (research 02 §3.10)

#### Scenario: Friendly fire
- **WHEN** digger 1's flying fireball overlaps living, non-invincible digger 2
- **THEN** digger 2 dies as if killed by a monster and the fireball explodes

#### Scenario: One digger down
- **WHEN** digger 1 is dead and digger 2 is alive
- **THEN** the round continues
