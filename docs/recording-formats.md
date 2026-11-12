# Recording formats

The two recording formats of Digger Remastered as the port reads them: the
DRF text format and the extended eDRF format (`eDRF 2`). This is the
reference for `digger_core::recording` (change
`add-recording-and-conformance`, game-recording spec; research 05 §4 and
§5). Later sections, added with the writers, the state hash and playback,
cover writing, the state hash and the playback rules.

Both formats are text. Lines end in LF or CR LF; the two are read alike
everywhere. Upstream's 23 `.drf` and 27 `.edrf` recordings in
`tests/recordings/data/` are the acceptance test for every rule below
(`corpus_drf_bodies_tokenise`, `corpus_edrf_parse`).

## Common header

| Line | Content |
|---|---|
| 1 | `DRF` (only the first three bytes are checked) or exactly `eDRF 2`; anything else is "not a recording" |
| 2 | version line, free text of the form `XX YYY YYYYMMDD`, for example `AJ DOS 19990506` or `MS MPL 20190212` |
| 3 | mode line |
| 4 | bonus score: one decimal number from 0 to 65535, the interval at which an extra life is awarded |

The date is the decimal number at byte offset 7 of the version line, read
as C's `atol` reads it; no digits read as 0. A recording dated 19981125 or
earlier is refused (`recording "<version line>" not supported: recorded by a
version dated 19981125 or earlier`): those versions moved monsters
differently and upstream emulated them with a "kludge" the port does not
carry.

Mode line:

```
mode := ("1" | "2" | ["M" digit] ["G" digits]) ["U"] ["I" digits]
```

`1`/`2` alternating players, `M1`/`M2` simultaneous diggers, `G<n>` a
gauntlet of 1 to 3599 seconds, `I<n>` start level 1 to 1000. `U` (unlimited
lives) is read and ignored: playback never has unlimited lives. An empty
mode line is a one-player game.

## DRF

### Layout

After the four header lines come exactly 80 map lines: the eight level
plans, plan 1 first, ten rows each, top row first. Only the first 15 bytes
of a line are used; `S V H B C` are the map letters, and every other byte,
and every column a short line lacks, is plain earth. Fewer than 80 map lines
is a header error.

The rest of the file is the body. Every byte below 0x20 is dropped from it,
so line breaks (upstream wraps at about 60 columns) are insignificant, even
inside a seed or a count.

### Body grammar

```
body   := round* ["EOG"] trailer
trailer:= any bytes               ; usually "*" c c c, the initials; ignored
round  := ["*" c c c] seed run* "EOL"
seed   := 8 hex digits            ; round RNG seed, most significant digit first
run    := dirchar [count]
dirchar:= "s"|"r"|"u"|"l"|"d"|"S"|"R"|"U"|"L"|"D"
count  := decimal digits          ; repeat count; 0 and 1 both mean once
```

`s r u l d` are stop, right, up, left and down without fire; the capitals
are the same with fire. Seeds take hex digits of either case.

### Semantics

- One record is taken per control read, not per tick: the body cannot be
  decoded without running the game. `DrfRecording` keeps the body as bytes
  and a `DrfCursor`, one per playback, reads it as the game asks: a seed at
  each round start, a record at each control read, `EOL` at each round end.
  Runs are never expanded; a run of four thousand million costs one count.
- A control read that meets `E` or `e` (the start of `EOL` or `EOG`) or the
  end of the data finds the end marker: the game ends as quit. The end of
  the data also counts as `EOL` at a round end.
- What follows the last round is the trailer and is ignored. Upstream's
  files hold `EOG` and the initials there as `*abc`, twice in `2test01.drf`,
  and without the `*` in `p1092525.drf` and `s1058800.drf`.

### Deviations from upstream's reader

- A seed with a byte that is not a hex digit, a seed cut off by the end of
  the data, a run letter other than the ten above and `E`/`e`, and a count
  above 4294967295 are errors that give the byte offset in the file
  (`DRF: bad record at offset N: ...`). Upstream reads garbage silently.
- A round end where a run still has repeats to give, or where the body does
  not hold `EOL`, is reported as out of step instead of being ignored.
- Short map rows are earth; upstream stores the NUL that ended the line.
- The run counter starts afresh with every playback; upstream's is a static
  variable never reset.
- No line-length or file-size limit.
- Pre-1998 recordings are refused instead of emulated.

## eDRF

### Layout

The four header lines, then one record per line, in game order. There is no
map block in the header: maps come with the `L` records as levels start.
Blank lines are skipped, except inside a level map. A last line without a
line end is dropped as incomplete, and the recording counts as cut short
before it (the program recording it was stopped while writing).

### Record grammar

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

Every separator is exactly one space. `hex8` is exactly eight hex digits of
either case (upstream writes `%08X`); `ticks`, `level` and `n` are decimal
numbers up to 4294967295; `slot` is `0` or `1`. Any line that matches none
of the forms is `eDRF: bad record at line N: "<line>"`.

### Control values

A control character is `'@'` plus the slot's raw control bits on one tick:
up = 1, down = 2, right = 4, left = 8, fire = 16. So `@` is nothing, `A` up,
`B` down, `D` right, `H` left, `P` fire, `Q` fire+up, `T` fire+right, up to
`_` (all five). The values are injected before direction resolution: on
playback the recorded controls go through the same last-pressed-wins
resolution, tap pulse and one-tick fire delay as live controls.

### `I` records

- An `I` record covers the ticks since the previous record (the writer
  flushes every 32 ticks and before every other record).
- A count is 1 to 4294967295; absent means 1; leading zeros are allowed;
  0 is a bad record. A run list must not be empty.
- No slot list (`I @8D13A5@3P2@`): one run list for both slots. This is the
  normal form of a one-digger game.
- A slot list (`I0`, `I1`, `I01`): exactly one run list per listed slot, in
  slot order. A slot not listed gets "no controls" for as many ticks as the
  first listed slot's list covers (`I1 D4H2` gives slot 0 six ticks of
  nothing). A wrong number of run lists, a slot digit other than 0 and 1, or
  digits not strictly ascending (`I10`, `I00`) is a bad record.
- The runs are kept as `(value, count)` pairs, one queue per slot, used up
  by counting down: `I @4000000000` costs one pair, and padding an unlisted
  slot is one pair however long.
- Every slot gets one value per tick, read or not, so in a recording played
  to its end each slot's queue holds exactly the `Z` tick total.

### `L` records and maps

- `L <level>` is followed by ten row lines. A row is empty (plain earth) or
  a space and up to 15 map characters, padded with earth. A row longer than
  15 characters, a non-empty row not starting with a space, or a map cut
  off by the end of the data (fewer than ten complete rows) is a bad record;
  the last is reported at the `L` line.
- The map's characters are kept exactly as recorded. During playback the
  map is used verbatim: recorded maps already carry the two-digger
  start-cell patch, so it is not applied again.
- `L <level> <n>` plays the level on the n-th map given inline so far,
  counted from 1. An index of 0 or beyond the maps given is a bad record.
  The writer never repeats a map inline, so "the n-th inline map" and "the
  n-th distinct map" are the same.

### End of the recording

- `Q <slot>` says which player quit; it comes before `Z`. Upstream's
  `2riptest01.edrf` (quit at tick 1024) and `2test03.edrf` end with `Q 0`.
- `Z <ticks> <hash>` is the last record. Only blank lines may follow it;
  any other line after it is a bad record.
- A recording without `Z` was cut short: it is classified as truncated
  once played.

### Data model

`EdrfRecording::parse` reads the whole file before play starts into one
queue per kind: control runs per slot, seeds, levels (with their maps
resolved), `C` hashes, `E` hashes, the quit slot, the end record and the
truncated flag. `EdrfRecords` gives the same records one at a time, with
their line numbers, for tools that copy or cut a recording.

### Deviations from upstream's reader

Upstream reads records lazily with `sscanf`; the port reads them eagerly
and by the grammar above.

- A bad record fails before play starts, not when playback reaches it.
- Record forms are strict: one space after the letter and between fields,
  exactly eight hex digits, nothing after the last field. Upstream also
  accepts `R1234`, `R  1234`, short hashes, text after an `R` or `Z`
  record, and `Q 00`. Every upstream recording uses the strict forms.
- A zero count is a bad record wherever it stands; upstream refuses it only
  when the whole run list then covers no tick. Counts above 4294967295 are
  bad records.
- A carriage return ends a line only directly before its LF; upstream cuts
  every line at its first CR.
- No 511-byte line limit (upstream splits longer lines into several
  records) and no limit on the file size.
- A map cut off by the end of the data, and any record after `Z`, are bad
  records; upstream leaves such a level pending or reads on.

## State hash

The `C`, `E` and `Z` records of an eDRF recording carry a hash of the game
state (replay-verification spec; research 01 §8). The port computes it in
`digger_core::state_hash` from one field table (`state_hash/layout.rs`);
the table below is generated from it, and the test
`state_hash::layout::tests::layout_doc_is_current` fails when this section
is stale (run it with `DIGGER_UPDATE_DOCS=1` to rewrite the table).

### Value

The hash is the MD5 digest of the state stream cut to its first four
bytes, read as one unsigned 32-bit number with the first digest byte most
significant. In a record it is written as exactly eight upper-case
hexadecimal digits, so it equals the first eight characters of the
digest's usual hex form, leading zeros included (`000ABC01`).

### Encoding

Every value is written at a fixed width in little-endian byte order, with
no padding and no separators: 8-bit values as one byte, 16-bit values as
two, 32-bit values as four, yes/no values as one byte 0 or 1. Negative
values are two's complement; the gauntlet clock and the tick count are
unsigned. The simulation's enums are written as the original's numbers
(`state_hash/codes.rs`):

| Value | Codes |
|---|---|
| Direction | right 0, up 2, left 4, down 6, none −1 (FF FF) |
| Digger death stage | 0 never placed, 1 bag (also while alive), 2 grave, 3 monster, 4 tombstone, 5 arc, 6 out of play |
| Monster death | 0 none, 1 carried by a bag, 4 bag stopped |
| Fireball stage | 0 flying (or none in play), 1 to 3 exploding, 4 spent |
| Bag index, digger record index | 0-based |
| Emerald mask | 1 first player, 2 second player |

### Order and conditions

The stream holds the playfield block, the digger block, the monster
block, the bag block, then the random-number state, both total scores and,
in the gauntlet modes only, the gauntlet clock and the time-out flag. The
round-end form (`E` records) appends the tick count. A per-cell entry
occurs 150 times in a row, cell (h, v) at position v × 15 + h. A run of
consecutive per-slot entries is written once for each slot in turn: both
digger slots, all six monster slots and all seven bag slots, whether or not
the game uses them (quirk P10). The digger records are the original's
`digdat[0]` and `digdat[1]`: in an alternating game record `n` is player
`n`'s. The flags `fire_pressed` and `level_done` are kept by the original
but never set and are always 0.

A monster slot's body values (`with monster body`) are present from the
tick a monster is spawned into the slot until the monster reset at the
start of the next round; a monster that was killed or removed keeps them,
with "alive" 0, until then. A stream is therefore 903 bytes long in a
non-gauntlet game with no monster body, 908 in a gauntlet game, and up to
951 (plus 5) with all six bodies; the round-end form adds 4.

Only the live state of the player currently playing is covered: the field
in play and the bags in play, never the saved field or bags of the other
alternating player. Nothing about the picture, the palette, sound, pause
or real time enters the stream. Values the original keeps but does not
hash — the dirge timer, the death-music progress, a digger's "fireball
available" look, a monster's "dead and still shown" flag and its animation
— do not change the hash.

The round-end hash is taken after the round's clean-up: fireballs and the
bonus removed, the bags tidied, the field saved and the monster sprites
erased (quirk P11).

<!-- state-hash-table:begin -->
| # | Block | Name | Width | Repeat | Present | Value |
|---|---|---|---|---|---|---|
| 1 | playfield | `cell` | 16-bit | per cell | always | tunnel bit set of the cell (`field`) |
| 2 | digger | `bonus_start_time_left` | 16-bit | once | always | bonus-start countdown (`startbonustimeleft`) |
| 3 | digger | `bonus_time_left` | 16-bit | once | always | bonus time left (`bonustimeleft`) |
| 4 | digger | `emerald_mask` | 16-bit | once | always | emerald bit of the current player, 1 or 2 (`emmask`) |
| 5 | digger | `bonus_visible` | bool | once | always | bonus (cherry) shown (`bonusvisible`) |
| 6 | digger | `bonus_mode` | bool | once | always | bonus mode on (`bonusmode`) |
| 7 | digger | `diggers_visible` | bool | once | always | diggers shown (`digvisible`) |
| 8 | digger | `emeralds` | 8-bit | per cell | always | emerald bits of the cell, bit 0 player 1, bit 1 player 2 (`emfield`) |
| 9 | digger | `h` | 16-bit | per digger | always | cell column (`h`) |
| 10 | digger | `v` | 16-bit | per digger | always | cell row (`v`) |
| 11 | digger | `rx` | 16-bit | per digger | always | x remainder (`rx`) |
| 12 | digger | `ry` | 16-bit | per digger | always | y remainder (`ry`) |
| 13 | digger | `moved` | 16-bit | per digger | always | direction moved this tick (`mdir`) |
| 14 | digger | `bag_time` | 16-bit | per digger | always | bag-push pause (`bagtime`) |
| 15 | digger | `recharge_time` | 16-bit | per digger | always | fireball recharge countdown (`rechargetime`) |
| 16 | digger | `death_stage` | 16-bit | per digger | always | death stage (`deathstage`) |
| 17 | digger | `death_bag` | 16-bit | per digger | always | index of the killing bag (`deathbag`) |
| 18 | digger | `death_frame` | 16-bit | per digger | always | death animation counter (`deathani`) |
| 19 | digger | `death_timer` | 16-bit | per digger | always | death timer (`deathtime`) |
| 20 | digger | `emerald_window` | 16-bit | per digger | always | emerald-streak window countdown (`emocttime`) |
| 21 | digger | `emerald_run` | 16-bit | per digger | always | emeralds in the streak (`emn`) |
| 22 | digger | `eat_multiplier` | 16-bit | per digger | always | monster-eating score multiplier (`msc`) |
| 23 | digger | `lives` | 16-bit | per digger | always | lives (`lives`) |
| 24 | digger | `invincibility` | 16-bit | per digger | always | invincibility ticks (`ivt`) |
| 25 | digger | `not_firing` | bool | per digger | always | no fireball in play (`notfiring`) |
| 26 | digger | `fire_pressed` | bool | per digger | always | fire pressed, never set: always 0 (`firepressed`) |
| 27 | digger | `dead` | bool | per digger | always | death sequence over (`dead`) |
| 28 | digger | `level_done` | bool | per digger | always | level done, never set: always 0 (`levdone`) |
| 29 | digger | `invincible` | bool | per digger | always | invincible (`invin`) |
| 30 | digger | `x` | 16-bit | per digger | always | sprite x (`dob.x`) |
| 31 | digger | `y` | 16-bit | per digger | always | sprite y (`dob.y`) |
| 32 | digger | `facing` | 16-bit | per digger | always | facing direction (`dob.dir`) |
| 33 | digger | `alive` | bool | per digger | always | alive and controllable (`dob.alive`) |
| 34 | digger | `fireball_x` | 16-bit | per digger | always | fireball x (`bob.x`) |
| 35 | digger | `fireball_y` | 16-bit | per digger | always | fireball y (`bob.y`) |
| 36 | digger | `fireball_direction` | 16-bit | per digger | always | fireball direction (`bob.dir`) |
| 37 | digger | `fireball_stage` | 16-bit | per digger | always | fireball explosion stage (`bob.expsn`) |
| 38 | monster | `next_monster` | 16-bit | once | always | monsters spawned this round (`nextmonster`) |
| 39 | monster | `total_monsters` | 16-bit | once | always | monsters for the level (`totalmonsters`) |
| 40 | monster | `max_on_screen` | 16-bit | once | always | most monsters at once (`maxmononscr`) |
| 41 | monster | `next_spawn_time` | 16-bit | once | always | time to next spawn (`nextmontime`) |
| 42 | monster | `spawn_gap` | 16-bit | once | always | gap between spawns (`mongaptime`) |
| 43 | monster | `next_chase` | 16-bit | once | always | digger the next monster chases (`chase`) |
| 44 | monster | `bonus_pending` | bool | once | always | bonus still to appear this round (`unbonusflag`) |
| 45 | monster | `h` | 16-bit | per monster | always | cell column (`h`) |
| 46 | monster | `v` | 16-bit | per monster | always | cell row (`v`) |
| 47 | monster | `xr` | 16-bit | per monster | always | x remainder (`xr`) |
| 48 | monster | `yr` | 16-bit | per monster | always | y remainder (`yr`) |
| 49 | monster | `direction` | 16-bit | per monster | always | movement direction (`dir`) |
| 50 | monster | `delay` | 16-bit | per monster | always | delay before next move (`t`) |
| 51 | monster | `counter` | 16-bit | per monster | always | nobbin crossing count / time as hobbin (`hnt`) |
| 52 | monster | `death` | 16-bit | per monster | always | death stage (`death`) |
| 53 | monster | `death_bag` | 16-bit | per monster | always | squashing bag index (`bag`) |
| 54 | monster | `death_delay` | 16-bit | per monster | always | death delay (`dtime`) |
| 55 | monster | `standstill` | 16-bit | per monster | always | start delay (`stime`) |
| 56 | monster | `chase` | 16-bit | per monster | always | digger record chased (`chase`) |
| 57 | monster | `in_use` | bool | per monster | always | slot in use (`flag`) |
| 58 | monster | `x` | 16-bit | per monster | with monster body | sprite x (`pos.x`) |
| 59 | monster | `y` | 16-bit | per monster | with monster body | sprite y (`pos.y`) |
| 60 | monster | `facing` | 16-bit | per monster | with monster body | facing direction (`pos.dir`) |
| 61 | monster | `alive` | bool | per monster | with monster body | alive (`isalive`) |
| 62 | monster | `nobbin` | bool | per monster | with monster body | is a nobbin (`ISNOB`) |
| 63 | bag | `push_count` | 16-bit | once | always | push counter (`pushcount`) |
| 64 | bag | `gold_time` | 16-bit | once | always | gold lifetime for the level (`goldtime`) |
| 65 | bag | `x` | 16-bit | per bag | always | pixel x (`x`) |
| 66 | bag | `y` | 16-bit | per bag | always | pixel y (`y`) |
| 67 | bag | `h` | 16-bit | per bag | always | cell column (`h`) |
| 68 | bag | `v` | 16-bit | per bag | always | cell row (`v`) |
| 69 | bag | `xr` | 16-bit | per bag | always | x remainder (`xr`) |
| 70 | bag | `yr` | 16-bit | per bag | always | y remainder (`yr`) |
| 71 | bag | `direction` | 16-bit | per bag | always | direction (`dir`) |
| 72 | bag | `wobble` | 16-bit | per bag | always | wobble countdown (`wt`) |
| 73 | bag | `gold_age` | 16-bit | per bag | always | gold timer (`gt`) |
| 74 | bag | `cells_fallen` | 16-bit | per bag | always | cells fallen (`fallh`) |
| 75 | bag | `wobbling` | bool | per bag | always | wobbling (`wobbling`) |
| 76 | bag | `unfallen` | bool | per bag | always | not yet fallen (`unfallen`) |
| 77 | bag | `exists` | bool | per bag | always | exists (`exist`) |
| 78 | game | `rng_state` | 32-bit | once | always | random-number state (`randv`) |
| 79 | game | `score_1` | 32-bit | once | always | total score of player/digger 1 (`gettscore(0)`) |
| 80 | game | `score_2` | 32-bit | once | always | total score of player/digger 2 (`gettscore(1)`) |
| 81 | game | `gauntlet_clock` | 32-bit | once | gauntlet only | gauntlet clock, unsigned, wrapped on the time-out tick (`cgtime`) |
| 82 | game | `timed_out` | bool | once | gauntlet only | gauntlet time is out (`timeout`) |
| 83 | game | `tick` | 32-bit | once | round end only | ticks since game start, unsigned (`ticks`) |

| Block | Bytes |
|---|---|
| playfield | 300 |
| digger | 263 |
| monster | 163 to 211 |
| bag | 165 |
| game | 12 to 21 |
<!-- state-hash-table:end -->
