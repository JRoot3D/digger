# Spec Delta

## Purpose

Proves that the game behaves like Digger Remastered by replaying recordings
and comparing the outcome. It defines the state hash that eDRF checkpoints
contain, when checkpoints are taken and compared, how a divergence is
reported, the headless verify run with its result line and exit statuses, and
the conformance suite over upstream's 50 recordings.

## ADDED Requirements

### Requirement: State hash value
The state hash SHALL be computed by feeding a defined byte stream of game
state to MD5 and taking the first four bytes of the digest as one unsigned
32-bit number, first digest byte most significant. In text it SHALL be
written as exactly 8 upper-case hexadecimal digits, so that it equals the
first 8 characters of the digest's usual hex form. (research 01 §8;
research 05 §5.4)

#### Scenario: Digest prefix
- **WHEN** the MD5 digest of the state stream begins with the bytes 0x9E 0x10 0x7D 0x9D
- **THEN** the state hash is 0x9E107D9D and is written as `9E107D9D`

#### Scenario: Leading zeros are kept
- **WHEN** the digest begins with 0x00 0x0A 0xBC 0x01
- **THEN** the hash is written as `000ABC01`

### Requirement: State stream value encoding
Every value in the state stream SHALL be written at a fixed width in
little-endian byte order with no padding and no separators: 16-bit values as
2 bytes, 32-bit values as 4 bytes, 8-bit values as 1 byte, and yes/no values
as one byte that is 0 or 1. Negative 16-bit values are written in two's
complement. Directions SHALL be encoded as right 0, up 2, left 4, down 6 and
"no direction" −1 (bytes FF FF). (research 01 §8, §9; research 05 §1.6, §5.4)

#### Scenario: Sixteen-bit value
- **WHEN** a 16-bit value of 300 is written
- **THEN** the bytes are 2C 01

#### Scenario: Negative value
- **WHEN** the bonus-start countdown is −2
- **THEN** it is written as FE FF

#### Scenario: Yes/no value
- **WHEN** "bonus mode active" is yes and "digger dead" is no
- **THEN** they are written as the single bytes 01 and 00

#### Scenario: No direction
- **WHEN** a monster's movement direction is "none"
- **THEN** it is written as FF FF

### Requirement: State stream order
The state stream SHALL consist of these parts in this order: the playfield
block, the digger block, the monster block, the bag block, the random-number
state, both total scores, and, only in gauntlet mode, the gauntlet clock and
time-out flag. The round-end form of the hash SHALL append the tick count
after all of them. Only the live state of the player currently playing is
covered, never the saved state of the other alternating player.
(research 01 §8; research 05 §5.4)

#### Scenario: Top-level layout
- **WHEN** the state stream is produced
- **THEN** its parts are

| # | Part | Width | Bytes |
|---|---|---|---|
| 1 | Playfield: tunnel bit set of each of the 150 cells, row by row from the top-left cell | 16-bit each | 300 |
| 2 | Digger block | see its requirement | 263 |
| 3 | Monster block | see its requirement | 163 to 211 |
| 4 | Bag block | see its requirement | 165 |
| 5 | Random-number state | signed 32-bit | 4 |
| 6 | Total score of player/digger 1, then of player/digger 2 | signed 32-bit each | 8 |
| 7 | Gauntlet only: clock (unsigned 32-bit), timed-out (yes/no) | | 5 |
| 8 | Round-end form only: tick count (unsigned 32-bit) | | 4 |

#### Scenario: Stream length without monsters
- **WHEN** a one-player non-gauntlet game is hashed while no monster is in play
- **THEN** the stream is exactly 903 bytes long

#### Scenario: Gauntlet adds five bytes
- **WHEN** the same state is hashed in a gauntlet game
- **THEN** the stream is 908 bytes long and ends with the 4 clock bytes and the timed-out byte

#### Scenario: Second score is always present
- **WHEN** a one-player game is hashed
- **THEN** part 6 still holds two scores, the second being player 2's total (0 if never changed)

#### Scenario: Wrapped gauntlet clock
- **WHEN** the gauntlet clock has wrapped below zero on the time-out tick
- **THEN** the wrapped unsigned 32-bit value is what is written

### Requirement: Digger block layout
The digger block SHALL hold the shared bonus and emerald state followed by
the state of both digger slots, slot 1 then slot 2, whether or not the game
uses the second digger. Values that the game keeps but never changes (the
"fire pressed" and per-digger "level done" flags) SHALL still be written.
(research 01 §8; research 02 §3.1)

#### Scenario: Shared part
- **WHEN** the digger block is produced
- **THEN** it starts with these 159 bytes

| Value | Width |
|---|---|
| Bonus-start countdown | 16-bit |
| Bonus time left | 16-bit |
| Emerald mask of the current player (1 or 2) | 16-bit |
| Bonus (cherry) visible | yes/no |
| Bonus mode active | yes/no |
| Diggers visible | yes/no |
| Emerald map: one byte per cell for 150 cells, row by row | 8-bit each |

#### Scenario: Per-digger part
- **WHEN** the digger block continues
- **THEN** for each of the two slots it holds these 52 bytes

| Value | Width |
|---|---|
| Cell column, cell row, x remainder, y remainder | 4 × 16-bit |
| Direction moved this tick | 16-bit |
| Bag-push pause, fireball recharge countdown | 2 × 16-bit |
| Death stage, index of the killing bag, death animation counter, death timer | 4 × 16-bit |
| Emerald-streak window countdown, emeralds in the streak | 2 × 16-bit |
| Monster-eating score multiplier, lives, invincibility ticks | 3 × 16-bit |
| Not firing, fire pressed, dead, level done, invincible | 5 × yes/no |
| Digger sprite x, y, facing direction | 3 × 16-bit |
| Digger alive (controllable) | yes/no |
| Fireball x, y, direction, explosion stage | 4 × 16-bit |

#### Scenario: Unused second digger
- **WHEN** a one-digger game is hashed
- **THEN** the second slot's 52 bytes are still present, holding that slot's current (initial) values

#### Scenario: Values that are not hashed
- **WHEN** only the death-music progress, the dirge timer or the "fireball available" flag differ between two states
- **THEN** the two states have the same hash

### Requirement: Monster block layout
The monster block SHALL hold the spawn-schedule values followed by all six
monster slots in order. Each slot contributes 25 bytes always, and a further
8 bytes only while the slot holds a monster body, that is from the tick a
monster is spawned into the slot until the monster reset at the start of the
next round; a monster that was killed or removed keeps its body until then.
(research 01 §8; research 02 §2, §4.1, §4.2, §4.7)

#### Scenario: Schedule part
- **WHEN** the monster block is produced
- **THEN** it starts with these 13 bytes

| Value | Width |
|---|---|
| Next monster index, total monsters for the level, maximum on screen | 3 × 16-bit |
| Time to next spawn, gap between spawns, digger being chased | 3 × 16-bit |
| "Bonus mode just ended" flag | yes/no |

#### Scenario: Per-slot part
- **WHEN** the monster block continues
- **THEN** each of the six slots holds

| Value | Width | Present |
|---|---|---|
| Cell column, cell row, x remainder, y remainder | 4 × 16-bit | always |
| Movement direction, delay before next move | 2 × 16-bit | always |
| Nobbin crossing count / time as hobbin | 16-bit | always |
| Death stage, squashing bag index, death delay, start delay, chased digger | 5 × 16-bit | always |
| Slot in use | yes/no | always |
| Sprite x, y, facing direction | 3 × 16-bit | only with a monster body |
| Alive, is a nobbin | 2 × yes/no | only with a monster body |

#### Scenario: Stream length with three monsters
- **WHEN** three slots hold a monster body and three do not
- **THEN** the monster block is 13 + 6 × 25 + 3 × 8 = 187 bytes long

#### Scenario: Slot whose monster was removed
- **WHEN** a monster has been killed and removed from play and the round is still running
- **THEN** its slot contributes 33 bytes: the slot's last cell, direction and timer values with "slot in use" as 00, then its last sprite position and facing, "alive" as 00 and its last nobbin value

#### Scenario: Slot never used this round
- **WHEN** a round has started and no monster has yet been spawned into a slot
- **THEN** that slot contributes 25 bytes

### Requirement: Bag block layout
The bag block SHALL hold the shared push counter and gold timer followed by
all seven bag slots in order, each at a fixed 23 bytes whether or not the bag
exists. Only the bags of the player currently playing are covered.
(research 01 §8; research 02 §5.1)

#### Scenario: Layout
- **WHEN** the bag block is produced
- **THEN** it holds these 165 bytes

| Value | Width |
|---|---|
| Push counter, gold lifetime for the level | 2 × 16-bit |
| Then for each of 7 bags: pixel x, pixel y, cell column, cell row, x remainder, y remainder | 6 × 16-bit |
| Direction, wobble countdown, gold timer, cells fallen | 4 × 16-bit |
| Wobbling, not yet fallen, exists | 3 × yes/no |

#### Scenario: Removed bag
- **WHEN** a bag has been collected and no longer exists
- **THEN** its 23 bytes are still written, with "exists" as 00 and its other values as last left

### Requirement: The hash depends only on simulation state
The state hash SHALL be identical on every supported platform and build
mode, including big-endian and 32-bit machines, for the same game, seeds and
controls. It SHALL NOT depend on the picture shown, the palette, sound,
elapsed real time, pause, or frames that are not ticks.
(research 01 §8; research 05 §4.6, §5.4)

#### Scenario: Paced and unpaced runs agree
- **WHEN** the same recording is replayed once at normal speed and once unpaced
- **THEN** every checkpoint hash is the same in both runs

#### Scenario: Pause does not change the hash
- **WHEN** a game is paused for any number of frames between tick 300 and tick 301
- **THEN** the hash at tick 512 equals that of the same game without the pause

### Requirement: Checkpoint cadence
While a game is recorded as eDRF the system SHALL emit a `C` checkpoint with
the state hash each time the tick count, after being incremented, is a
multiple of 256; an `E` checkpoint with the round-end form of the hash at the
end of every round, after the round's clean-up; and a `Z` record with the
decimal tick total and the plain state hash when the game ends. Ticks count
from 0 at game start, in the play loop and the round wind-down only.
(research 05 §4.6, §5.2, §5.3; research 01 §8)

#### Scenario: Regular checkpoints
- **WHEN** a round runs from tick 0 to tick 1000
- **THEN** `C` records are emitted after ticks 256, 512 and 768 and nowhere else in that round

#### Scenario: Round end on a multiple of 256
- **WHEN** a round ends with the tick count at exactly 1024
- **THEN** a `C` record for tick 1024 is emitted, followed by the round's `E` record

#### Scenario: Round-end hash includes the tick
- **WHEN** two rounds end in identical game state at ticks 1378 and 1379
- **THEN** their `E` hashes differ

#### Scenario: Round-end hash is taken after clean-up
- **WHEN** a round ends with monsters on screen and a broken bag's gold lying
- **THEN** the `E` hash covers the state after monsters are removed, bags are tidied and the playfield is saved

#### Scenario: Game end
- **WHEN** a game ends after 4489 ticks
- **THEN** the last record is `Z 4489 <hash>` and it follows the last round's `E` record (and the `Q` record if the game was quit)

#### Scenario: Non-tick frames are not counted
- **WHEN** 45 frames of "PLAYER 1" flashing pass before play starts
- **THEN** the tick count is still 0

### Requirement: Checkpoint verification during eDRF playback
During eDRF playback the system SHALL compute the same hashes at the same
points and compare each with the next recorded checkpoint of the same kind.
The first mismatch SHALL mark the playback as failed and end the comparison.
Recorded checkpoints that the game never reaches, or a point the game reaches
with no checkpoint recorded for it, SHALL NOT by themselves count as a
mismatch. (research 05 §5.3, §5.5)

#### Scenario: All checkpoints match
- **WHEN** an upstream eDRF is replayed and every `C`, `E` and `Z` value equals the computed one
- **THEN** the playback is classified as verified

#### Scenario: Mismatch at a tick checkpoint
- **WHEN** the hash computed after tick 768 is `0A1B2C3D` and the recording has `C 99887766` there
- **THEN** the playback is classified as failed at the checkpoint of tick 768

#### Scenario: End mismatch
- **WHEN** the recording ends with `Z 4489 11223344` and the game ends after 4490 ticks, or after 4489 ticks with another hash
- **THEN** the playback is classified as failed at the end record

#### Scenario: Truncated recording is checked up to the cut
- **WHEN** an eDRF is cut after its checkpoint for tick 512
- **THEN** the checkpoints for ticks 256 and 512 are compared and no failure is raised for the missing later ones

### Requirement: Divergence report
A failed playback SHALL be reported on the error stream with the kind of the
first failing checkpoint, the tick count at which it was taken, the expected
(recorded) hash and the actual (computed) hash, followed by a line naming the
recording as failed. Because a `C` mismatch only bounds the fault to the 256
ticks before it, the report SHALL say "before tick N" and SHALL name the last
tick at which a checkpoint matched. (research 05 §5.5)

#### Scenario: Tick checkpoint message
- **WHEN** the checkpoint of tick 768 fails with computed `0A1B2C3D` and recorded `99887766`, the checkpoint of tick 512 having matched
- **THEN** the report contains `eDRF: the game diverges from the recording before tick 768 (tick state 0A1B2C3D, recorded 99887766)`, the note `last matching checkpoint: tick 512`, and `eDRF: playback of <file> FAILED`

#### Scenario: Round-end message
- **WHEN** a round-end checkpoint fails at tick 1378
- **THEN** the report contains `before tick 1378 (end of round state <actual>, recorded <expected>)`

#### Scenario: End message
- **WHEN** the end record fails
- **THEN** the report contains `eDRF: the game ends differently from the recording (expected <ticks>/<hash>, got <ticks>/<hash>)`

#### Scenario: Only the first failure is reported
- **WHEN** the checkpoints of ticks 768, 1024 and 1280 would all fail
- **THEN** exactly one divergence is reported, for tick 768

### Requirement: Playback outcome classification
Every playback SHALL end in exactly one outcome, decided in this order:
**failed** (a checkpoint mismatch, a level-number mismatch, a bad record, or
a DRF playback out of step); **stopped** (the game ended while the recording
still held unused controls or rounds); **truncated** (an eDRF whose data ran
out with no `Z` record); **taken over**; otherwise **verified** for eDRF and
**completed** for DRF, which has no checkpoints. (research 05 §3, §5.3,
§11 item 7)

#### Scenario: Verified eDRF
- **WHEN** an eDRF replays with all checkpoints and its `Z` record matching
- **THEN** the outcome is verified

#### Scenario: Completed DRF
- **WHEN** a DRF replays to its last `EOL` and the game ends there
- **THEN** the outcome is completed

#### Scenario: Game over too early
- **WHEN** the game loses its last life at tick 2000 while the eDRF still holds controls up to tick 4489 and no checkpoint had failed
- **THEN** the outcome is stopped

#### Scenario: Truncated
- **WHEN** an eDRF without `Z` replays with every available checkpoint matching
- **THEN** the outcome is truncated

#### Scenario: Failure takes precedence
- **WHEN** a truncated eDRF has a mismatching checkpoint before the cut
- **THEN** the outcome is failed

### Requirement: Headless verify run
The system SHALL provide a verify run that plays one recording from start to
end with no window, no audio device, no keyboard or gamepad input and no
frame pacing, and that returns the outcome, both total scores, the final
level and the tick total. It SHALL stop at the first failure. It SHALL be
available both as a programmatic entry point and as a developer command that
takes the recording's path. (research 05 §3, §6.3, §7.3)

#### Scenario: No pacing
- **WHEN** a recording of 4489 ticks (about 6 minutes of play at 12.5 ticks per second) is verified
- **THEN** the run performs no waiting and finishes in well under one second on a desktop machine

#### Scenario: No devices needed
- **WHEN** the verify command runs on a machine with no display server and no sound hardware
- **THEN** it behaves exactly as on a desktop machine

#### Scenario: Stops at first failure
- **WHEN** the checkpoint of tick 768 of a 500000-tick recording fails
- **THEN** the run ends after tick 768 with the failure report

### Requirement: Result line
On a successful verify run the system SHALL print exactly one line on
standard output: `score=<S> level=<L> ticks=<T>`, or
`score=<S> score2=<S2> level=<L> ticks=<T>` when the game had two alternating
players or two simultaneous diggers. S and S2 are the total scores of
player/digger 1 and 2, L the level number of the player playing when the game
ended, T the tick total; all decimal, single spaces, ending in one LF.
Nothing else SHALL be written to standard output. (research 05 §7.1, §7.2)

#### Scenario: One-player game
- **WHEN** a one-player recording ends with 3100 points on level 2 after 1378 ticks
- **THEN** standard output is exactly `score=3100 level=2 ticks=1378` and a line feed

#### Scenario: Two-digger game
- **WHEN** a two-digger recording ends with 18100 and 1100 points on level 4 after 4489 ticks
- **THEN** standard output is exactly `score=18100 score2=1100 level=4 ticks=4489` and a line feed

#### Scenario: Zero score
- **WHEN** a recording ends with no points after 2947 ticks (upstream's `dror0`)
- **THEN** the line starts with `score=0 level=` and ends with ` ticks=2947`

#### Scenario: No result line on failure
- **WHEN** a verify run ends as failed, stopped or truncated, or the file cannot be read
- **THEN** standard output is empty and the explanation is on the error stream

### Requirement: Verify exit statuses
The verify command SHALL exit with status 0 when the outcome is verified or
completed; 1 when the file cannot be opened, is not a recording, has a bad
header or is a rejected pre-1998 recording; 2 when the outcome is failed;
3 when it is stopped; 4 when it is truncated. No other status SHALL be used
for a run that received a readable path argument. (research 05 §3)

#### Scenario: Status table
- **WHEN** the verify command finishes
- **THEN** its exit status is

| Status | Meaning | Standard output |
|---|---|---|
| 0 | eDRF verified, or DRF completed | result line |
| 1 | cannot open, not a recording, bad header, pre-1998 recording | empty |
| 2 | failed: divergence, level mismatch, bad record, out of step | empty |
| 3 | stopped before the end of the recording | empty |
| 4 | truncated: eDRF without its `Z` record | empty |

#### Scenario: Missing file
- **WHEN** the path does not exist
- **THEN** the status is 1 and the error stream names the path

#### Scenario: Truncated upstream recording
- **WHEN** an upstream eDRF is cut at a line boundary after one of its checkpoints and verified
- **THEN** the status is 4

#### Scenario: Corrupted checkpoint
- **WHEN** one hex digit of a `C` record in an upstream eDRF is changed and the file is verified
- **THEN** the status is 2 and the report names that checkpoint's tick

#### Scenario: Bad record in the body
- **WHEN** a line `X 1` is inserted into the body of an upstream eDRF
- **THEN** the status is 2 and the error stream quotes the line

### Requirement: Configuration isolation during playback
A playback or verify run SHALL produce the same outcome, scores, level and
tick total whatever the user's saved or current settings are. In particular
unlimited lives, start level, game mode, gauntlet time, speed, key bindings,
sound settings and the high-score table SHALL NOT influence it, and it SHALL
NOT modify any of them. Playback always runs with the five-life cap in force.
(research 05 §2.3, §4.6 "Not from the file", §11 item 11; research 01 §2.1)

#### Scenario: Unlimited lives does not leak in
- **WHEN** an upstream recording in which the player earns more bonus lives than the cap allows is verified with the user's "unlimited lives" setting on
- **THEN** the result line is identical to the one obtained with the setting off

#### Scenario: Start level does not leak in
- **WHEN** a recording whose mode line has no `I` option is verified with the user's start level set to 5
- **THEN** the playback starts on level 1

#### Scenario: High scores are untouched
- **WHEN** a recording ending with 3333300 points is verified
- **THEN** the stored high-score table is byte-for-byte unchanged

### Requirement: Upstream recording corpus
The repository SHALL contain upstream's 23 `.drf` and 27 `.edrf` recordings
and the 27 expected-result files, byte-for-byte as in sobomax/digger at
commit `638125e`, together with a checksum manifest and a notice stating
their origin and their licence (GPL-2.0-or-later, the same as the
repository). They SHALL be copied by one documented developer command given
the path of an upstream checkout; building and testing SHALL NOT need that
checkout. (research 05 §7.1)

#### Scenario: Corpus contents
- **WHEN** the corpus directory is listed
- **THEN** it holds 23 `.drf` files, 27 `.edrf` files, 27 `.out` files (one per base name, shared by a DRF and its eDRF twin), a manifest and a notice

#### Scenario: Integrity check
- **WHEN** one byte of any corpus file is changed
- **THEN** the test suite fails with a checksum mismatch naming that file

#### Scenario: Import from the wrong commit
- **WHEN** the import command is pointed at an upstream checkout whose commit is not `638125e`
- **THEN** it refuses, unless explicitly told to proceed, and changes nothing

### Requirement: Conformance suite
The system SHALL include an automated suite that verifies each of the 50
corpus recordings and passes only if, for every one, the exit status is 0 and
the result line equals the content of the expected-result file of the same
base name. For the 27 eDRF recordings status 0 implies that every `C`, `E`
and `Z` checkpoint matched. Each recording SHALL be reported as its own pass
or fail. (research 05 §7.1, §7.3)

#### Scenario: Small one-player recording
- **WHEN** a corpus recording whose expected result is `score=3100 level=2 ticks=1378` is verified, as DRF and as its eDRF twin
- **THEN** both runs exit 0 and print that line

#### Scenario: Largest recording
- **WHEN** `r3333300.drf` and `r3333300.edrf` are verified
- **THEN** both print `score=3333300 level=353 ticks=565330`

#### Scenario: eDRF-only recordings
- **WHEN** `2riptest01.edrf`, `2test02.edrf`, `2test03.edrf` and `gauntlet01.edrf` are verified
- **THEN** each exits 0 with its expected line; `2riptest01` reports `ticks=1024`

#### Scenario: One failure is isolated
- **WHEN** one recording fails
- **THEN** the suite names that recording and its divergence report, and still reports the other 49 individually

### Requirement: Suite tiers and run time
The suite SHALL have a quick tier, covering the 11 base names whose DRF is at
most 15 KB (`2riptest01`, `sobodig`, `dror0`, `gauntlet01`, `2test03`,
`thevoid`, `ramon`, `muu27700`, `2test01`, `2test02`, `thevoids`), that runs
with the ordinary test command, and a full tier of all 50 recordings. The
full tier SHALL run on every change in continuous integration and SHALL
complete within 10 minutes in an optimised build. (research 05 §7.1, §7.3)

#### Scenario: Quick tier in the default test run
- **WHEN** the ordinary workspace test command is run
- **THEN** the recordings of the 11 quick base names (18 files: 7 DRF/eDRF pairs and 4 eDRF-only) are verified

#### Scenario: Full tier
- **WHEN** the full-tier command is run
- **THEN** all 50 recordings are verified, including 16 base names of 110000 to 565330 ticks each

### Requirement: Per-tick state trace for triage
The system SHALL be able to produce, for any recording and tick range, one
text line per tick giving the tick count, the full state hash, a separate
hash for each of the playfield, digger, monster and bag blocks, the
random-number state and both scores, and on request the decoded values of
every hashed field. Tracing SHALL NOT change the outcome of the run.
(research 01 §8; research 05 §10.6)

#### Scenario: Trace of a window
- **WHEN** a trace is requested for ticks 513 to 768 of a recording
- **THEN** exactly 256 lines are produced, numbered 513 to 768, each with the four block hashes

#### Scenario: Locating the block that differs
- **WHEN** two traces of the same recording differ first at tick 700 only in the monster-block hash
- **THEN** comparing the traces shows tick 700 and the monster block as the first difference

#### Scenario: Tracing is passive
- **WHEN** a recording is verified with and without tracing
- **THEN** the outcome and result line are identical
