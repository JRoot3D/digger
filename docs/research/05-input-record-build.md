# 05 — Input, configuration, recording/playback, timing, build and test infrastructure

Source tree: `https://github.com/sobomax/digger`
(sobomax/digger, HEAD `638125e`). All `file:line` references are relative to that
directory. Everything below was read from source; nothing was built or run
(no SDL2 dev package in this sandbox), so behaviour claims are from code
reading only.

Contents

1. Input
2. INI configuration
3. Command line (as far as it drives this area)
4. DRF recording format
5. eDRF recording format
6. Timers and the frame/tick contract
7. Tests
8. Build, CI, packaging
9. Platform/backend inventory
10. Suggested Rust design
11. Trickiest parts / quirk list

---

## 1. Input

### 1.1 Key table

`NKEYS = 20` (`input.h:32`). `int keycodes[NKEYS][5]` is defined per backend
(`sdl_kbd.c:23-42`, `fbsd_kbd.c:54-73`). Index meaning (`input.h:34-56`),
INI key name (`keyboard.c:22-25`, with `2` appended for indices 5..9), and SDL
default with its numeric `SDL_Scancode` value (this is what lands in the INI
file):

| idx | constant | INI name | action | SDL default | scancode |
|---|---|---|---|---|---|
| 0 | PKEY_RIGHT | `Right` | P1 right | Right arrow | 79 |
| 1 | PKEY_UP | `Up` | P1 up | Up arrow | 82 |
| 2 | PKEY_LEFT | `Left` | P1 left | Left arrow | 80 |
| 3 | PKEY_DOWN | `Down` | P1 down | Down arrow | 81 |
| 4 | PKEY_FIRE | `Fire` | P1 fire | F1 | 58 |
| 5 | PKEY_RIGHT2 | `Right2` | P2 right | S | 22 |
| 6 | PKEY_UP2 | `Up2` | P2 up | W | 26 |
| 7 | PKEY_LEFT2 | `Left2` | P2 left | A | 4 |
| 8 | PKEY_DOWN2 | `Down2` | P2 down | Z | 29 |
| 9 | PKEY_FIRE2 | `Fire2` | P2 fire | Tab | 43 |
| 10 | DKEY_CHT | `Cheat` | take over playback / invalidate recording | T | 23 |
| 11 | DKEY_SUP | `Accel` | speed up (ftime -= 10000 µs) | Keypad + | 87 |
| 12 | DKEY_SDN | `Brake` | slow down (ftime += 10000 µs) | Keypad - | 86 |
| 13 | DKEY_MTG | `Music` | toggle music | F7 | 64 |
| 14 | DKEY_STG | `Sound` | toggle sound | F9 | 66 |
| 15 | DKEY_EXT | `Exit` | `escape=true` (back to title; quits program from title) | F10 | 67 |
| 16 | DKEY_PUS | `Pause` | `pausef=true` | Space | 44 |
| 17 | DKEY_MCH | `Mode Change` | cycle game mode on title | N | 17 |
| 18 | DKEY_SDR | `Save DRF` | `savedrf=true` (save last game's recording) | F8 | 65 |
| 19 | DKEY_RDK | `Redefine` | `keyredef=true` (title only) | K | 14 |

Not in the table (hard-wired in the SDL event filter, `sdl_kbd.c:52-85`):

* Alt+Enter / Alt+KP-Enter → `switchmode()` (fullscreen toggle), not buffered.
* LAlt/RAlt key-downs are swallowed (so Alt+Enter does not start/unpause).
* `SDL_QUIT` → `exit(0)` directly from the filter (`sdl_kbd.c:81-82`) — no
  cleanup, no recording save.
* `/F` on the command line starts fullscreen (`main.c:1016-1020`).

Doc/code mismatches worth knowing: `digger.txt` says "Ctrl-T" for cheat and
"+/-" for speed; the code binds plain `T` and *keypad* +/- only.

Each row of `keycodes` has 5 columns; defaults are `{code,-2,-2,-2,-2}`.

* Column 0 = "primary" code, used for the *level-triggered* test
  `kbd_async_key_state(keycodes[i][0])` (`input.h:64-73`) and as the code the
  redefine screen overwrites.
* Columns 2..4 = alternate codes matched against *buffered key-down events*
  for player keys (`input.c:107-110`); columns 0..4 are all matched for the
  function keys 10..19 (`input.c:111-114`).
* Column 1 is never consulted for player keys (historical ASCII/scancode
  split). `-2` means "unused".

Consequence: with the defaults a player key is only seen through the async
state (column 0); function keys are only seen through the event buffer.

### 1.2 Storage and remapping

* Stored in the INI `[Keys]` section, one line per key:
  `Name=c0/c1/c2/c3/c4` (decimal ints), e.g. `Fire=58/-2/-2/-2/-2`,
  `Mode Change=17/-2/-2/-2/-2`, `Save DRF=65/-2/-2/-2/-2` (names contain
  spaces). Written by `redefkeyb` step 3 (`keyboard.c:153-159`), read by
  `inir()` (`main.c:1247-1262`).
* On read, the default string is formatted from the compiled-in table, looked
  up, then parsed with `atoi` per `/`-separated field; fewer than 5 fields
  leaves the remaining columns untouched (`main.c:1253-1261`).
* `inir()` sets `krdf[i]=true` for **every** key (`main.c:1252`), so the
  redefine screen always rewrites all 20 lines, not only changed ones.
  (`krdf` is declared `[NKEYS]` with 18 initialisers, `input.c:22-23`.)
* The values are backend-specific raw codes: SDL scancodes on SDL builds,
  VGL scancode+128 / ASCII on FreeBSD VGL. A Rust port that wants to read
  existing `~/.digger.rc` files must keep SDL scancode numbering (USB HID
  usage IDs).

### 1.3 Redefine-keys flow (`redefkeyb`, `keyboard.c:40-163`)

Entry points: `K` on the title screen (`main.c:505-513`, `allf=false`),
`/K` (`allf=false`) or `/KA` (`allf=true`) on the command line
(`main.c:1148-1153`).

1. `maininit()`, clear screen, print `PRESS NEW KEY FOR`; if `diggers==2`
   print `PLAYER 1:`.
2. For i in 0..4 (P1 Right, Up, Left, Down, Fire): print the name in colour 2
   (red), `prockey(i)` = blocking `getkey(true)` (a scancode); if the pressed
   key equals the current Exit key's primary code and i is not Exit, the whole
   function returns immediately (`keyboard.c:29-38`, macro `FINDKEY_EX` line
   27) — **nothing is written to the INI, but keys already reassigned stay
   changed in memory**. Otherwise `keycodes[i][0]=key`, name reprinted in
   colour 1 (green). Then duplicate check against earlier keys j<i: same
   primary code (non-zero) or any clash among alternates (cols 2..4, ≠ -2)
   → redo this key (`keyboard.c:65-81`).
3. If `diggers==2`: `PLAYER 2:` and the same for i in 5..9, checking against
   all j<i (`keyboard.c:84-110`).
4. Function keys i in 10..19 (`keyboard.c:114-149`): a key is prompted for if
   it conflicts with any player key (any column vs any column, ≠ -2) or an
   earlier function key, or — with `allf` — once unconditionally (`i!=z`
   guard). After a prompt `i--` re-checks the same key, so conflicts loop
   until resolved. Layout wraps to a second column when `y >= MAX_H-CHR_H`.
5. Write all 20 keys to `[Keys]` (`keyboard.c:153-159`); on Emscripten call
   `ems_keymap_changed()`.

### 1.4 Keyboard buffering (SDL backend, `sdl_kbd.c`)

* FIFO of 30 entries `{int16 sym, int16 scancode}` (`KBLEN`, `sdl_kbd.c:21,
  44-50`). Filled from an `SDL_SetEventFilter` callback on `SDL_KEYDOWN`
  (key repeat events included — no `repeat` check). On overflow the oldest
  entry is dropped (`sdl_kbd.c:56-60`; note the drop happens before the Alt
  filtering, so an ignored Alt press can still evict an entry).
* `kbhit()` = `SDL_PumpEvents()` + `klen>0` (`sdl_kbd.c:129-138`).
* `getkey(scancode)` blocks: `while(!kbhit()) gethrt(true,1);` then pops the
  head; returns the scancode if the argument is true, else the keysym
  truncated to int16 (`sdl_kbd.c:112-127`).
* `kbd_async_key_state(code)` = `SDL_PumpEvents()` + `SDL_GetKeyboardState`
  indexed by scancode (`sdl_kbd.c:87-97`).
* Mouse motion/button events are disabled (`sdl_kbd.c:99-106`).

### 1.5 `checkkeyb()` (`input.c:77-155`) — called once per frame from
`syncframe` (`digger.c:267`) and from a few wait loops

1. For each of the 10 player keys: if currently down (async), set its sticky
   flag (`aleftpressed`, `af1pressed`, `aleft2pressed`, …).
2. Drain the event buffer: for each key `akeypressed=getkey(true)`;
   `anykeyf=true`; set sticky flags for player keys whose alternates
   (cols 2..4) match; find function key `k` (last match of cols 0..4 over
   i=10..19) and run its action (`input.c:115-150`):
   * CHT: `if (playing) playtakeover(); else if (!gauntlet) drfvalid=false;`
   * SUP: `if (ftime>10000) ftime-=10000;` SDN: `ftime+=10000;`
   * MTG/STG: `togglemusic()/togglesound()` (post a command to the sound queue)
   * EXT: `escape=true`; PUS: `pausef=true`; MCH: `mode_change=true`;
     SDR: `savedrf=true`; RDK: `keyredef=true`.
3. `start=true` unless `mode_change` or `keyredef` is set, or (title up/down
   suppression enabled, i.e. NetSim title) the key is P1 Up/Down's primary
   code (`input.c:151-153`). So *any* key, including F7/F10/F8, starts the
   game / ends the title wait; the title loop then sorts out `escape`,
   `savedrf` (`main.c:505-528`).

Quirk: `k` is initialised once before the `while (kbhit())` loop and not
reset per key (`input.c:79`), so after a function key is matched its action
is repeated for every further key drained in the same call.

`flushkeybuf()` (`input.c:171-177`) drains the buffer and clears all ten
sticky flags; called at the start of every round (`main.c:209`).

### 1.6 Direction resolution — `readdirect(n)` / `getdirect(n)`

`n` is the *input slot* = digger index − `curplayer` (`digger.c:343,588`), so
in alternating two-player mode (`nplayers==2, diggers==1`) both players use
slot 0 = player-1 keys. Slot sources (`input.c:33-37,51-52`): slot 0 =
PRIMARY (P1 keys), slot 1 = SECONDARY (P2 keys); in a NetSim game both are
NETWORK (`input_enable_network_mode`, `input.c:228-236`) and read
`network_bits[slot]`.

`readdirect(n)` is called once per slot per game tick at the top of
`dodigger` (`digger.c:342-343`), for dead diggers too. Per call
(`input.c:371-415` primary, `416-460` secondary, `295-346` network — the
three copies are the same algorithm on separate state):

```
u = sticky_up || async_up;  clear sticky_up      (same for d, l, r)
f = async_fire || sticky_fire; clear sticky_fire
(u,d,l,r,f) = edrf_controls(n, u,d,l,r,f)        // record / replace, see §5
fire_flag[n] = f                                  // firepflag or fire2pflag
// rising edges, tested in the order U, D, L, R -> last one wins
if (u && !old_u) static = dynamic = UP
if (d && !old_d) static = dynamic = DOWN
if (l && !old_l) static = dynamic = LEFT
if (r && !old_r) static = dynamic = RIGHT
// the key that owns `dynamic` was released: fall back to any key still held
if ((old_u && !u && dynamic==UP) || (old_d && !d && dynamic==DOWN) ||
    (old_l && !l && dynamic==LEFT) || (old_r && !r && dynamic==RIGHT)) {
    dynamic = NONE
    if (u) dynamic = static = UP      // again order U, D, L, R: R wins
    if (d) dynamic = static = DOWN
    if (l) dynamic = static = LEFT
    if (r) dynamic = static = RIGHT
}
old_u=u; old_d=d; old_l=l; old_r=r
keydir[n] = static; if (dynamic != NONE) keydir[n] = dynamic
static = NONE
```

Semantics: `dynamic` is the "currently held, most recently pressed" direction
(last pressed wins; on release of that key the priority among still-held keys
is Right > Left > Down > Up). `static` is a one-tick pulse so that a tap
shorter than a tick (caught only by the sticky flag) still moves Digger for
exactly one tick. Releasing a key that is *not* the current `dynamic` changes
nothing. Direction codes: RIGHT 0, UP 2, LEFT 4, DOWN 6, NONE −1
(`def.h:23-27`).

`input_reset_directions()` (`input.c:259-277`) zeroes all of this plus the
fire flags; called at the start of every round (`main.c:210`).

`getdirect(n)` (`input.c:536-565`) is called from `updatedigger` only
(`digger.c:588`), i.e. **only on ticks where that digger is alive, visible
and not in bag-push delay** (`digger.c:348-360`). It returns `keydir`/`keydir2`,
then:

```
if (playing && !edrf_playing) playgetdir(&dir, &fireflag[n]);   // DRF playback overrides
recputdir(dir, fireflag[n]);                                     // DRF recording
edrf_movement(n, dir, fireflag[n]);                              // DRF->eDRF conversion
```

### 1.7 Fire latch

* `firepflag` / `fire2pflag` (globals, `input.c:20`) are written every tick by
  `readdirect` and, during DRF playback, overwritten later in the same tick by
  `playgetdir` inside `getdirect`.
* The game does not read them directly: `updatefire` calls `getfirepflag(n)` →
  `input_get_fire_active(n)` (`digger.c:412,1003-1007`; `input.c:288-293`),
  which returns `firepactive`/`fire2pactive`.
* `input_advance_fire_state()` copies flag → active once per *frame* in
  `syncframe` (`digger.c:280`), i.e. before the next tick's `dodigger`.

So fire sampled/recorded on tick N takes effect on tick N+1. This one-tick
delay is part of the recorded behaviour and must be reproduced exactly for
both DRF and eDRF playback. `clearfire(n)` (`input.c:247-253`, called on
respawn and death, `digger.c:842,850`) only clears the sticky key flag.
`input_reset_directions` clears flag and active at round start.

### 1.8 Joystick

Not implemented. `readjoy()` is empty and `detectjoy()` forces
`joyflag=false` (`input.c:159-167`); `joyflag` is never set true anywhere, so
the joystick branches in `readdirect` (`input.c:465-481`), `teststart`
(`input.c:486-532`) and `getdirect` (`input.c:539-551`) are dead code. Drop
them in the port.

### 1.9 Title start, pause, any-key

* `teststart()` (`input.c:486-532`) returns and clears the `start` latch.
* Pause: `testpause()` (`main.c:806-819`) runs after each tick; local game →
  `run_pause` shows `PRESS ANY KEY` and blocks in `getkey(true)`
  (`main.c:840-841`); NetSim uses `input_consume_anykey()` (`input.c:199-207`).
* `input_snapshot_primary_controls()` (`input.c:180-197`) builds the
  `INPUT_CTRL_*` bitmask (UP 0x01, DOWN 0x02, RIGHT 0x04, LEFT 0x08,
  FIRE 0x10; `input.h:26-30`) from P1 keys for NetSim and clears the sticky
  flags.

### 1.10 Text entry (high-score initials)

`getinitials` / `getinitial` (`scores.c:364-430`):

* Three positions; each `getinitial` alternates two 40-iteration polling
  phases (cursor shown / blinking), each iteration `kbhit()` then
  `flashywait(15)`.
* Keys are read with `getkey(false)` = SDL **keysym truncated to int16**. In
  the first phase non-`isalnum` keys are skipped; in the second phase *any*
  key is returned unfiltered (`scores.c:417-431`).
* 8 (Backspace) or 127 (Delete) steps back one position
  (`scores.c:377-381`). Letters are stored as typed (lower case, e.g. the
  `*mmm` in `tests/data/sobodig.drf`).
* Quirk from the int16 truncation: non-character keysyms (`0x400000xx`) lose
  their high bits, so Right/Left/Down/Up arrive as 0x4F/0x50/0x51/0x52 =
  `O`,`P`,`Q`,`R` and pass `isalnum`.
* Afterwards `recputinit(scoreinit[0])` appends `*XXX\n` to the DRF buffer
  (`scores.c:395`, `record.c:525-528`).
* Skipped entirely for playbacks, NetSim replays, cheated or taken-over games
  (`scores.c:244-245`).

---

## 2. INI configuration (`ini.c`, `ini.h`, `def.h`)

### 2.1 File name and location (`def.h:78-104`)

| build | `ININAME` |
|---|---|
| FreeBSD + `_VGL` | `/var/games/digger/digger.rc` |
| UNIX (FreeBSD/Linux/macOS), not VGL, not Emscripten | `$HOME/.digger.rc` (`./.digger.rc` if `HOME` unset) |
| everything else: Windows (MinGW), Emscripten | `DIGGER.INI` in the current directory |

Related per-platform files (same pattern): scores `~/.digger.sco` /
`DIGGER.SCO` / `/var/games/digger/digger.sco` (`scores.c:55-75`), NetSim
friends `~/.digger.frd` / `DIGGER.FRD` (`netsim_friends.c:36-52`), log
`DIGGER.log` in cwd on non-UNIX non-debug builds, stderr otherwise
(`main.c:1241-1245`). On Emscripten the INI and score file are mirrored to
`localStorage["digger:<path>"]` as base64 (`ems_store.c:26-63`; restore at
`main.c:376`, save after every `WriteINIString`, `ini.c:35-41`). The Windows
installer ships empty `digger.ini`, `digger.sco`, `digger.log` next to the
exe (`pkg/windows/digger.iss`).

### 2.2 Format (re-implementation of Win32 Get/WritePrivateProfileString)

```
[Section]\r\n
Key=Value\r\n
Key=Value\r\n
\r\n                <- a blank line ends the section
```

Reader `GetINIString` (`ini.c:129-169`):

* Copies the default to `dest` first (skipped when `dest==def`, which is how
  `GetINIInt`/`GetINIBool` pass their formatted default).
* Missing file → silent return of the default.
* Lines are read with `fgets(s1, 80, fp)` — **lines longer than 79 bytes are
  split** — and cut at the first CR/LF/NUL (`sgets`, `ini.c:17-28`), so
  CRLF, LF and CR-only files all work.
* Section header match: whole line equals `[Section]`, case-insensitive
  (`stricmp`). Key match: line starts with `Key=`, case-insensitive
  (`strnicmp`). No whitespace trimming, no comments, value = rest of line.
* Within a matching section the scan continues **until an empty line**
  (not until the next `[`), so a section not terminated by a blank line
  bleeds into the following one, and keys after a blank line are not found.
* If the key is not found the reader runs into EOF and prints
  `GetINIString: read failed: <file>` on stderr for every such lookup
  (`ini.c:148-157`). This is normal noise; `do-test-netsim.sh:103` filters it.
* `GetINIInt` = `atol(value)` (`ini.c:171-177`).
* `GetINIBool` (`ini.c:187-197`): upper-cases the value; first char `T` →
  true; otherwise `atoi(value) != 0` (so `True`, `true`, `1` are true;
  `False`, `0`, garbage are false).

Writer `WriteINIString` (`ini.c:43-127`):

* Reads the whole file, then:
  * key found in section → replace that line;
  * section found, key missing → insert `Key=Value\r\n` right before the
    section's terminating blank line (or at EOF);
  * section missing → **prepend** `[Section]\r\nKey=Value\r\n\r\n` to the
    old content;
  * file missing or zero-length ("short read") → create with just that
    section.
* Always writes CRLF (`NEWL`, `ini.c:13`), file opened `"wb"`.
* Stack buffers are 80 bytes (`s1,s2,s3`).
* `WriteINIBool` writes `True`/`False`. `WriteINIInt` is declared in `ini.h`
  but the definition is misspelled `WriteINIIntconst` (`ini.c:179`) — neither
  is called anywhere.

### 2.3 Every key read or written

All reads are in `inir()` (`main.c:1235-1311`), executed before the command
line is parsed. The only writer in the code base is `redefkeyb`
(`keyboard.c:158`) → `[Keys]`. Nothing else is ever saved (speed changes,
sound toggles, game mode are not persisted).

| section | key | type | default | use |
|---|---|---|---|---|
| `Keys` | `Right`,`Up`,`Left`,`Down`,`Fire` | `a/b/c/d/e` | see §1.1 | P1 keys |
| `Keys` | `Right2`,`Up2`,`Left2`,`Down2`,`Fire2` | same | see §1.1 | P2 keys |
| `Keys` | `Cheat`,`Accel`,`Brake`,`Music`,`Sound`,`Exit`,`Pause`,`Mode Change`,`Save DRF`,`Redefine` | same | see §1.1 | function keys |
| `Game` | `GauntletTime` | int | 120 | `dgstate.gtime` (seconds) |
| `Game` | `Speed` | int | 80000 (`DEFAULT_FTIME`) | `dgstate.ftime`, µs per frame (12.5 Hz) |
| `Game` | `GauntletMode` | bool | false | `dgstate.gauntlet` |
| `Game` | `Players` | string | `1` | upper-cased; `2S…` → `diggers=2,nplayers=1`; else `nplayers=atoi`, clamped to 1 if not 1..2, `diggers=1` |
| `Game` | `UnlimitedLives` | bool | false | `dgstate.unlimlives` |
| `Game` | `StartLevel` | int | 1 | `dgstate.startlev` |
| `Sound` | `SoundOn` | bool | true | `soundflag` |
| `Sound` | `MusicOn` | bool | true | `musicflag` |
| `Sound` | `Rate` | int | 44100 | `sound_rate` (uint16) |
| `Sound` | `BufferSize` | int | 512 (`DEFAULT_BUFFER`) | `sound_length` |
| `Sound` | `SoundVolume` | int | 0 | `dx_sound_volume` (legacy DirectX; unused by SDL) |
| `Graphics` | `640x480` | bool | false | `use_640x480_fullscreen` |
| `Graphics` | `Async` | bool | true | `use_async_screen_updates` |
| `Graphics` | `CGA` | bool | false | switch draw API to CGA |

`dgstate.ftime` is not statically initialised (`game.c:12-17`), so the
`if (dgstate.ftime == 0)` guard at `main.c:1264` always reads `Speed`.

Note for determinism: `UnlimitedLives` and `StartLevel` from the user's INI
are live during `/E` playback of a DRF unless the header overrides
(`startlev` is reset to 1 and re-read from the header; `unlimlives` is *not*
reset — the "U" header flag is parsed and ignored, `record.c:161-162`). The
conformance harness must run with a clean `HOME` (the NetSim script does,
`do-test-netsim.sh:85`; `do-test-run.sh` does not).

---

## 3. Command line (parts relevant here)

`main()` (`main.c:370-390`): `inir()` → `playtakeftime=ftime` →
`playbacklast()` → `parsecmd()` → `maininit()` → `mainprog()`.

Options are `/X` or `-X`, case-insensitive, value after an optional `:`
(`BASE_OPTS "OUH?QM2CKVL:R:P:S:E:G:I:N:T:"`, `main.c:929`):

* `/S:n` → `ftime = n*2000` µs; `n==0` → `ftime = 1` = unthrottled
  (`main.c:1061-1071`). A bare all-digits word is the same; any other bare
  word is a level file name (`main.c:1177-1211`).
* `/R:name` → `recname()` (remember the save name; `.edrf` suffix selects
  eDRF, `record.c:364-368`).
* `/P:file` → play back, then continue to the title; `/E:file` → play back
  and exit with a status (`main.c:1023-1056`):
  * 1 — cannot open / not a recording (`playbad`), message on stderr;
  * 2 — `edrf_failed` (diverged or bad record);
  * 3 — stopped before the end (`edrf_stopped`, or `escape` without
    `playend`/`edrf_checked`/`edrf_truncated`/`playtaken`);
  * 4 — `edrf_truncated` (eDRF without its `Z`);
  * 0 — otherwise; with env `DIGGER_CI_RUN` set, first prints the result
    line (see §7.2).
  * Before exiting, `/R:name` + `/E:file` re-saves the re-recorded game
    (`main.c:1030-1032`) — this is the DRF→eDRF converter:
    `digger /R:game.edrf /E:game.drf`.
* `playbacklast()` (`main.c:944-962`) moves the last `/P`//`E` argument to
  the end of argv so options written after it still apply (unless `/O`).
* `/T:tick` → `playtakeat`: hand the game to the player after that tick
  (`main.c:1074-1075`, `edrf.c:705-707`). `/O` loops the command line.
  `/I:lev` start level, `/U` unlimited lives, `/G[:secs]` gauntlet (clamped
  to 3599, 0 → 120), `/2` two diggers, `/Q` no sound at all, `/M` no music,
  `/K[A]` redefine keys, `/C` CGA, `/F` fullscreen, `/X:xid` X11 embed,
  `/N:…` NetSim, `/L:file` level file (`.DLF` appended on retry; 2 bytes
  bonus score + 1200 bytes of maps, `main.c:877-909`).

---

## 4. DRF recording format (`record.c`)

Plain ASCII text. Two parts (`record.c:115-116`): a line-oriented header,
then a body in which line breaks are meaningless.

### 4.1 Grammar

```
drf      := header body
header   := "DRF" NL                      ; only the first 3 bytes are checked
            version NL                    ; e.g. "AJ DOS 19990506", "TD WIN 20020310",
                                          ;      "POK SDL 20110912", "MS MPL 20190212"
            mode NL
            bonus NL                      ; decimal, atoi -> bonusscore (20000 in all test files)
            80 * (maprow NL)              ; 8 level plans x 10 rows x 15 chars
mode     := "1" | "2" | ["M" digit] ["G" digits] ["U"] ["I" digits]
                                          ; "1"/"2" may also be followed by U / I
body     := round* ["EOG"]
round    := ["*" c c c]                   ; initials entered before this round's seed
            seed                          ; 8 hex digits, the round's RNG seed
            run*                          ; directions, see below
            "EOL"
run      := dirchar [count]               ; count = decimal 2..999, absent = 1
dirchar  := "s"|"r"|"u"|"l"|"d"           ; stop / right / up / left / down
          | "S"|"R"|"U"|"L"|"D"           ; same with fire pressed
trailer  := ["*" c c c]                   ; initials of the final high score
NL       := LF | CR LF
```

Header reading (`openplay`, `record.c:89-236`):

* Lines are read with an 80-byte `fgets` and stripped of `\n` / `\r\n`
  (`smart_fgets`, `record.c:45-64`). `tests/data/muu27700.drf` is CRLF.
* Line 1: exactly `eDRF 2` selects eDRF (same header, no maps, different
  body); otherwise must start with `DRF` (`record.c:123-126`).
* Line 2: `kludge = atol(line+7) <= 19981125` — the date is at byte offset 7
  (after `XX YYY `). `kludge` reproduces a monster bug of old versions
  (`monster.c:408-417`). No file in `tests/data` triggers it (oldest is
  `AJ DOS 19990130`).
* Line 3 (`record.c:137-164`): `1`/`2` → `nplayers` (alternating players);
  `M<d>` → `diggers=d` (simultaneous); `G<n>` → gauntlet with `gtime=n`
  seconds; `U` skipped; `I<n>` → `startlev=n`. Before parsing: gauntlet off,
  `startlev=nplayers=diggers=1`. The caller's values are restored after the
  playback (`record.c:223-228`).
* Maps: for each of 80 lines the first 15 bytes go to
  `dgstate.leveldat[n][y][x]`. The buffer is pre-filled with spaces but
  `fgets` writes a NUL after a short line, so a short row yields a NUL (not a
  space) at the cut-off column (`record.c:171-181`). Semantically "not a map
  letter = empty" (see `mapch`, `edrf.c:112-117`); the re-recorder
  reproduces short lines because `mprintf("%c", 0)` emits nothing.
  Map letters: `S`,`V`,`H`,`B`,`C`, space.
* Body: the rest of the file is slurped, **dropping every byte < 0x20**
  (`record.c:203-209`), into one string walked by `plp`.

### 4.2 Body semantics

* Seed: `playgetrand()` (`record.c:504-523`): if the next char is `*`, skip 4
  chars (initials); then consume exactly 8 chars, hex digits either case
  (other chars contribute 0). Written as `%08lX\n` (`record.c:433-438`).
* Runs: `playgetdir()` (`record.c:287-309`):

  ```
  if rlleft > 0:   apply rld; rlleft--
  else:
      if *plp in {'E','e'}: playend = true; escape = true; return   // EOL or EOG reached
      rld = *plp++
      while isdigit(*plp): rlleft = rlleft*10 + digit
      apply rld
      if rlleft > 0: rlleft--
  ```

  `makedir` (`record.c:270-285`): upper case → `fire=true`, else
  `fire=false`; `s/r/u/l/d` → NONE/RIGHT/UP/LEFT/DOWN; any other letter
  leaves `dir` unchanged.
* `EOL`: after a round the game calls `playskipeol()` → `plp += 3`
  (`main.c:273-275`, `record.c:497-502`). If instead `playgetdir` runs into
  the `E` (the playback outlived the recorded round, or the recording was
  ended with Exit and has `EOL` then `EOG`), the playback stops with
  `playend=true` → exit status 0 from `/E`.
* There is **no validation**: a desynchronised DRF playback just reads the
  stream at the wrong phase and ends with a different score. `rlleft` is a
  static that is never reset (`record.c:30`); `plb` is not NUL-terminated.
  A Rust port should bound-check and treat end-of-buffer like `E`.

### 4.3 What one direction record corresponds to

One `recputdir`/`playgetdir` per **`getdirect()` call**, not per tick:
`getdirect` is only called from `updatedigger` when the digger is alive,
visible and `bagtime==0` (`digger.c:348-360,588`). With two diggers the calls
of both diggers interleave in one stream in call order (digger 0 then 1
within a tick). So the DRF can only be decoded by running the game logic.

The fire bit stored is `firepflag` at the time of `getdirect` (i.e. the flag
that will become active on the *next* tick, §1.7).

### 4.4 Writer

* `recstart()` allocates `MAX_REC_BUFFER = 262144` bytes (`def.h:58`,
  `record.c:238-252`). `mprintf` appends; if `recp > MAX-80` it resets
  `recp=0` ("give up", `record.c:266-267`) — silent corruption for very long
  games.
* `recinit()` (`record.c:393-431`), called at every game start and by
  `openplay` (so a playback is itself re-recorded): `DRF\n`, version
  (`DIGGER_VERSION "MS MPL 20190212"`, or `AJ DOS 19981125` when `kludge`),
  mode line (`M%i` + optional `G%i` if `diggers>1`; else `G%i` if gauntlet;
  else `%i` nplayers; then `I%i` if `startlev>1`), `\n%i\n` bonus score,
  80 map rows; then `recstream()` opens the eDRF stream if applicable.
  `rechdr` marks the end of the bonus line.
* `recputrand(seed)` at each round start: `%08lX\n`, reset run state
  (`record.c:433-438`).
* `recputdir` run-length encoder (`record.c:344-361`): run of identical
  chars, max 999; `putrun` (`record.c:323-342`) emits `%c` or `%c%i`
  (count only if > 1) and wraps with `\n` once the column counter
  (`1 + digits`) reaches 60.
* `recputeol()` (`record.c:530-538`): flush run, newline if mid-line,
  `EOL\n`. `recputeog()` (`record.c:540-544`): `EOG\n` when the round ended
  by Exit. `recputinit` → `*abc\n`.
* File opened in text mode `"wt"` (CRLF on Windows).

### 4.5 Saving and naming (`recsavedrf`, `record.c:440-495`)

Triggered by: F8 on the title screen if a game was played (`main.c:515-522`);
automatically after every game when `/R` gave a name (`main.c:591-595`);
after `/E` when `/R` was given (`main.c:1030-1032`).

* Name ends in `.edrf` → already streamed; if `!drfvalid` (cheated) the file
  is removed; return.
* `!drfvalid` → nothing saved.
* `/R:name` → that file; if it cannot be opened fall through to auto naming.
* Auto name: two alternating players → `DIGGER.DRF`. Otherwise from
  `scoreinit[0]` (non-letters → `_`) and `scoret`:
  `score < 100000` → `<3 initials><score>.drf`; else two initials + score,
  dropping the `_` one (third if `init[2]=='_'`, first if `init[0]=='_'`,
  else the middle one) — 8.3-friendly names such as `k1859075.drf`.
  Written in the current directory.

### 4.6 RNG seed, tick, and determinism

* A "round" = one iteration of the inner loop of `game()`
  (`main.c:160-306`): each life / each new level start draws a new seed.
  Playback: `randv = playgetrand()`; live: `roundseed(sbase, round++)` =
  low 32 bits of `splitmix64(base, round)` with
  `base = time ^ clock<<32 ^ ngames<<48` (or the NetSim session nonce)
  (`main.c:104-128`, `digger_math.h:69-79`). The seed is always re-recorded
  (`main.c:175`).
* RNG: `randv = randv*0x15a4e35 + 1; return (randv & 0x7fffffff) % n`
  on `int32_t` (`main.c:1222-1226`) — wrapping 32-bit arithmetic.
* `dgstate.ticks` (`game.h:15-17`) is the game's clock: reset in `game()`
  (`main.c:138`), incremented by `game_tick()` (`game.c:100-107`) at the end
  of every iteration of the play loop and of the post-round cleanup loop
  (`main.c:211-256`). Frames without a tick (player flash, pauses, redraw
  delays, title) do not count.
* Why playback is deterministic:
  1. seeds come from the file; inputs come from the file (live keys are
     zeroed during DRF playback: `edrf_input` returns 0 when `playing`,
     `edrf.c:526-527`, then `playgetdir` supplies dir+fire);
  2. level maps, bonus score, mode, start level come from the header;
  3. game logic advances only in ticks; nothing reads wall-clock time. The
     gauntlet clock subtracts the constant `DEFAULT_FTIME` per tick whatever
     the real speed (`digger.c:334-341`), starting from `gtime*1193181`
     (`main.c:140-143`);
  4. the one sound-coupled wait (death dirge) is skipped when `playing`
     (`digger.c:219-236`); the `deathffwd` fast-forward only skips sleeping
     (`digger.c:260-263,791-801`);
  5. `penalty`/`incpenalty` are pure counters;
  6. the `'H'` map patch for two-digger levels 3/4 in `getlevch`
     (`main.c:73-84`) applies for DRF playback and is disabled for eDRF
     (whose maps already contain it).
  Not from the file: `unlimlives` (INI/`/U`), and the high-score table is
  not touched during playback.
* Take-over (`T` key or `/T:tick`, `record.c:78-87`): `playing=false`,
  `playtaken=true`, eDRF playback off, speed back to the INI/`/S` value;
  recording continues with live input; no high score.

---

## 5. eDRF — extended recording format (`edrf.c`, `edrf.h`, `edrf_feed.c`)

### 5.1 What it is and how it differs from DRF

"eDRF 2" (`EDRF_MAGIC`, `edrf.h:19`). Design comment: `edrf.h:11-17`,
`edrf.c:4-34`.

| | DRF | eDRF |
|---|---|---|
| what is recorded | direction+fire per `getdirect()` call | raw control bits per input slot per game tick |
| where it is injected on playback | after direction resolution (`playgetdir`) | before it (`edrf_controls` in `readdirect`) — the static/dynamic logic of §1.6 runs on playback |
| maps | 8 plans in the header | inline `L` records as levels start (de-duplicated) |
| self-checking | none | state-hash checkpoints every 256 ticks, at round end, at game end |
| writing | buffered, saved at the end | streamed line by line, flushed every 32 ticks; a killed game leaves a playable prefix |
| quit | `EOG` | `Q <slot>` (who quit) |
| needs game logic to decode | yes | no (tick-indexed) |
| line breaks | insignificant in body | one record per line |

### 5.2 Full grammar

```
edrf     := "eDRF 2" NL version NL mode NL bonus NL record*
            ; header lines 2-4 exactly as in DRF (the writer copies them from the
            ; DRF buffer, record.c:386-390); no maps
record   := input | level | seed | ckpt | rend | quit | end | blank
input    := "I" slots (" " runs)+ NL
slots    := "" | "0" | "1" | "01"          ; strictly ascending digits < EDRF_SLOTS (2)
runs     := (ctl [count])+                 ; ctl = one char '@'..'_' = '@' + bits (0..31)
                                           ; count = decimal repeat, absent = 1
level    := "L " level NL 10*(" " row NL)  ; new map follows: 10 rows of up to 15 chars
          | "L " level " " n NL            ; reuse the n-th (1-based) distinct map seen so far
seed     := "R " hex8 NL                   ; RNG seed of the round starting
ckpt     := "C " hex8 NL                   ; game_state_hash() when ticks % 256 == 0
rend     := "E " hex8 NL                   ; game_state_hash_tick(ticks) at the end of a round
quit     := "Q " slot NL                   ; game quit by that slot's player; before Z
end      := "Z " ticks " " hex8 NL         ; decimal tick count and game_state_hash() at game end
```

Control bits (`input.h:26-30`): UP 1, DOWN 2, RIGHT 4, LEFT 8, FIRE 16 →
`@` none, `A` up, `B` down, `D` right, `H` left, `P` fire, `Q` fire+up, …

`I` record rules (`flushin`/`putruns`/`allsame`, `edrf.c:208-279`;
`load_slots`/`load_runs`, `edrf.c:320-386`):

* An `I` record covers the ticks since the previous record; at most
  `EDRF_CHUNK = 32` (flushed when `ticks % 32 == 0` and before every other
  record).
* No slot list (`I @8D13A5@3P2@`): one run list that applies to **every**
  slot. Written when all slots agree on every tick, where a slot that was
  not read on a tick ("UNREAD", 0x80) agrees with anything. This is the
  normal form for one-digger games (slot 1 is never read).
* With a slot list (`I0 …`, `I1 …`, `I01 … …`): one run list per listed slot
  in order; unlisted slots had no controls for the same number of ticks
  (the reader pads them with zeros to the length of the first listed slot).
  A slot is listed if it is not idle (or if the per-slot lengths differ).
* Writer detail: in per-slot lists runs are split on raw value, and UNREAD
  (0x80) and 0 are different raw values that both print as `@`, so adjacent
  `@n@m` runs can appear. The reader does not care.
* Reader: each `ctl` must be in `'@'..'_'`; a field ends at space or EOL;
  a field must be non-empty; trailing garbage → bad record.

Other reader rules (`readrec`, `edrf.c:390-482`):

* Lines up to 511 bytes; CR/LF stripped. A last line without `\n` is treated
  as end-of-file (recording cut short while being written).
* Blank lines are skipped, except inside a map where an empty line is a row
  of 15 spaces. Map rows must start with a space unless empty; rows longer
  than 15 → bad; short rows are space-padded.
* `C`/`E`: one hex number and nothing else; `Q`: one decimal < 2;
  `Z`: decimal and hex; `L`: one or two decimals (`sscanf`).
* Any other line → `eDRF: bad record "<line>"` on stderr, `edrf_failed`.

The C reader is lazy (reads on demand into per-kind FIFO queues: inputs per
slot, seeds, checkpoints, round ends, levels). Because records are in game
order, an eager parse into the same queues is equivalent, except that a bad
record then fails before play starts rather than when reached.

### 5.3 Recording hooks

* `edrf_input(slot, bits)` (`edrf.c:514-530`) from `readdirect` via
  `edrf_controls` (`input.c:350-364`): marks the slot read this tick; when
  playing an eDRF returns the recorded bits instead; when playing a DRF
  returns 0 (placeholder); always pushes `bits & 0x1f` to the chunk.
* `edrf_movement(slot, dir, fire)` (`edrf.c:533-549`) from `getdirect`: only
  while playing a **DRF** — overwrites the placeholder just pushed with the
  bit for the direction the DRF supplied (+FIRE). This is what makes
  `/R:x.edrf /E:x.drf` conversion work; ticks without a `getdirect` call
  stay 0.
* `edrf_tick()` (`edrf.c:681-708`) from `game_tick()` after `ticks++`:
  slots not read this tick get UNREAD pushed (and, on playback, their
  recorded value is consumed and discarded); every 32 ticks flush; every
  256 ticks hash → write `C`, verify against the next recorded `C`;
  `/T` take-over check.
* `edrf_level(level, plan)` (`edrf.c:608-642`) from `initlevel()`
  (`main.c:733-745`, only when `curplayer < nplayers`): playback pops the
  next level record, checks the level number
  (`eDRF: level N starts, recorded level M` → failed) and copies the 150 map
  bytes into `leveldat[plan-1]`; recording writes `L level` + 10 rows
  (`" %.15s"`, built with `getlevch` + `mapch`) or `L level n` if that exact
  map was already written.
* `edrf_putrand`/`edrf_getrand` (`edrf.c:644-662`): `R %08X`.
* `edrf_roundend()` (`edrf.c:711-724`) from `recputeol`: `E` with
  `game_state_hash_tick(ticks)`; verified unless the playback was stopped by
  the user before the recording was exhausted.
* `edrf_quit()` (`edrf.c:588-602`) from `recputeog`: `Q slot` (0 locally, the
  NetSim quitter, or the recorded one when a playback ran out).
* `edrf_gameend()` (`edrf.c:726-764`) from the end of `game()`: writes
  `Z ticks hash`, closes the file, then classifies the playback:
  not exhausted → `edrf_stopped`; exhausted but no `Z` → `edrf_truncated`;
  tick or hash mismatch → `edrf_failed`
  ("the game ends differently from the recording"); else `edrf_checked`.
* Playback end: every frame `syncframe` asks `edrf_quitting()`
  (`digger.c:268-269`, `edrf.c:553-584`): when all input queues are empty and
  the file has no more records, `escape=true`. So a recording with `Q`, or a
  truncated one, ends "as quit" exactly there.
* Stream open (`recstream`, `record.c:371-391`): only with `/R:<name>.edrf`;
  refuses to record over the file being played back.

### 5.4 State hash (what checkpoints compare)

`state_hash.c`: MD5 over a fixed sequence of state variables, each fed at its
native C size in **little-endian** byte order (byte-swapped on big-endian
hosts, `state_hash.c:22-90`); the result is the first 4 digest bytes as a
big-endian u32 (`state_hash.c:100-109`), printed `%08X`.

`game_state_append` order (`state_hash.c:156-175`): `field[MSIZE]`; digger
block (`digger.c:164-210`); monster block (`monster.c:125-163`); bag block
(`bags.c:96-117`); `dgstate.randv` (i32); `gettscore(0)`, `gettscore(1)`
(i32 each); and in gauntlet mode `cgtime`, `timeout`. `E` additionally
appends the u32 tick. **A Rust port must reproduce every field's width
(`int16_t` vs `int` vs `bool` = 1 byte) and order to pass eDRF playback** —
the exact struct types belong to the game-logic reports; treat those three
`*_debug_hash_append` functions as part of the file format.

### 5.5 "Diverging"

A playback has diverged when a hash it computes differs from the recorded
one at the same point. Messages (all stderr, all set `edrf_failed` → `/E`
exit status 2 after `eDRF: playback of <file> FAILED`):

* `eDRF: the game diverges from the recording before tick N (tick state X,
  recorded Y)` — `C` mismatch (`edrf.c:666-679`); "before" because the
  checkpoint only bounds the divergence to the last 256 ticks.
* same with `end of round state` — `E` mismatch.
* `eDRF: the game ends differently from the recording (expected T/H, got
  T/H)` — `Z` mismatch.
* `eDRF: level N starts, recorded level M`.
* `eDRF: bad record "..."`.
* NetSim feed only: `eDRF: player P controls XX on tick N, recorded YY`.

After the first failure further checks are skipped; the game keeps running
on the recorded inputs.

### 5.6 Feeding over NetSim (`edrf_feed.c`, `DIGGER_INSTRUMENTATION` only)

Test facility: two instrumented processes each open the same two-digger
eDRF (`DIGGER_NETSIM_REPLAY=<file>`, `main.c:401-415`).
`edrf_netfeed_open` (`edrf_feed.c:31-70`) accepts only mode `M2` /
`M2I<n>`, sets up the game from the header and opens the body with
`edrf_playopen` with `edrf_feeding=true` (not `edrf_playing`). Then:

* each peer transmits its own slot's next recorded controls instead of the
  keyboard (`edrf_feedpeek` → `edrf_peekinput`, non-consuming;
  `netsim_game.c:77-80`);
* in `edrf_input` the controls that actually arrived for both slots are
  compared with the recording (`edrf_feed_checkinput`, `edrf_feed.c:114-124`);
* seeds come from the recording (`main.c:165-168`); `C`/`E`/`Z` are verified
  as in local playback;
* a recorded `Q <slot>` is initiated only by the peer that owns that slot
  (`edrf_feed_quitting`, `edrf_feed.c:127-132`);
* `DIGGER_NETSIM_REPLAY_START` makes that peer start the game by itself;
  `DIGGER_NETSIM_REPLAY_REPEAT=n` replays the file n times in successive
  sessions (`edrf_feed.c:79-95`);
* exit status: 0 ok, 2 failed, 3 stopped/no session; the result line is
  printed to stdout (`main.c:561-585`).
* Without instrumentation `DIGGER_NETSIM_REPLAY` is a fatal error
  (`main.c:416-421`).

---

## 6. Timers and the frame/tick contract

### 6.1 API (`hardware.h`)

`inittimer()`, `gethrt(bool minsleep, int mult)`, `getkips()`, and the DOS
PIT stubs `s0soundoff, s0setspkrt2, s0settimer0, s0timer0, s0settimer2,
s0timer2, s0soundkillglob` — all empty on SDL (`sdl_timer.c:96-128`; on
FreeBSD VGL they print a FIXME, `fbsd_timer.c:102-135`). `getkips()` returns
1 (SDL) / 0 (VGL); `calibrate()` turns that into `volume=1`
(`main.c:870-875`). Nothing to port except `inittimer`/`gethrt`.

### 6.2 `gethrt` (SDL, `sdl_timer.c:54-89`)

`gethrt` is "wait until the next frame is due, presenting the screen on the
way". It is a software PLL rather than a fixed sleep:

```
if ftime <= 1:                       // /S:0 -> unthrottled
    doscreenupdate(false); if minsleep: SDL_Delay(10); return
tfreq   = mult * 1e6 / ftime         // frames per second
clk     = SDL_GetTicks()[ms] * tfreq / 1000  (+ netsim_remote_lead_ms * tfreq / 1000)
eval    = PFD_get_error(&pfd, clk)   // phase error in frames, 0 on the first call
filt    = eval != 0 ? recfilter_apply(lp, sigmoid(eval)) : recfilter_getlast(lp)
delay   = round(1000 * (1/tfreq) * (1 + filt))   // ms
doscreenupdate(false); SDL_Delay(delay)
```

* `PFD_get_error` (`digger_math.c:45-68`): `next = trunc(t)+1`; first call
  sets `target = next`, returns 0; otherwise `err = target - t`;
  `target = (err > 0) ? next+1 : next`.
* `sigmoid(x) = x/(1+|x|)`; `recfilter` is a one-pole low-pass
  `y = a*x + b*y`, `b = exp(-2π·Fc/Fs)`, `a = 1-b`, created by `inittimer()`
  with `Fs = 1e6/ftime`, `Fc = 0.1 Hz` (`sdl_timer.c:42-52`,
  `digger_math.c:108-126`); it aborts if `Fs < 0.2`. `inittimer` is called
  from the sound set-up functions (`sound.c:734`, `newsnd.c:81`), again after
  initials entry, and is not re-run when the speed changes.
* On Emscripten `SDL_Delay` is `emscripten_sleep` (Asyncify,
  `sdl_timer.c:9-13`).
* `DIGGER_GETHRT_DEBUG` (debug builds) logs every call.
* FreeBSD VGL version (`fbsd_timer.c:64-94`): same loop on
  `clock_gettime(CLOCK_UPTIME_PRECISE)` and `usleep`; no `ftime<=1`
  shortcut; calls `VGLCheckSwitch()`.

`mult`: 1 for a normal frame; 3 for pause/freeze/redraw-delay frames
(`digger.c:265,310`) — those poll three times per frame period.
`minsleep=true` is used by blocking waits (`getkey`, `flashywait`, end of
pause) so that an unthrottled game still yields 10 ms per poll.

### 6.3 Contract

* One frame = `newframe()` → `syncframe` (`digger.c:213-217,254-283`):
  `gethrt` (skipped when `deathffwd`), `checkkeyb()`, eDRF end-of-recording
  check, `frame++`, `input_advance_fire_state()`, NetSim frame exchange.
* One game tick = `newframe()` + `dodigger/domonsters/dobags` + `game_tick()`
  (`main.c:211-234`). Ticks ⊂ frames.
* Speed: `ftime` µs per frame, default 80000 (12.5 Hz); `/S:n` = `n*2000`;
  Accel/Brake keys ∓10000 with a floor just above 10000. Speed never changes
  game results.
* The timer source is irrelevant to determinism; a port may replace the PLL
  with any pacer (e.g. absolute deadlines) as long as (a) `ftime<=1` means
  "do not sleep", (b) the screen is presented once per `gethrt`, (c) the
  NetSim lead term can bias it.

---

## 7. Tests

### 7.1 Layout

`tests/data` — 50 recordings, flat directory: 23 `*.drf` + 27 `*.edrf`.

* Every `.drf` has an `.edrf` twin with the same base name (converted with
  `/R:x.edrf /E:x.drf`; twins have version `MS MPL 20190212` and the DRF's
  seeds, often `00000000`).
* eDRF-only: `2riptest01` (M2, ends with `Q 0` at tick 1024), `2test02`,
  `2test03` (M2), `gauntlet01` (`G120`).
* Modes present: `1` (one player), `M2` (two diggers: `2test01` DRF+eDRF and
  the three above), `G120` (gauntlet, eDRF only). No alternating two-player
  (`2`) recording, no `I` start level, no `kludge`-era file.
* Recorder versions in DRFs: `AJ DOS 19990130/19990506`, `TD WIN 20020310`,
  `POK SDL 20110912`, `MS MPL 20190212`. One CRLF file (`muu27700.drf`).
* Sizes: 759 bytes … 240 KB. Small set (≤ 15 KB by the DRF's size):
  `2riptest01, sobodig, dror0, gauntlet01, 2test03, thevoid, ramon,
  muu27700, 2test01, 2test02, thevoids`. The 16 big ones are million-point
  games (110k–565k ticks, levels 117–353).
* `extras/good_games/thevoids.drf` is byte-identical to
  `tests/data/thevoids.drf`.

`tests/results` — 27 files `<base>.out`, one per base name, shared by the
DRF and the eDRF. Each is exactly one line:

```
score=3100 level=2 ticks=1378                         (one-player games)
score=18100 score2=1100 level=4 ticks=4489            (two-digger games)
```

Range: `dror0` score 0 / 2947 ticks … `r3333300` score 3333300, level 353,
565330 ticks.

### 7.2 What the binary prints

`game_dbg_info_emit()` (`main.c:95-106`): `score` = `gettscore(0)`,
`score2` = `gettscore(1)` (only when the last game had `nplayers==2` or
`diggers==2`), `level` = current player's level number at the end, `ticks` =
`dgstate.ticks`. Printed on stdout by `/E` just before `exit(0)` when
`DIGGER_CI_RUN` is set (`main.c:1053-1054`). `DIGGER_CI_RUN_DTL` additionally
prints the same line at every completed level (`main.c:278-282`; not used by
the scripts).

### 7.3 `scripts/do-test-run.sh`

For each type in `TEST_TYPES` (default `quick short long xlong netsim`,
space or comma separated) and each `tests/data/*.drf` and `*.edrf`:

| type | files | command line | notes |
|---|---|---|---|
| `quick` | size ≤ 15 KB | `/Q /S:0 /E:<file>` | no sound, unthrottled |
| `short` | all 50 | `/Q /S:0 /E:<file>` | |
| `long` | size ≤ 15 KB | `/S:0 /E:<file>` | sound code runs (dummy driver) |
| `xlong` | size ≤ 15 KB and `ticks` ≤ 8000 | `/E:<file>`, or `/S:20 /E:<file>` if ticks > 7000 | real time (80 ms/tick; 40 ms with `/S:20`); all started at once |

* Size = file length rounded up to KB; an `.edrf` with a twin uses the
  `.drf`'s size so both land in the same types (`do-test-run.sh:83-93`).
  `ticks` is read by `eval`-ing the result file (`:106`).
* Environment per run: `SDL_AUDIODRIVER=dummy SDL_VIDEODRIVER=dummy
  DIGGER_CI_RUN=1` (`:24-25`). `HOME` is not overridden.
* Pass criteria (`run_test`, `:23-38`): exit status 0, and stdout (CRs
  stripped) is `diff -u`-identical to `tests/results/<base>.out`. On failure
  the last 20 lines of stderr are shown.
* For a DRF this checks final score(s), level and tick count only. For an
  eDRF, status 0 additionally implies every `C` (each 256 ticks), `E` and the
  final `Z` hash matched.
* Parallelism: `TEST_JOBS` or the CPU count; outputs go to
  `<type>-<file>.out/.err` in the cwd (git-ignored).
* Binary: `DIGGER_BIN`, else `./production/digger` (moved to `./`), else
  `digger` on PATH. `NETSIM_DIGGER_BIN` optionally selects a different
  (instrumented) binary for the NetSim part.
* After the playbacks: `netsim` type → `scripts/do-test-netsim.sh`; with
  `CI_COVERAGE` → `lcov --capture` into `digger_lcov/digger.info` using
  `scripts/gen-test-coverage.sh` (`gcov` or `llvm-cov gcov`) as gcov tool.

Wrappers: `make do-test` (GNU make builds, also cross-builds MinGW 32/64
unless `CC=clang`, `scripts/do-test.sh`), `make do-test-cmake`
(`scripts/do-test-cmake.sh`: Debug into `debug/`, Release into
`production/`, both with `-DDIGGER_INSTRUMENTATION=ON
-DDIGGER_WARNINGS_AS_ERRORS=ON`), `TEST_TYPE=…` → `TEST_TYPES`
(`GNUmakefile:247-262`). Only the production binary is executed; the debug
build is compile-only. `scripts/do-test-dpkgs.sh` runs the same against the
installed packages.

### 7.4 `scripts/do-test-netsim.sh` (summary; internals are another report)

For every `tests/data/*.edrf` whose mode line matches `M2(I<n>)?` (4 files):
two processes "alice" (`/N:alice-bob@:PORT`, player 2) and "bob"
(`/N:bob@127.0.0.1:PORT-alice`, player 1, `DIGGER_NETSIM_REPLAY_START=1`)
replay the file over localhost with `/Q /S:$NETSIM_SPEED`, each in its own
temp `HOME`/cwd, with a watchdog. Each must exit 0 and print the expected
result line, and be the expected player. Variants: packet loss none /
`DIGGER_NETSIM_RX_DROP_EVERY=7` / `DIGGER_NETSIM_TX_DROP_EVERY=5`; derived
recordings cut at a checkpoint with `Q 0` / `Q 1` appended (made with
`/R:quitN.edrf /E:cut.edrf`, expected result taken from playing that back);
quit by both; 50 games in a row (`DIGGER_NETSIM_REPLAY_REPEAT`); missing ACK;
answer lost twice; caller vanishing. Tunables: `NETSIM_TIMEOUT` (120),
`NETSIM_SPEED` (0), `NETSIM_TAIL`, `NETSIM_KEEP`, `NETSIM_REPEAT` (50),
`NETSIM_REPEAT_REC`, `PLATFORM`/`TARGETPLATFORM` (port derived from its MD5,
1024..9999), `DIGGER_NETSIM_SYNC_TIMEOUT`.

The `quit_rec` helper is also a useful local eDRF test: a truncated eDRF
plays to status 4, and re-recording it yields a valid shorter recording.

### 7.5 Environment variables seen in this area

`DIGGER_CI_RUN`, `DIGGER_CI_RUN_DTL` (result line); `SDL_VIDEODRIVER`,
`SDL_AUDIODRIVER` (headless SDL); `HOME` (INI/score/friends location);
`DIGGER_GETHRT_DEBUG`, `DIGGER_MONSTER_DEBUG`, `DIGGER_LOG_T0` (logging);
`DIGGER_NETSIM_REPLAY`, `_REPLAY_START`, `_REPLAY_REPEAT`,
`_EXPECT_NO_ACK`, `_RX_DROP_EVERY`, `_TX_DROP_EVERY`, `_LOSS_SEED`,
`_SIP_DROP_2XX`, `_SIP_NO_ACK`, `_MUTE_AFTER_ACK`, `_SIP_TIMER_L`,
`_SYNC_TIMEOUT`, `_TRACE`, `_DEBUG`, `_PROTO_DEBUG` (NetSim);
build-time `CI_COVERAGE`, `CI_UBSAN`, `CC`, `TEST_TYPE(S)`, `TEST_JOBS`,
`DIGGER_BIN`, `NETSIM_DIGGER_BIN`.

### 7.6 Sanitizers and coverage

* Coverage: env `CI_COVERAGE` adds `--coverage` in both CMake
  (`CMakeLists.txt:50-54`) and GNU make (`GNUmakefile:99-102`); lcov →
  Coveralls (parallel upload per matrix cell).
* UBSan: env `CI_UBSAN` adds `-fsanitize=undefined
  -fno-sanitize-recover=all` (`CMakeLists.txt:44-48`); CI job runs
  `short,netsim` with gcc and clang.
* ASan (gcc, clang) and MSan (clang) via explicit `CMAKE_C_FLAGS`,
  RelWithDebInfo + instrumentation, `short,netsim`. MSan links static SDL2
  and zlib built with MSan by `scripts/build-msan-deps.sh` (pinned versions
  and SHA-256, dummy video/audio backends only);
  `MSAN_OPTIONS=halt_on_error=1:exit_code=86`.

---

## 8. Build, CI, packaging

### 8.1 Dependencies

* **SDL2** (not SDL3): distro `libsdl2-dev` on Linux, Homebrew `sdl2`
  (really sdl2-compat on SDL3) on macOS, pinned **2.32.10** MinGW dev
  tarball on Windows and for MSan (`scripts/dep-versions.sub`). Emscripten
  port via `-s USE_SDL=2`.
* **zlib** — pinned 1.3.2 for Windows/MSan, distro elsewhere,
  `-s USE_ZLIB=1` on wasm (compressed title bitmap, `title_gz.c`).
* X11 (optional; `/X` window embedding, auto-detected,
  `HAVE_SDL_X11_WINDOW`), pthreads, libatomic where C11 atomics need it
  (e.g. ARMv5), libm; ws2_32 on Windows.
* Vendored `microsippy/` (SIP stack + `external/mackron_md5`, the MD5 used by
  the state hash).
* Tooling: C11 with GNU extensions; CMake ≥ 3.10; GNU make; BSD make for
  `Makefile.fbsd`; Emscripten 6.0.11 in CI; Inno Setup 6; lcov/llvm.
  `apt-get-requirement.txt`: `cmake cmake-data libsdl2-dev lcov llvm
  zlib1g-dev`.

### 8.2 Build systems

**CMake** (`CMakeLists.txt`), single target `digger`:

* Options: `DIGGER_INSTRUMENTATION` (NetSim replay/fault injection; default
  ON only for `CMAKE_BUILD_TYPE=Debug`), `DIGGER_VGL` (FreeBSD console
  backend instead of SDL2; OFF), `DIGGER_WARNINGS_AS_ERRORS` (OFF),
  `DIGGER_LTO` (ON, not for Debug).
* Defines: `-DLINUX -D_REENTRANT`, `-D_SDL -D_SDL_SOUND` (or `-D_VGL`),
  `-DDIGGER_DEBUG` in Debug, `USIPY_BIGENDIAN=0/1`, `HAVE_CLOCK_NANOSLEEP`
  etc.
* Emscripten swaps `netsim.c`/`netsim_rx.c` for `netsim_stubs.c` and adds
  `ems_vid.c ems_kbd.c ems_store.c` (`CMakeLists.txt:232-268`).
* `scripts/gen-build-info.sh` generates `digger_version.h` (git revision →
  SIP branding) or `digger-build-info.js` for the web page.
* Installs `bin/digger` and `README.md`, `digger.txt` under
  `share/doc/digger`. `VERSION` is `20110912`.

**GNUmakefile**: `ARCH=LINUX|FREEBSD|MINGW|MINGW64|WASM|FooOS`,
`BUILD_TYPE=production` (`-O3`, strip, instrumentation OFF by default) or
anything else (`-O0 -g3 -DDIGGER_DEBUG`, instrumentation ON). Always adds
`-DNO_SND_FILTER` (CMake does not — the two builds differ in sound
filtering). WASM link flags: `-s ASYNCIFY -s
EXPORTED_RUNTIME_METHODS=UTF8ToString --emrun --shell-file shell.html`,
`-flto=full`, output `digger.html/.js/.wasm` + `digger-build-info.js`
copied to `web-dist/`. Targets: `all`, `clean`, `do-test`, `do-test-cmake`,
`coverage-report`, `coverage-report-html`.

**Makefile.fbsd**: BSD `bsd.prog.mk`; default is VGL (`-D_VGL`, `-lvgl`,
`fbsd_*.c`), `SDL=1` selects SDL, `SOUND=1` adds SDL audio to the VGL build.
Its source list lacks `netsim_friends.c` and the `netsim_sip*`/microsippy
files, so it looks stale relative to the other two.

Legacy: `.travis.yml` (Ubuntu xenial, gcc-5/clang, cpp-coveralls).

### 8.3 CI matrix (`.github/workflows`, `appveyor.yml`)

* `linux.yml`
  * `build`: {ubuntu-24.04, ubuntu-26.04} × {gcc, clang} × test type
    {short, long, xlong, netsim}; `CI_COVERAGE=1 make do-test-cmake`;
    Coveralls. Also nightly cron.
  * `ubsan`: ubuntu-24.04 × {gcc, clang}, `short,netsim`.
  * `memory_sanitizers`: ubuntu-26.04, GCC ASan / Clang ASan / Clang MSan,
    `short,netsim`.
  * Docker: `docker/Dockerfile.build` built with buildx+QEMU (clang, lld,
    ccache) for every platform of `debian:sid-slim`, `debian:13-slim`,
    `debian:12-slim`, `ubuntu:latest` (amd64, 386, arm/v5, arm/v7, arm64,
    ppc64le, riscv64, s390x, mips64le where available); then each platform
    is tested in its own container with `TEST_TYPES="quick netsim"`,
    `DIGGER_NETSIM_SYNC_TIMEOUT=5000`. This is what exercises big-endian
    (s390x) and 32-bit targets for the state hash.
* `windows.yml`: windows-latest, MSYS2 MINGW64, `scripts/do-build-windows.sh`
  (downloads SDL2/zlib, builds debug + production with GNU make
  `ARCH=MINGW64`, zips `digger.exe digger_debug.exe digger.txt SDL2.dll
  zlib1.dll` → `digger-win64.zip`), Inno Setup installer, then
  `TEST_TYPES='quick netsim'` with `MSYS2_ARG_CONV_EXCL='*'` (so `/E:file`
  is not path-mangled).
* `appveyor.yml`: same build + installer on the VS2022 image (no tests).
* `macos.yml`: macos-latest, Homebrew, CMake Release + instrumentation,
  `pkgbuild` → `digger-macos.pkg` (id `org.digger.digger`, binary in
  `/usr/local/bin`, bundled `libSDL2-2.0.0.dylib` and `libSDL3.dylib` in
  `/usr/local/lib/digger`), installs it after removing Homebrew libs and
  runs `quick netsim` under a hang watchdog.
* `wasm.yml`: emsdk, `make ARCH=WASM` debug + production, artifact
  `web-dist`, upload to S3 (`digger-build-artefacts`).
* `dpkgs.yml`: reusable `sippy/cimagic` Debian package workflow, tested with
  `scripts/do-test-dpkgs.sh`.
* `release.yml`: on `v*` tags, collects Linux/Windows/macOS artifacts into a
  draft release.

### 8.4 Packaging

* **Debian** (`debian/`): source `digger`, built twice with clang/lld,
  RelWithDebInfo: package `digger` (`/usr/bin/digger`, docs; no
  instrumentation) and `digger-instrumentation`
  (`/usr/lib/digger/digger-instrumented`). Build-Depends: cmake,
  debhelper-compat 13, libx11-dev, libsdl2-dev, pkg-config, zlib1g-dev.
* **Windows** (`pkg/windows/`): `digger.iss` (Inno Setup; installs
  `digger.exe`, `SDL2.dll`, `zlib1.dll`, README, LICENSE, plus empty
  world-writable `digger.log`, `digger.sco`, `digger.ini` in the app
  directory; Start-menu shortcuts for `/F`, `/2`, `/G`, `/K`, `/2 /K`),
  `digger.rc`/`digger.ico` (icon + version resource), `-Dmain=SDL_main`,
  `-mwindows` (`-mconsole` for debug).
* **WebAssembly** (`shell.html`): Emscripten shell page with a 640×400
  canvas, loading bar, fullscreen button (`Module._digger_request_fullscreen`),
  a CONTROLS panel rendered from `Module._digger_get_keymap()` JSON whenever
  the game calls `window.diggerKeymapChanged()` (`ems_kbd.c:76-115`), and a
  footer link built from `digger-build-info.js`. Settings and scores persist
  through localStorage (`ems_store.c`). Exit on the title does not quit
  (`main.c:616-624`).
* `resource.h` is a leftover Visual Studio resource-ID header from the old
  WinDig GUI (menus, dialogs: `ID_GAME_*`, `ID_RECORDING_SAVE/PLAY`,
  `IDD_DIALOG_*` …); not referenced by any current source.

---

## 9. Platform / backend inventory

| backend | files (this area) | status |
|---|---|---|
| SDL2 desktop (Linux, FreeBSD, macOS, Windows/MinGW) | `sdl_kbd.c`, `sdl_timer.c` (+ `sdl_vid.c`, `sdl_snd.c`) | the real one; scancode key table, 30-entry event FIFO, PLL timer |
| Emscripten / browser | SDL2 port + `ems_kbd.c` (keymap JSON for the page), `ems_store.c` (localStorage persistence), `ems_vid.c` (browser fullscreen), `netsim_stubs.c` | maintained; `SDL_Delay` → `emscripten_sleep`; INI is `DIGGER.INI` in MEMFS |
| FreeBSD console VGL | `fbsd_kbd.c`, `fbsd_timer.c` (+ `fbsd_vid.c`, `fbsd_snd.c`) | legacy; own key codes (scancode+128 or ASCII), keys are buffered on *release* (`fbsd_kbd.c:182-187`), Alt-Fn console switching with auto-pause, buggy `memcpy` byte counts; `/var/games/digger/*` files |
| common UNIX glue | `fbsd_sup.c` (`strupr`, signal `catcher`), `fbsd_sup.h` (`FIXME` macro) | used on all non-MSVC builds |
| DOS / Win32 GUI | only traces: `s0*` stubs, `resource.h`, `near/far/huge` macros, `INTDRF` debug hooks | dead |

A Rust port needs one desktop backend plus optionally wasm; VGL can be
dropped.

---

## 10. Suggested Rust design for this area

### 10.1 Crate layout

```
digger-core     no I/O, no SDL: game state, tick(), state hash, DRF/eDRF codecs, INI codec,
                input resolver.  #![forbid(unsafe_code)], no_std-friendly if cheap
digger-sdl      SDL2 front end (video, audio, keyboard, pacer), INI/score file locations
digger-cli      argument parsing (/X and -X forms), exit codes, result line
xtask or tests/ conformance harness over tests/data
```

Cargo features: `sdl` (default), `netsim`, `instrumentation` (implies
`netsim`; replay-over-network + fault injection), `wasm` (target-gated
storage/keymap glue), `cga`. A `headless` build (no `sdl` feature) must still
run `/E` so the conformance suite needs no SDL at all.

### 10.2 Input abstraction

```rust
bitflags! { struct Ctl: u8 { UP=1; DOWN=2; RIGHT=4; LEFT=8; FIRE=16; } }

pub trait Keyboard {                       // backend
    fn poll(&mut self);                    // pump events
    fn is_down(&self, code: i32) -> bool;  // async state, SDL scancode numbering
    fn pop_key(&mut self) -> Option<KeyEvent>;   // {scancode: i16, sym: i16}, 30-entry FIFO
}

pub struct KeyMap { pub codes: [[i32; 5]; 20] }  // same shape as keycodes[][]

pub struct SlotResolver { old: [bool;4], stat: Dir, dynm: Dir }   // §1.6, one per source
pub struct Input {
    sticky: [bool; 10], fire_flag: [bool;2], fire_active: [bool;2],
    keydir: [Dir;2], sources: [Source;2], net_bits: [u8;2], ...
}
impl Input {
    fn check_keyb(&mut self, kb, game_flags) ;          // §1.5
    fn read_direct(&mut self, slot, rec: &mut Recorder);// §1.6 incl. the edrf hook
    fn get_direct(&mut self, slot, rec) -> Dir;         // DRF hook
    fn advance_fire(&mut self); fn reset_directions(&mut self); fn flush(&mut self, kb);
}
```

Keep one resolver implementation and instantiate it three times (the C code
has three hand-copied variants with identical logic). Keep the exact edge
order (U, D, L, R) and the one-tick `static` pulse. Keep the one-tick fire
latch as an explicit `advance_fire()` called once per frame. Use SDL scancode
numbers in the key map so existing `.digger.rc` files load unchanged.
Decide deliberately whether to keep the quirks (function-key `k` repeat,
int16 keysym truncation in initials, any key starts the game); none of them
affect recordings.

### 10.3 INI

Hand-written, ~150 lines, operating on `Vec<u8>`:

* `get(section, key) -> Option<&[u8]>` with the exact rules of §2.2 (case
  insensitive ASCII, section ends at a blank line, 79-byte line split if
  strict bug-compatibility is wanted — otherwise read whole lines; document
  the choice).
* `set(section, key, value)` with the three placements (replace / insert
  before the blank line / prepend new section) and CRLF output, so files
  stay interchangeable with the C game.
* Typed helpers: `get_int` (C `atol` semantics: leading whitespace, optional
  sign, stop at first non-digit, 0 on garbage), `get_bool` (first char
  `T`/`t`, else atoi ≠ 0).
* Do not print the `read failed` noise.
* Path resolution per §2.1 behind a small `Paths` trait (wasm: localStorage
  with the same `digger:<path>` base64 scheme if migration matters).

### 10.4 DRF / eDRF

No serde. Two small modules with byte-level parsers and writers:

```rust
pub struct Header { pub kind: Kind /*Drf|Edrf*/, pub version: String, pub kludge: bool,
                    pub nplayers: u8, pub diggers: u8, pub gauntlet: Option<u32>,
                    pub start_level: i16, pub bonus_score: i32,
                    pub maps: Option<[[[u8;15];10];8]> }

pub struct DrfPlayer { body: Vec<u8>, pos: usize, rl_left: u32, rl_char: u8 }
impl DrfPlayer { fn next_seed(&mut self) -> u32; fn next_dir(&mut self) -> Step /*Dir,fire | End*/;
                 fn skip_eol(&mut self); }
pub struct DrfWriter { buf: Vec<u8>, col: u32, run: Option<(u8,u32)> }   // byte-exact putrun/wrap

pub struct EdrfPlayer { inputs: [VecDeque<u8>;2], seeds, ckpts, round_ends, levels,
                        quit: Option<u8>, end: Option<(u32,u32)>, truncated_line: bool }
pub struct EdrfWriter<W: Write> { chunk: [Vec<u8>;2], maps: Vec<[u8;150]>, .. }
pub enum Outcome { Checked, Failed(Divergence), Stopped, Truncated }
```

* Mirror the C hook points one-to-one: `input(slot,bits)`,
  `movement(slot,dir,fire)`, `tick()`, `level()`, `put/get_rand`,
  `round_end`, `quit`, `game_end`.
* Parse the eDRF eagerly but keep "bad record at line N" as a deferred
  failure if identical exit-status timing is wanted.
* Make the writers byte-exact (`%08X` upper case, 60-column wrap rule,
  `I` slot-list selection, UNREAD vs 0 run splitting, 15-char map rows with
  a leading space, `\n` line ends) and add round-trip tests: play
  `x.drf` while recording eDRF and compare with the checked-in `x.edrf`
  byte for byte. (This was not verified here by running the C binary; if a
  twin does not match byte-for-byte, fall back to semantic comparison.)
* State hash: implement MD5 (the `md-5` crate or 100 lines by hand), feed
  fields with explicit `to_le_bytes()` at the C widths, take
  `u32::from_be_bytes(digest[0..4])`.

### 10.5 Pacer

`trait Pacer { fn wait_frame(&mut self, min_sleep: bool, mult: u32); }` with
(a) the PLL port (`PFD` + `recfilter` + sigmoid, ~40 lines) for the SDL
front end, (b) a `NoPacer` for headless runs. Keep `ftime` in µs and the
`/S:n → n*2000`, `0 → 1` mapping. Game logic must never read a clock.

### 10.6 Conformance harness

The core should expose something like

```rust
pub fn replay(recording: &[u8], opts: ReplayOpts) -> ReplayResult
// ReplayResult { score: [i32;2], two_players: bool, level: i32, ticks: u32, outcome: Outcome }
```

and a formatter producing exactly `score=… [score2=…] level=… ticks=…\n`.

* `tests/conformance.rs`: iterate `tests/data/*.{drf,edrf}`, compare the
  formatted line with `tests/results/<stem>.out`, and for eDRFs require
  `Outcome::Checked` (`2riptest01` included — it ends with `Q` then `Z`).
  Use `datatest-stable`/`libtest-mimic` or a build-script-generated
  `#[test]` per file so failures are per recording.
* Tiering like upstream: the 11 small bases by default (`quick`); the 16
  large ones behind `--ignored` or a `long-tests` feature (they are
  100k–565k ticks each; in pure Rust without rendering they should still be
  seconds, so they can probably all run in CI in release mode).
* eDRF first: its checkpoints localise a divergence to 256 ticks, which is
  the fastest way to debug the port. Add a debug mode that dumps the
  per-module hashes (`field_hash`, `digger_debug_hash`, `bags_debug_hash`,
  `monster_debug_hash`) per tick; the C binary can produce the same with
  `DIGGER_NETSIM_TRACE`/instrumentation for bisection.
* Keep CLI compatibility (`/Q /S:0 /E:file`, `DIGGER_CI_RUN=1`, exit codes
  0/1/2/3/4) so the upstream `scripts/do-test-run.sh` can be pointed at the
  Rust binary via `DIGGER_BIN` unchanged — the cheapest end-to-end check,
  and it also covers the `long`/`xlong` variants that exercise sound and the
  pacer.
* Extra unit tests: DRF→eDRF conversion vs checked-in twins; truncated eDRF
  → status 4; eDRF cut + `Q n` → re-record → status 0 (the `quit_rec`
  recipe); INI round trips against fixture files written by the C code;
  resolver table tests (simultaneous edges, release fallback order, tap
  pulse).
* Run with a temp `HOME` so a developer's `.digger.rc` cannot leak
  `UnlimitedLives`/`StartLevel` into results.

### 10.7 CI

GitHub Actions: `cargo fmt --check`, `clippy -D warnings`, `cargo test`
(headless core + quick conformance) on Linux/macOS/Windows; a release-mode
job for the full 50-recording suite; a big-endian/32-bit job via `cross`
(`s390x-unknown-linux-gnu`, `armv5te`, `i686`) to keep the hash portable;
`wasm32-unknown-emscripten` or `wasm32-unknown-unknown` build job; optional
Miri on the codecs; `cargo llvm-cov` for coverage; packaging via
`cargo-deb`, Inno Setup (reuse `pkg/windows/digger.iss`) and the existing
`shell.html` contract (`digger_get_keymap`, `diggerKeymapChanged`,
`digger_request_fullscreen`) if the web page is kept.

---

## 11. Trickiest parts and quirks to port

1. **DRF records are per `getdirect()` call, not per tick**, and interleave
   two diggers. The decoder is inseparable from the game logic: one missed
   or extra `updatedigger` call desynchronises the stream silently
   (wrong score, no error).
2. **Tick count is part of the expected output.** `ticks` counts both the
   play loop and the post-round cleanup loop (`t=20` countdown / moving
   bags, `main.c:238-256`) and nothing else; result files are shared by DRF
   and eDRF so both paths must produce identical tick counts.
3. **Injection point differs**: DRF replaces the *resolved* direction and
   fire flag; eDRF replaces the raw bits and then runs the static/dynamic
   resolver and the sticky-old-state. The resolver's state reset
   (`input_reset_directions` at each round) must match exactly.
4. **Fire is delayed one tick** through `firepflag` → `firepactive`, and in
   DRF playback the flag is written in `getdirect` (late in the tick) but in
   eDRF playback in `readdirect` (early). Both end up active on the next
   tick; keep the latch call in the per-frame path.
5. **State hash layout** (field order and C integer widths, little-endian,
   MD5 prefix) is a de-facto wire format for 27 eDRF files and for NetSim.
6. **eDRF unread-slot handling**: slots not read in a tick still consume one
   recorded value (`edrf_tick`), e.g. when `dodigger` returns early on
   gauntlet timeout or escape.
7. **End-of-recording logic**: exhaustion → `escape` on the next frame;
   `E` verification is skipped only when stopped early; classification into
   checked / failed / stopped / truncated and the 0/1/2/3/4 exit codes.
8. **Map bytes**: short DRF header rows give NULs; eDRF normalises with
   `mapch`; the `'H'` patch for two-digger levels 3/4 applies to DRF
   playback and live play but not eDRF playback.
9. **`kludge` (≤ 19981125) recordings** change monster behaviour; no test
   covers it, so it can only be ported by inspection.
10. **INI section termination by blank line** and prepend-on-new-section are
    unusual; a generic INI crate will not be file-compatible.
11. **Environment leakage**: `UnlimitedLives` from the user's INI affects
    playback; upstream's playback script does not isolate `HOME`.
12. **DRF buffer limits**: 256 KB recording buffer that silently wraps;
    unbounded reads past the end of the play buffer; static `rlleft` never
    reset between playbacks. Replace with bounded, per-playback state.
13. **Things safe to drop**: joystick code, `s0*` timer stubs, `getkips`
    calibration, VGL backend, `resource.h`, `INTDRF` hooks, `WriteINIInt`
    /`WriteINIBool`.
