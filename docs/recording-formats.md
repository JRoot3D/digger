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
