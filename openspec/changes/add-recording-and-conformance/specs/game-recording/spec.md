# Spec Delta

## Purpose

Lets a game of Digger be stored as, and replayed from, the two recording file
formats of Digger Remastered: the DRF text format and the extended eDRF
format ("eDRF 2"). It defines both grammars, how a recording supplies seeds,
maps and controls to a game, how a game is recorded and named, and how
unusable files are refused.

## ADDED Requirements

### Requirement: Recording kind is detected from the first line
The system SHALL decide the kind of a recording from its first line alone: a
first line that is exactly `eDRF 2` selects the eDRF format; otherwise a first
line whose first three bytes are `DRF` selects the DRF format; anything else
is refused as "not a recording". Header lines SHALL be accepted with either
LF or CR LF line ends. (research 05 §4.1, §5.2)

#### Scenario: DRF first line
- **WHEN** a file starts with the line `DRF`
- **THEN** it is read as a DRF recording

#### Scenario: eDRF first line
- **WHEN** a file starts with the line `eDRF 2`
- **THEN** it is read as an eDRF recording and no level-map block is expected in its header

#### Scenario: CRLF header
- **WHEN** a DRF file uses CR LF line ends throughout (as upstream's `muu27700.drf` does)
- **THEN** its header and body are read exactly as the same file with LF line ends

#### Scenario: Unknown first line
- **WHEN** a file starts with `eDRF 3`, with `PNG`, or is empty
- **THEN** reading fails with a "not a recording" error and no game is started

### Requirement: Common header lines
Both formats SHALL carry the same three lines after the first: a version
line (free text of the form `XX YYY YYYYMMDD`, for example
`AJ DOS 19990506` or `MS MPL 20190212`), a mode line, and a bonus-score line
holding one decimal number from 0 to 65535, the score interval at which an
extra life is awarded. A missing line or a bonus score that is not such a
number SHALL be a header error. (research 05 §4.1, §5.2; research 01 §9)

#### Scenario: Typical header
- **WHEN** lines 2 to 4 are `POK SDL 20110912`, `1` and `20000`
- **THEN** the recording is a one-player game with a bonus-life interval of 20000 points

#### Scenario: Bonus score out of range
- **WHEN** the bonus line is `70000`, `-1` or `abc`
- **THEN** reading fails with a header error naming line 4

#### Scenario: File ends inside the header
- **WHEN** a file contains only `DRF` and a version line
- **THEN** reading fails with a header error and no game is started

### Requirement: Mode line grammar
The mode line SHALL be parsed as `mode := ("1" | "2" | ["M" digit] ["G" digits]) ["U"] ["I" digits]`.
`1`/`2` give the number of alternating players, `M<d>` the number of
simultaneous diggers (1 or 2), `G<n>` gauntlet mode lasting n seconds
(1 to 3599), `I<n>` the start level (1 to 1000). `U` SHALL be accepted and
ignored. Everything not stated defaults to one player, one digger, no
gauntlet, start level 1. Any other content SHALL be a header error.
(research 05 §4.1; research 01 §2.1)

#### Scenario: Mode lines found in upstream recordings
- **WHEN** the mode line is `1`, `M2` or `G120`
- **THEN** the game is, respectively, one player with one digger; one player with two simultaneous diggers; a one-digger gauntlet game of 120 seconds

#### Scenario: Combined options
- **WHEN** the mode line is `M2G300I5`
- **THEN** the game has two simultaneous diggers, gauntlet mode of 300 seconds and starts on level 5

#### Scenario: Alternating players with a start level
- **WHEN** the mode line is `2UI3`
- **THEN** the game has two alternating players starting on level 3, and the `U` has no effect

#### Scenario: Invalid mode lines
- **WHEN** the mode line is `3`, `M3`, `G0`, `G4000`, `I0`, `I1001` or `1X`
- **THEN** reading fails with a header error naming line 3

### Requirement: Pre-1998 recordings are rejected
The system SHALL read the decimal number that starts at byte offset 7 of the
version line (0 when there is none) and SHALL refuse the recording when that
number is 19981125 or less. The error message SHALL quote the version line
and state that recordings made by versions dated 19981125 or earlier are not
supported. No game SHALL be started. (research 05 §4.1; research 02 §9 item 3)

#### Scenario: Recording from an old version
- **WHEN** the version line is `AJ DOS 19981125` or `AJ DOS 19980704`
- **THEN** reading fails with the "not supported: recorded by a version dated 19981125 or earlier" error quoting that line

#### Scenario: Oldest upstream test recording
- **WHEN** the version line is `AJ DOS 19990130`
- **THEN** the recording is accepted

#### Scenario: Version line without a date
- **WHEN** the version line is `DIGGER` or is empty
- **THEN** the recording is refused with the same error, because the date reads as 0

### Requirement: DRF header level maps
A DRF header SHALL continue with exactly 80 map lines: 8 level plans of 10
rows each, plan 1 first, top row first. Only the first 15 bytes of a line are
used. The map letters are `S` (tunnel junction), `V` (vertical tunnel), `H`
(horizontal tunnel), `B` (gold bag) and `C` (emerald); every other byte, and
every column missing from a short line, SHALL mean plain earth. Fewer than 80
map lines SHALL be a header error. (research 05 §4.1; research 01 §5.5)

#### Scenario: Full-width row
- **WHEN** the first map line is `S   B     HHHHS`
- **THEN** row 0 of plan 1 has a junction at column 0, a bag at column 4, horizontal tunnel at columns 10 to 13 and a junction at column 14

#### Scenario: Short row
- **WHEN** a map line is `V  CC` (5 characters)
- **THEN** columns 0 to 4 are read from it and columns 5 to 14 are earth

#### Scenario: Over-long row
- **WHEN** a map line has 20 characters
- **THEN** characters 16 to 20 are ignored

#### Scenario: Missing rows
- **WHEN** the file ends after 79 map lines
- **THEN** reading fails with a header error

### Requirement: DRF body grammar
The DRF body SHALL be read as one character stream in which every byte below
0x20 is discarded, so line breaks are insignificant. It consists of rounds:
optional initials, an 8-digit hexadecimal seed (either case), any number of
runs, then `EOL`. A run is one of `s r u l d` (stop, right, up, left, down
without fire) or `S R U L D` (the same with fire), optionally followed by a
decimal repeat count. `EOG` may follow the last `EOL`.
(research 05 §4.1, §4.2)

#### Scenario: Grammar
- **WHEN** a DRF body is parsed
- **THEN** it is accepted if and only if it matches

```
body   := round* ["EOG"] ["*" c c c]
round  := ["*" c c c] seed run* "EOL"
seed   := 8 hex digits            ; round RNG seed, most significant digit first
run    := dirchar [count]
dirchar:= "s"|"r"|"u"|"l"|"d"|"S"|"R"|"U"|"L"|"D"
count  := decimal digits          ; repeat count; 0 and 1 both mean once
```

#### Scenario: Run-length decoding
- **WHEN** the runs after a seed are `r12Ul3s`
- **THEN** the round's records are 12 × (right, no fire), 1 × (up, fire), 3 × (left, no fire), 1 × (stop, no fire)

#### Scenario: Line breaks inside the body
- **WHEN** a line break falls between `r1` and `2` of the run `r12`, or inside a seed
- **THEN** the result is the same as without the line break

#### Scenario: Initials before a seed
- **WHEN** the stream at the start of a round is `*mmm0001E240`
- **THEN** the four characters `*mmm` are skipped and the round's seed is 0x0001E240

### Requirement: DRF playback supplies one record per control read
During DRF playback the system SHALL take one record from the stream each
time the game reads a digger's controls for movement, and at no other time.
A control read happens on a tick only for a digger that is alive, while
diggers are visible, and not during its pause after pushing a bag. Two
simultaneous diggers draw from the one stream, digger 1 before digger 2
within a tick. Live controls SHALL have no effect.
(research 05 §1.6, §4.3; research 02 §3.3)

#### Scenario: Dying digger consumes nothing
- **WHEN** the digger is in its death sequence for 89 ticks
- **THEN** no record is consumed during those ticks and the next record is used on the first tick of the next life

#### Scenario: Bag-push pause consumes nothing
- **WHEN** the digger pushes a bag and is paused for the following ticks
- **THEN** no record is consumed on the paused ticks

#### Scenario: Two diggers share the stream
- **WHEN** both diggers have a control read on the same tick and the next two records are `R` and `l`
- **THEN** digger 1 gets (right, fire) and digger 2 gets (left, no fire)

#### Scenario: One digger dead in a two-digger game
- **WHEN** digger 1 is dead and digger 2 is alive for 10 ticks
- **THEN** exactly 10 records are consumed and all go to digger 2

### Requirement: DRF records replace the resolved direction and the fire input
A DRF record SHALL replace the digger's resolved movement direction for that
control read (stop means no direction) and its fire input. The fire input
SHALL take effect on the following tick, as for live play. On a tick where a
digger has no control read, its fire input for the following tick SHALL be
"not pressed". (research 05 §1.6, §1.7, §4.3, §11 items 3-4)

#### Scenario: Fire is delayed one tick
- **WHEN** the record consumed on tick N is `R` (right with fire) and the fireball is available
- **THEN** the digger moves right on tick N and the fireball is launched on tick N+1

#### Scenario: No lingering fire across a pause
- **WHEN** a record with fire is consumed on tick N and the digger has no control read on tick N+1
- **THEN** fire is active on tick N+1 and not active on tick N+2

### Requirement: DRF round boundaries and end of data
At each round start the next seed SHALL be read from the stream and at each
round end `EOL` SHALL be consumed. If a control read finds `E`, `e` or the
end of the stream, playback SHALL end as a quit: that read yields no
direction and no fire, the current tick completes and is counted, and the
game ends. If a round ends with a repeat count unfinished or the stream not
at `EOL`, the playback SHALL be reported as out of step with the recording.
(research 05 §4.2, §11 item 12; research 01 §3.2)

#### Scenario: Game recorded to its natural end
- **WHEN** the last round's records are all consumed and the game ends by itself with `EOL` next in the stream
- **THEN** `EOL` is consumed, the playback is complete and anything after it (initials) is ignored

#### Scenario: Recording ended by Exit
- **WHEN** a control read on tick N finds `EOL` followed by `EOG`
- **THEN** the digger does not move on tick N, tick N is counted in the tick total, and the game ends as quit after it

#### Scenario: Stream ends without EOL
- **WHEN** a control read finds the end of the file
- **THEN** the playback ends exactly as if `EOL` had been found, without reading out of bounds

#### Scenario: Out of step at round end
- **WHEN** a round ends while 5 repeats of a run are still pending
- **THEN** an "out of step with the recording" failure is reported with the tick number and the game is not reported as verified

### Requirement: eDRF record grammar
After its four header lines an eDRF recording SHALL consist of one record per
line, in game order. Line ends are LF or CR LF. Blank lines SHALL be skipped
except inside a level map. A final line that is not terminated by a line end
SHALL be discarded as incomplete. Any line that matches no record form SHALL
be a "bad record" error that quotes the line and its line number.
(research 05 §5.2)

#### Scenario: Grammar
- **WHEN** an eDRF body is parsed
- **THEN** each line is accepted if and only if it matches one of

```
input := "I" slots (" " runs)+        ; controls for the ticks since the previous record
slots := "" | "0" | "1" | "01"        ; strictly ascending slot digits
runs  := (ctl [count])+               ; ctl = one char '@'..'_' ; count = decimal repeat, absent = 1
level := "L " level                   ; followed by 10 map-row lines
       | "L " level " " n             ; reuse the n-th (1-based) distinct map seen so far
row   := " " up-to-15-map-chars | ""  ; an empty line is a row of plain earth
seed  := "R " hex8                    ; RNG seed of the round that starts
ckpt  := "C " hex8                    ; state hash when the tick count is a multiple of 256
rend  := "E " hex8                    ; round-end hash
quit  := "Q " slot                    ; the game was quit by that slot's player
end   := "Z " ticks " " hex8          ; decimal tick total and final state hash
```

#### Scenario: Bad record
- **WHEN** line 57 of an eDRF file is `X 1234`, `C 12G4`, `C 1 2`, `Q 2`, `Z 100` or `I0`
- **THEN** reading fails with a bad-record error that contains `57` and the offending line

#### Scenario: Record after the end record
- **WHEN** any non-blank line follows the `Z` record
- **THEN** reading fails with a bad-record error

#### Scenario: Unterminated last line
- **WHEN** the file ends with `I @8D13` and no line end
- **THEN** that line is ignored and the recording is treated as cut short before it

### Requirement: eDRF control values and input records
An eDRF control character SHALL encode one tick of one slot's raw controls as
`'@' + bits`, with up = 1, down = 2, right = 4, left = 8, fire = 16. An `I`
record without a slot list SHALL apply its single run list to both slots.
With a slot list it SHALL carry one run list per listed slot, in order; an
unlisted slot gets "no controls" for as many ticks as the first listed slot.
A wrong number of run lists, a character outside `'@'..'_'` or a zero count
SHALL be a bad record. (research 05 §5.2)

#### Scenario: Control characters
- **WHEN** the characters `@`, `A`, `B`, `D`, `H`, `P`, `Q` and `T` are decoded
- **THEN** they mean, in order: nothing, up, down, right, left, fire, fire+up, fire+right

#### Scenario: Record for all slots
- **WHEN** the record is `I @8D13A5@3P2@`
- **THEN** both slots receive 32 ticks: 8 × nothing, 13 × right, 5 × up, 3 × nothing, 2 × fire, 1 × nothing

#### Scenario: Record for one slot
- **WHEN** the record is `I1 D4H2`
- **THEN** slot 1 receives right for 4 ticks then left for 2 ticks, and slot 0 receives 6 ticks of nothing

#### Scenario: Record for both slots separately
- **WHEN** the record is `I01 D3 @2P`
- **THEN** slot 0 receives 3 × right and slot 1 receives 2 × nothing then 1 × fire

#### Scenario: Wrong number of run lists
- **WHEN** the record is `I01 D3` or `I10 D3 @3`
- **THEN** it is a bad record

### Requirement: eDRF playback supplies raw controls per slot per tick
During eDRF playback the system SHALL give each input slot the next recorded
control value on every tick, in place of the live controls and before
direction resolution, so that last-pressed-wins resolution, the one-tick tap
pulse and the one-tick fire delay behave as in live play. Slot 0 serves the
only digger or digger 1, slot 1 digger 2; alternating players both use slot
0. A slot not read on a tick SHALL still use up one recorded value.
(research 05 §1.6, §5.1, §5.3, §11 items 3 and 6)

#### Scenario: Controls go through direction resolution
- **WHEN** slot 0's values on three consecutive ticks are right, right+up, right
- **THEN** the digger's resolved direction on those ticks is right, up, right

#### Scenario: Dead digger still uses values
- **WHEN** the digger is dying for 89 ticks
- **THEN** 89 recorded values of slot 0 are used up during those ticks

#### Scenario: Slot not read on a tick
- **WHEN** a gauntlet game times out so that no controls are read on the final tick
- **THEN** each slot's value for that tick is used up and discarded, and the following tick (if any) uses the next value

#### Scenario: One-digger game ignores slot 1
- **WHEN** a one-digger eDRF is played
- **THEN** slot 1's values are used up one per tick and have no effect on the game

### Requirement: eDRF inline level maps
During eDRF playback each time a level is set up for a participating player
the system SHALL take the next `L` record, require its level number to equal
the level being started, and use its 10 × 15 map (or the earlier map it
refers to) as that level's plan. A mismatch SHALL be reported as
`level N starts, recorded level M`. An over-long row, a non-empty row not
starting with a space, a missing row, or a reference to a map not yet seen
SHALL be a bad record. (research 05 §5.2, §5.3)

#### Scenario: New map
- **WHEN** the record `L 1` is followed by 10 lines each starting with a space
- **THEN** level 1 is played on that map, short rows being padded with earth

#### Scenario: Reused map
- **WHEN** the record is `L 9 6` and six distinct maps have been given so far
- **THEN** level 9 is played on the sixth of them

#### Scenario: Reference to an unknown map
- **WHEN** the record is `L 9 7` and only six distinct maps have been given so far
- **THEN** reading fails with a bad-record error

#### Scenario: Level number mismatch
- **WHEN** the game starts level 3 and the next level record is `L 4`
- **THEN** the playback is reported as failed with the message `level 3 starts, recorded level 4`

### Requirement: Two-digger start-cell patch applies to DRF, not eDRF
When a two-digger game is played from a DRF recording, or played live, the
bottom-row cells at columns 6 and 8 of level plans 3 and 4 SHALL be
horizontal tunnel whatever the map says. During eDRF playback maps SHALL be
used exactly as recorded, because recorded eDRF maps already contain the
patch. (research 05 §4.6 item 6, §11 item 8; research 01 §5.5)

#### Scenario: DRF playback of a two-digger game
- **WHEN** a DRF with mode `M2` reaches level 3 and its header map has earth at row 9, columns 6 and 8
- **THEN** both cells are open horizontal tunnel when the level starts

#### Scenario: eDRF playback uses the recorded map
- **WHEN** an eDRF with mode `M2` supplies a level-3 map whose row 9 has earth at column 6
- **THEN** that cell is earth when the level starts

### Requirement: End of an eDRF recording
During eDRF playback, when every slot's recorded values are used up and no
record remains, the system SHALL quit the game at the start of the next
frame. A `Q <slot>` record states which player quit and SHALL precede `Z`.
A recording that ends without a `Z` record SHALL be classified as truncated.
(research 05 §5.3, §11 item 7)

#### Scenario: Recording of a quit game
- **WHEN** an eDRF ends with `Q 0` then `Z 1024 <hash>` (upstream's `2riptest01.edrf`)
- **THEN** the playback quits after tick 1024 and its tick total is 1024

#### Scenario: Recording of a game played to its end
- **WHEN** the recorded game ended by losing the last life
- **THEN** the playback ends by itself with every recorded value used up and the `Z` record still to compare

#### Scenario: Recording cut short
- **WHEN** an eDRF stops after an `I` record with no `Z`
- **THEN** the playback quits when the values run out and the recording is classified as truncated

### Requirement: Playback settings come only from the recording
A playback SHALL take the game mode, gauntlet time, start level, bonus-life
interval, level maps, per-round seeds and controls from the recording and
from nothing else, SHALL play without the unlimited-lives option, and SHALL
leave the high-score table unchanged. The settings in force before the
playback SHALL be in force again after it. (research 05 §2.3, §4.1, §4.6;
research 01 §7.3)

#### Scenario: Seeds come from the recording
- **WHEN** a recording's first two rounds carry the seeds `00000000` and `1A2B3C4D`
- **THEN** round 1 starts with random-number state 0 and round 2 with 0x1A2B3C4D

#### Scenario: Maps come from the recording
- **WHEN** a DRF header's plan 1 differs from the built-in level 1
- **THEN** level 1 of the playback uses the header's plan

#### Scenario: Settings are restored
- **WHEN** a gauntlet recording is played while the player's own setting is a one-player game starting at level 4
- **THEN** after the playback the player's setting is still a one-player game starting at level 4

### Requirement: Writing a DRF recording
The system SHALL be able to record any game, live or played back, as DRF
text: the header (`DRF`, a version line dated after 19981125, the mode line,
the bonus score, 80 map rows of 15 characters), then per round the seed as 8
upper-case hex digits on its own line, one record per control read
run-length encoded with a maximum run of 999, and `EOL` on its own line;
`EOG` follows when the game was quit. Lines end in LF. A recording SHALL
have no length limit. (research 05 §4.4)

#### Scenario: Mode line written
- **WHEN** games are recorded in these modes: one player; two alternating players; two diggers; one-digger gauntlet of 120 s; two-digger gauntlet of 90 s; one player from level 5
- **THEN** the mode lines are `1`, `2`, `M2`, `G120`, `M2G90` and `1I5`

#### Scenario: Run-length encoding
- **WHEN** a round's control reads are 1 × stop, 1000 × right without fire, 2 × left with fire
- **THEN** the round's run text is `sr999rL2`

#### Scenario: Line wrapping
- **WHEN** runs are written, each adding 1 plus its number of count digits to a column counter
- **THEN** a line break is written, and the counter reset, each time the counter reaches 60 or more

#### Scenario: Quit game
- **WHEN** the player quits during round 3
- **THEN** round 3's records are followed by `EOL` and then `EOG`

#### Scenario: Very long game
- **WHEN** a game produces more than 262144 bytes of recording
- **THEN** the whole recording is kept and replays to the same result

#### Scenario: Initials
- **WHEN** the player enters the initials `abc` for the high-score table after the game
- **THEN** the line `*abc` is appended to the recording

### Requirement: Writing an eDRF recording
The system SHALL be able to record any game as eDRF text: the four header
lines; an `L` record each time a level is set up (a map already written is
referred to by number); `R` with the seed at each round start; each slot's
controls on each tick in `I` records of at most 32 ticks, written when the
tick count is a multiple of 32 and before any other record; and the `C`,
`E`, `Q` and `Z` records. Hex numbers are 8 upper-case digits.
(research 05 §5.2, §5.3)

#### Scenario: One-digger game uses the short form
- **WHEN** only slot 0 is read during a 32-tick chunk
- **THEN** the chunk is written as one `I` record without a slot list

#### Scenario: Differing slots use the slot list
- **WHEN** in a two-digger game slot 0 holds right for 32 ticks and slot 1 holds nothing
- **THEN** the chunk is written as `I0 D32`

#### Scenario: Both slots active and different
- **WHEN** slot 0 holds right and slot 1 holds left for 32 ticks
- **THEN** the chunk is written as `I01 D32 H32`

#### Scenario: Map reuse
- **WHEN** level 9 uses the same map as level 6, which was the sixth distinct map written
- **THEN** level 9's record is `L 9 6` with no map rows

#### Scenario: Recording is usable at any chunk boundary
- **WHEN** the text produced up to tick 96 of a running game is saved and played back
- **THEN** it replays the first 96 ticks and is classified as truncated

### Requirement: Re-recording and conversion between formats
A playback SHALL itself be recordable in either format, which converts a
recording from one format to the other. When a DRF playback is recorded as
eDRF, each slot's value on a tick SHALL be the control bits of the direction
and fire that the DRF supplied on that tick (right 4, up 1, left 8, down 2,
stop 0, fire 16), and 0 on ticks where that digger had no control read. The
converted recording SHALL replay to the same scores, level and tick total.
(research 05 §3, §5.3, §7.1)

#### Scenario: DRF to eDRF matches the upstream twin
- **WHEN** each of upstream's 23 `.drf` recordings is played and recorded as eDRF
- **THEN** the result has the same level, seed, checkpoint, round-end, quit and end records and the same control value for every slot on every tick as the upstream `.edrf` of the same base name

#### Scenario: eDRF to DRF
- **WHEN** an eDRF recording is played and recorded as DRF, and that DRF is played
- **THEN** the final scores, level and tick total equal those of the eDRF playback

#### Scenario: Round trip of a live game
- **WHEN** a game is recorded as eDRF, and that recording is played back while being recorded as eDRF again
- **THEN** the second text is byte-for-byte identical to the first

### Requirement: Saving and default file names
A save name ending in `.edrf` SHALL select the eDRF format; any other name
selects DRF. With no name given, a game of two alternating players SHALL be
saved as `DIGGER.DRF`; any other game as initials + final score + `.drf`,
where non-letters in the three initials (and missing initials) become `_`.
Below 100000 points all three initials are used; from 100000 points two are
used: the first two if the third is `_`, else the last two if the first is
`_`, else the first and third. (research 05 §4.5)

#### Scenario: Score below 100000
- **WHEN** the initials are `mmm` and the score is 27700
- **THEN** the default name is `mmm27700.drf`

#### Scenario: High score with three letters
- **WHEN** the initials are `abc` and the score is 1859075
- **THEN** the default name is `ac1859075.drf`

#### Scenario: High score with a non-letter
- **WHEN** the score is 250000 and the initials are `ab1`, or `1bc`
- **THEN** the default names are `ab250000.drf` and `bc250000.drf`

#### Scenario: No initials entered
- **WHEN** a one-player game ends with 3100 points and no initials were entered
- **THEN** the default name is `___3100.drf`

#### Scenario: Alternating two-player game
- **WHEN** a game of two alternating players is saved without a name
- **THEN** the name is `DIGGER.DRF`

### Requirement: Malformed input never crashes
Reading or playing any byte sequence, however malformed or truncated, SHALL
end in either a playable recording or an error that says what is wrong and
where (line number, or character offset in a DRF body). It SHALL never read
outside the data, crash, hang, or use memory in proportion to a repeat
count. In a DRF body a seed with a non-hex character, a run letter other
than the ten defined ones and `E`/`e`, or a count above 4294967295 SHALL be a
bad record. (research 05 §4.2, §11 item 12)

#### Scenario: Arbitrary bytes
- **WHEN** any of 10000 random byte strings, or any prefix of an upstream recording, is read and played
- **THEN** each attempt ends with a result or an error, and none crashes or runs for more ticks than the data can supply controls for

#### Scenario: Huge repeat count
- **WHEN** an eDRF contains `I @4000000000` or a DRF contains the run `s4000000000`
- **THEN** reading succeeds without reserving memory for four thousand million values

#### Scenario: Bad DRF seed
- **WHEN** a round's seed field is `12G45678` or is cut off after 5 digits
- **THEN** playback stops with a bad-record error giving the character offset

#### Scenario: Unknown DRF run letter
- **WHEN** the stream holds `x5` where a run is expected
- **THEN** playback stops with a bad-record error giving the character offset

### Requirement: Taking over from a playback
The system SHALL allow a playback to be handed over to live controls after a
chosen tick, or on request. From that point the recording supplies no more
controls or checkpoints, live controls drive the game, seeds for later rounds
are generated as in live play, no further verification is done, and the game
SHALL NOT be eligible for a high score. A recording being made of the game
SHALL continue across the hand-over. (research 05 §3, §4.6)

#### Scenario: Hand-over at a tick
- **WHEN** a playback is asked to hand over after tick 500
- **THEN** ticks 1 to 500 use the recorded controls and tick 501 onward use live controls

#### Scenario: Re-recorded taken-over game
- **WHEN** a taken-over game is recorded and that recording is played back
- **THEN** it reproduces the whole game: the played-back part followed by the live part

#### Scenario: No high score
- **WHEN** a taken-over game ends with a score that would enter the high-score table
- **THEN** the result is marked as taken over and not eligible for the table
