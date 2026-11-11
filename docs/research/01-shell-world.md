# 01 — Game shell and world model (porting reference)

Source root: `https://github.com/sobomax/digger`
(all `file:line` references are relative to it). Files read in full: `main.c`, `main.h`, `game.c`,
`game.h`, `def.h`, `digger_types.h`, `digger_math.c/.h`, `scores.c/.h`, `state_hash.c/.h`,
`digger_log.c/.h`, `drawing.c/.h`, `digger.txt`. Neighbouring modules (`digger.c`, `monster.c`,
`bags.c`, `record.c`, `edrf.c`, `input.c`, `sdl_timer.c`) were read only where they touch shared
state; their internals belong to other reports.

Items I consider upstream bugs or surprises are marked **[QUIRK]** (kept deliberately / needed for
compatibility) or **[BUG?]** (looks accidental; port must decide).

---

## 1. Program startup

`main()` — `main.c:370-390`:

1. `digger_log_start()` — log time base (monotonic ns, or env `DIGGER_LOG_T0` seconds) (`digger_log.c:114-130`).
2. `inir()` — read INI settings (`main.c:1235-1311`), see 1.2.
3. `playtakeftime = dgstate.ftime` — the speed a taken-over playback drops back to (`main.c:380`, `record.c:85`).
4. `playbacklast(argc, argv)` — reorder argv (`main.c:958-976`), see 1.1.
5. `parsecmd(argc, argv)` — options are executed *in order*, including playing recordings in place (`main.c:978-1224`).
6. `maininit()` — idempotent (static guard): `initsounddev`, `calibrate`, `ginit`, `gpal(0)`, `initkeyb`, `detectjoy`, `initsound`, `recstart` (`main.c:349-368`). `/P` and `/E` call it early (`main.c:1026`).
7. `mainprog()` — title loop / games (`main.c:392-605`). Then flush/close log, return its value (always 0).

`calibrate()` (`main.c:872-877`): `volume = getkips()/291`, min 1. SDL `getkips()` returns 1
(`sdl_timer.c:91`), so `volume == 1` always. `volume` is only used by me in `flashywait` (scores.c:405).

`finish()` (`main.c:607-614`): `killsound, soundoff, soundkillglob, restorekeyb, graphicsoff`.

### 1.1 Command-line options

Option syntax: word starts with `/` or `-`; letter is case-insensitive (`getarg`, `main.c:918-936`).
Option table `BASE_OPTS "OUH?QM2CKVL:R:P:S:E:G:I:N:T:"` (+`X:` with X11, +`F` with SDL)
(`main.c:938-944`). A letter followed by `:` in the table "takes an argument": the argument starts at
`word[3]` if `word[2]==':'`, otherwise at `word[2]` (so `/S:40` and `/S40` both work; `main.c:999-1002`).

`playbacklast` (`main.c:958-976`): finds the last `/P` or `/E`; if an `/O` appears after any `/P|/E`,
does nothing; otherwise moves that last playback option to the end of argv so that later options
(`/Q`, `/S:0`…) apply to it.

| Opt | Semantics | Ref |
|---|---|---|
| `/S:n` | `speedmul` parsed as decimal into **int16** (no digit validation, wraps); `ftime = speedmul*2000` µs if >0 else `ftime = 1` (unpaced). Sets `gs=true`. Manual: default speed 40 (=80000 µs). | main.c:1061-1071 |
| bare number | If no speed seen yet (`gs` false) and word is all digits → same as `/S`. **[BUG?]** once `gs` is true, `sf` stays true for *every* later bare word, so a bare level-file name after a speed is parsed as a (garbage) speed. | main.c:1180-1202 |
| `/L:name` / bare non-number | Copy into `dgstate.levfname[132]` (unbounded copy), `levfflag=true`. Loaded after all args parsed (`read_levf`, main.c:1215-1223); on failure `levfflag=false` silently (exit(1) only under `DIGGER_DEBUG`). | main.c:1003-1009, 1203-1211 |
| `/C` | CGA graphics: swap all `ddap` function pointers to `cga*`, `ginit()`, `gpal(0)`. (`'B'` is tested too but is not in the option table, so `/B` is "unknown".) | main.c:1146-1159 |
| `/Q` | `soundflag=false` **and** `quiet=true` (sound device never opened in `initsounddev`). | main.c:1138, 1166, 331-347 |
| `/M` | `musicflag=false`. | main.c:1140 |
| `/R:name` | `recname(name)` — record to file (`.edrf` suffix → eDRF). | main.c:1023 |
| `/P:name` | `maininit(); openplay(name)` — plays the recording **immediately, during parsing**, then parsing continues. If playback aborted (`escape`), `norepf=true`. | main.c:1025-1030 |
| `/E:name` | As `/P`, then: if `/R` name given and a game was got → `recsavedrf()`; `finish()`; exit status **1** unreadable/bad file (message to stderr), **2** eDRF diverged (`edrf_failed`), **3** stopped before the end (`edrf_stopped || (escape && !playend && !edrf_checked && !edrf_truncated && !playtaken)`), **4** recording truncated, else **0**. With env `DIGGER_CI_RUN` set prints `score=%d level=%d ticks=%u` (or `score=%d score2=%d level=…` for two players) before exit 0. | main.c:1031-1056, 95-106 |
| `/O` | If `!norepf`: `arg=0; continue` → restart parsing from argv[1] (endless loop, screensaver use). | main.c:1057-1060 |
| `/K`, `/KA` | `redefkeyb(ddap, all)` immediately; `all` if `word[2]` is `A`/`a`. | main.c:1160-1165 |
| `/G[:time]` | `gtime` parsed decimal from arg; clamp `>3599 → 3599`; `0 → 120`; `gauntlet=true; netsim=false`. Note: resets `gtime` to 0 first, so INI `GauntletTime` is ignored when `/G` has no number. | main.c:1168-1178 |
| `/2` | `diggers=2; netsim=false` (does **not** touch `nplayers`). | main.c:1142-1145 |
| `/U` | `unlimlives=true`. | main.c:1086 |
| `/I:level` | `sscanf("%hi")` into `startlev` (int16; `%i` accepts 0x/octal; no range check). | main.c:1072 |
| `/T:tick` | `playtakeat = strtoul(...)` — take over playback after that game tick. | main.c:1074, edrf.c:706 |
| `/N:spec` | `netsim_configure(spec)`; failure → stderr + exit(1). Sets `gauntlet=false, netsim=true, nplayers=1, diggers=2`. | main.c:1076-1085 |
| `/F` | SDL fullscreen. `/X:xid` embed in X11 window (strtol base 0). | main.c:1010-1022 |
| `/V` | Accepted (in table) but has **no effect** in this codebase (manual: vsync timing). | main.c:938 |
| `/?`, `/H`, unknown | Unknown prints `Unknown option "<c><c>"` to stderr; all three call `finish()`, print usage, `exit(1)`. | main.c:1088-1137 |

Manual section for options: `digger.txt:396-531`. Manual says `/N` uses `-` separator with `~` alternative;
usage text says `~` preferred.

Environment variables seen in my area: `DIGGER_CI_RUN`, `DIGGER_CI_RUN_DTL` (print score line at each level
completion, main.c:280), `DIGGER_NETSIM_REPLAY*` (instrumentation), `DIGGER_LOG_T0`, `HOME`.

### 1.2 INI settings (`inir`, main.c:1235-1311)

File: `$HOME/.digger.rc` (`./.digger.rc` if no HOME) on Unix/SDL; `DIGGER.INI` elsewhere;
`/var/games/digger/digger.rc` FreeBSD VGL (`def.h:78-104`). Windows-INI style `[Section]`/`key=value`
(`ini.c`). `GetINIBool` (`ini.c:187-197`): default printed as `%i`; value upper-cased; true if first
char is `T`, otherwise `atoi(value)`.

| Section | Key | Default | Target |
|---|---|---|---|
| `Keys` | `keynames[i]` (+ suffix `2` for i in 5..9), i in 0..NKEYS(20) | current `keycodes[i][0..4]` as `a/b/c/d/e` | `keycodes[i][j]` (parsed by `atoi` between `/`), `krdf[i]=true` |
| `Game` | `GauntletTime` | 120 | `gtime` |
| `Game` | `Speed` | 80000 (`DEFAULT_FTIME`, µs/frame = 12.5 Hz) — only read if `ftime==0` (it is at startup) | `ftime` |
| `Game` | `GauntletMode` | false | `gauntlet` |
| `Game` | `Players` | `"1"` | upper-cased; `"2S…"` → `diggers=2,nplayers=1`; else `diggers=1`, `nplayers=atoi` clamped to 1 if not in 1..2; `netsim=false` |
| `Game` | `UnlimitedLives` | false | `unlimlives` |
| `Game` | `StartLevel` | 1 | `startlev` |
| `Sound` | `SoundOn`, `MusicOn` | true | `soundflag`, `musicflag` |
| `Sound` | `Rate` 44100, `BufferSize` 512 (`DEFAULT_BUFFER`), `SoundVolume` 0 | | `sound_rate`, `sound_length` (uint16), `dx_sound_volume` |
| `Graphics` | `640x480` false, `Async` true, `CGA` false | | `use_640x480_fullscreen`, `use_async_screen_updates`, CGA fn-pointer swap |

Also: `digger_log = stderr` on Unix (else `DIGGER.log`), `g_bWindowed=true`.

**[QUIRK]** INI `Players=2` + `/2` gives `nplayers=2 && diggers=2`, a combination not in the mode table:
`getnmode()` falls through to the last entry (label "TWO PLAYER GAUNTLET"), and loops of the form
`for (i=curplayer; i<diggers+curplayer; i++)` index `digdat[2]`/`scdat[2]` when `curplayer==1`
(out of bounds, `DIGGERS==2`). A Rust port should normalise this combination away.

---

## 2. Title screen and outer loop (`mainprog`, main.c:392-605)

```
loadscores(); escape=false; title_anim_init
do {
  soundstop; title_anim_cleanup; creatembspr; detectjoy
  gclear; gtitle; outtext("D I G G E R",100,0,3)
  shownplayers()            // mode label, 2 lines at y=25 and y=39
  showtable()               // high scores (or NetSim friends list)
  started=false; keyredef=false; frame=0
  newframe(); teststart();  // swallow a pending start
  while (!started) {
    [netsim title status / friend list up-down handling]
    started = teststart();          // any key that is not mode-change/redefine (input.c:151-153, 486-501)
    if (keyredef) break;
    if (mode_change) { switchnplayers(); loadscores(); shownplayers(); showtable(); mode_change=false; }
    title_anim_step(&title_anim, ddap, frame);
    newframe(); frame++; if (frame > title_anim_last_frame()) frame=0;
  }
  if (keyredef) { redefkeyb(false); continue; }
  if (savedrf)  { if (gotgame) { recsavedrf(); gotgame=false; } savedrf=false; continue; }   // F8
  if (escape)   { if (title_escape_exits_program()) break; escape=false; continue; }        // F10: exit (not on Emscripten)
  [netsim: "WAITING FOR PEER" at 68,0; netsim_start_session; on failure soundddie + 15 frames, continue]
  recinit(); soundwakeup(); game();
  [netsim drain/end]; resetframe(false); dgstate.ticks=0;
  gotgame=true; if (gotname) { recsavedrf(); gotgame=false; }
  savedrf=false; escape=false;
} while (!escape);
netsim save/shutdown; finish(); return 0;
```

- There is **no attract/demo game**: the title screen only runs the title animation (`title_anim.c`,
  cyclic frame counter 0..`title_anim_last_frame()`), paced by `newframe()`.
- Title keys (manual `digger.txt:109-113`): N = cycle mode, K = redefine keys, F8 = save last game, F10 = exit.
- `escape` set in-game (F10) returns to the title; `escape` is cleared after `game()`, so the loop's
  `while (!escape)` only terminates through the `break`.
- Every mode switch reloads scores because each mode has its own table (main.c:490).

### 2.1 Game modes (main.c:631-645)

`possible_modes[]` = (gauntlet, netsim, nplayers, diggers, label line 1 @x, label line 2 @x):

| # | gauntlet | netsim | nplayers | diggers | labels |
|---|---|---|---|---|---|
| 0 | F | F | 1 | 1 | "ONE"@220 / " PLAYER "@192 |
| 1 | F | F | 2 | 1 | "TWO"@220 / " PLAYERS"@184 |
| 2 | F | F | 1 | 2 | "TWO PLAYER"@180 / "SIMULTANEOUS"@170 |
| 3 | F | T | 1 | 2 | "TWO-PLAYER"@180 / "NETSIM"@206 (only if `netsim_configured()`) |
| 4 | T | F | 1 | 1 | "GAUNTLET"@192 / "MODE"@216 |
| 5 (last) | T | F | 1 | 2 | "TWO PLAYER"@180 / "GAUNTLET"@192 |

`getnmode()` (main.c:656-674) scans entries 0..4 and **defaults to 5** for anything unmatched.
`switchnplayers()` (main.c:712-725) advances cyclically, skipping unavailable modes, and copies the four
fields into `dgstate`. `shownplayers()` erases 10 chars at (180,25) and 12 chars at (170,39), draws
line 1 colour 3, line 2 colour 3 (NetSim: 2 = starting/off, 1 = registered, 3 = running).

Mode semantics:
- **Two players (alternating)**: `nplayers=2, diggers=1`. Player index = `curplayer` (0/1); each has own
  level number, field copy (`field1/field2`), emerald bit, bag set, score, lives. Turn passes on death.
- **Two-player simultaneous** (`/2`): `nplayers=1, diggers=2`; both diggers share one field, level, lives are per digger.
- **Gauntlet** (`/G`): lives never decrease (`declife` no-op, digger.c:1066-1070 and 814); game ends on timeout; bonus = +15 s.
- **Unlimited lives** (`/U`): removes the 5-lives cap for bonus lives only (scores.c:220). Not recorded
  in DRF (record.c:416 commented out) and ignored on playback of the header (record.c:161).
- **Start level** (`/I`): `gamedat[*].level = startlev`; written to DRF header as `I<n>` when >1.
- **Cheat key** (Ctrl-T, `DKEY_CHT`, input.c:116-121): during playback → take over; otherwise, if not
  gauntlet, `drfvalid=false` → the game can no longer be saved nor enter the high-score table (scores.c:244).

---

## 3. The game proper (`game()`, main.c:130-320)

### 3.1 Setup (main.c:134-158)

```
sbase = seedbase(); round = 0
resetframe(true); dgstate.ticks = 0
twoplayers = nplayers==2 || diggers==2            // for CI score line only
if gauntlet: cgtime = gtime*1193181; timeout=false
initlives()                                       // memset digdat; lives=3 for i < diggers+nplayers-1 (digger.c:1057-1064)
gamedat[0].level = startlev; if nplayers==2: gamedat[1].level = startlev
alldead=false; gclear()
curplayer=0; initlevel(); curplayer=1; initlevel()   // BOTH players always
zeroscores(); bonusvisible=true
flashplayer = (nplayers==2); curplayer=0
```

**[QUIRK]** `initlevel()` is run for player index 1 even in one-player games. `gamedat[1].level` is not
assigned unless `nplayers==2`, so it is 0 in a fresh process (→ `getlevch` returns `' '` for `l<1`,
main.c:76) but **stale from the previous game** otherwise. That fills `field2`, bit 1 of every
`emfield[]` cell and `bagdat2`. `emfield` is hashed whole (both bits), so the state hash of a
one-player game depends on this. A port must reproduce "player-2 level = 0 → blank" for a fresh
process; whether to reproduce the stale case is a decision (I would reset `gamedat[1].level=0`).

`initlevel()` (main.c:734-745): `levdone=false`; if `curplayer<nplayers` → `edrf_level(levno(), levplan())`
(records the map, or on eDRF playback *overwrites `dgstate.leveldat[plan-1]`* with the recorded map,
edrf.c:607-641); `makefield(); makeemfield(); initbags(); levnotdrawn=true`.

### 3.2 Loop structure

```
while (getalllives()!=0 && !escape && !timeout) {            // lives of the CURRENT player's diggers
  while (!alldead && !escape && !timeout) {                  // one iteration = one "round" (a life or a level)
    initmbspr()
    randv = playing ? playgetrand() : [edrf feed] : roundseed(sbase, round++)
    recputrand(randv)
    if (levnotdrawn) {                                       // new level or player switch: full redraw
      levnotdrawn=false; drawscreen()
      if (flashplayer) { ... 15*3 frames flashing "PLAYER n" at (108,0) colours 1..3 with writecurscore;
                         each frame newframe(); if escape return;   // NB: returns without edrf_gameend
                         drawscores(); addscore(i,0) for each digger }
      else drawscores()
    } else initchars()                                       // after a death: initmbspr, initdigger, initmonsters
    erasetext(8, 108,0,3); initscores(); drawlives(); music(MUSIC_MAIN,1.0)
    flushkeybuf(); input_reset_directions()

    // ---- main tick loop ----
    while (!alldead && !levdone[cur] && !escape && !timeout) {
      penalty=0; newframe(); if (escape||timeout) break
      dodigger(); domonsters(); dobags()
      if (penalty>8) incmont(penalty-8)
      checklevdone()
      game_tick()                                            // ticks++, edrf_tick()
      testpause()
      if (escape||timeout) break
    }

    // ---- wind-down loop ----
    erasediggers(); musicoff(); t=20
    while ((getnmovingbags()!=0 || t!=0) && !escape && !timeout) {
      if (t!=0) t--
      penalty=0; newframe(); if (escape||timeout) break
      dobags(); dodigger(); domonsters()                     // NB different order from the main loop
      if (penalty<8) t=0
      game_tick()                                            // no testpause, no checklevdone, no incmont
    }
    soundstop()
    for i in cur..cur+diggers: killfire(i)
    erasebonus(); cleanupbags(); savefield(); erasemonsters(); gflush()
    recputeol(); if playing: playskipeol(); if escape: recputeog()
    if levdone[cur]: soundlevdone() [+ CI score line]
    if (countem()==0 || levdone[cur]) {
      for i in cur..cur+diggers: if (getlives(i)>0 && !digalive(i)) declife(i)
      drawlives(); level[cur]++; if (level[cur]>1000) level[cur]=1000; initlevel()
    } else if (alldead) {
      for i in cur..cur+diggers: if (getlives(i)>0) declife(i)
      drawlives()
    }
    if ((alldead && getalllives()==0 && !gauntlet && !escape) || timeout) endofgame()
  }
  alldead=false
  if (nplayers==2 && getlives(1-curplayer)!=0) { curplayer=1-curplayer; flashplayer=levnotdrawn=true }
}
edrf_gameend()
```

Details that matter:

- `drawscreen()` (main.c:747-756): `gclear, creatembspr, drawstatics, drawbags, drawemeralds, initdigger, initmonsters`.
  `drawstatics` → `drawfield` **mutates `field[]`** (section 5.3).
- `checklevdone()` (main.c:765-771): `levdone = (countem()==0 || monleft()==0) && isalive()`; recomputed every tick.
- Level advance condition is `countem()==0 || levdone` — a level whose last emerald was eaten on the
  tick the digger died still advances, and the dead digger also loses a life (`!digalive(i)` branch).
- Wind-down: at least one frame; ends when no bags are moving and either a frame had `penalty<8` or 20 frames passed.
- In two-digger mode a single digger's death decrements its life inside digger.c:813-816 (other digger
  still alive); the `alldead` branch in `game()` handles the all-dead case.
- `setdead(bool)` (main.c:804) is the only external writer of `alldead` (called from digger.c:811).
- `endofgame` is called from *inside* the inner loop; afterwards `alldead` is still true so the inner loop exits.
- Level number is capped at 1000 (main.c:293).

### 3.3 Pause (`testpause`/`run_pause`, main.c:809-870)

Called once per main-loop tick after `game_tick()`. Non-NetSim: if `pausef` (Space) → `soundpause()`,
`cleartopline()`, `outtext("PRESS ANY KEY",80,0,1)`, `gflush`, blocking `getkey(true)`; then
`cleartopline(); drawscores(); addscore(i,0)` for each digger; `drawlives(); gethrt(true,1); pausef=false;
soundpauseoff()`. No ticks elapse (so the gauntlet clock stops). NetSim adds a remote-pause handshake
through `pauseframe()` and the text "PLAYER n AWAY" (netsim report). `cleartopline()` = erase 26 chars at
(0,0) and 1 char at (308,0) (main.c:778-782).

### 3.4 Quit

F10 (`DKEY_EXT`) sets `escape` (input.c:135). In game: all loops exit, `recputeog()`, back to the title.
On the title: program exit. `edrf_quitting()` at the end of an eDRF playback also sets `escape`
(digger.c:268).

---

## 4. Frame timing

- One game tick = one `newframe()` (digger.c:213-217) → `syncframe(false,false,true,…)` (digger.c:254-283):
  `gethrt(false,1)` (skipped while `deathffwd`), `checkkeyb()`, `frame++`, `input_advance_fire_state()`,
  `netsim_game_frame(...)`.
- `dgstate.ftime` = microseconds per frame. Default `DEFAULT_FTIME = 80000` → 12.5 Hz (def.h:55-56).
  "Speed n" on the command line = `n*2000` µs (default 40). `ftime<=1` = run unthrottled
  (`gethrt` returns after a screen update, plus 10 ms sleep if `minsleep`; sdl_timer.c:60-65).
- In-game `+`/`-` keys: `ftime -= 10000` (only if `ftime>10000`) / `ftime += 10000` (input.c:122-128) —
  i.e. 5 speed units; no upper bound; `ftime` is `uint32_t`.
- `gethrt(minsleep, mult)` (sdl_timer.c:54-89) is a software PLL: `tfreq = mult*1e6/ftime`;
  `clk = SDL_GetTicks()*tfreq/1000 (+ netsim lead)`; `eval = PFD_get_error(clk)` (digger_math.c:44-68);
  low-pass `recfilter` (cut-off 0.1 Hz, initialised with the `ftime` at `inittimer` time) over
  `sigmoid(eval) = x/(1+|x|)`; sleep `round(1000/tfreq * (1 + filterval))` ms after `doscreenupdate`.
  `mult=3` is used for sub-frame delays: `redrawdelay()` (digger.c:299-311, one third of a frame per
  tunnel cell / emerald while a level is being drawn) and pause frames.
  None of this is simulation state; a Rust port only needs "sleep until next multiple of ftime/mult".
- `dgstate.ticks` (uint32) counts simulated ticks only (`game_tick`, game.c:101-107): zeroed at game
  start and after the game. It does **not** advance during player flashing, redraw delays, pause,
  "GAME OVER" frames. `frame` (digger.c:134) counts every `syncframe`.
- **Penalty counter** — emulation of the original's CPU slowdown. `static int16_t penalty`
  (main.c:53) is zeroed before each tick; `incpenalty()` (main.c:773) is called from drawing-heavy
  operations: `eatfield` (drawing.c:147), `addscore` ×3 (scores.c:229-231, even for a 0 score),
  many sites in `bags.c`, `digger.c`, `monster.c`, and ×2 in joystick read (input.c:466). After
  `dobags()` in the main loop: `if (penalty>8) incmont(penalty-8)`; `incmont(n)` (monster.c:644-651):
  `n=min(n,6); for (m=1;m<n;m++) mondat[m].t++` — **[QUIRK]** monster 0 is never delayed and an excess of
  exactly 1 does nothing. This is simulation state and must be exact.
- Gauntlet clock: section 7.4.

---

## 5. World model

### 5.1 Constants (def.h:23-56, draw_api.h:1-3)

`DIR_NONE=-1, DIR_RIGHT=0, DIR_UP=2, DIR_LEFT=4, DIR_DOWN=6`; `TYPES=5`; `BONUSES=1, BAGS=7,
MONSTERS=6, DIGGERS=2, FIREBALLS=DIGGERS`; `SPRITES=18`; sprite index ranges: bonus 0, bags 1..7,
monsters 8..13, fireballs 14..15, diggers 16..17. `MWIDTH=15, MHEIGHT=10, MSIZE=150`.
Screen 320×200 logical, `CHR_W=12`, `MAX_TEXT_LEN=26`. Cell → pixel: `x = h*20+12`, `y = v*18+18`
(emeralds drawn at `y*18+21`). `MAX_REC_BUFFER=262144`. `DIGGER_VERSION "MS MPL 20190212"`.

`struct obj_position { int16_t dir, x, y; }` (digger_types.h:33-38).

### 5.2 Shared state and who mutates it

`struct gamestate dgstate` (game.h:4-18, initialised game.c:12-98):

| Field | Type | Init | Writers |
|---|---|---|---|
| `nplayers, diggers` | int16 | 1,1 | inir, parsecmd, switchnplayers, openplay (set from recording, restored after) |
| `curplayer` | int16 | 0 | game() only |
| `startlev` | int16 | 1 | inir, `/I`, openplay (restored) |
| `levfflag`, `levfname[132]` | bool, char | false | parsecmd / read_levf (appends ".DLF") |
| `pldispbuf[14]` | char | | game(), endofgame ("PLAYER n") |
| `randv` | **int32** | 0 | game() per round, `randno()` |
| `netsim_remote_lead_ms` | int | 0 | netsim |
| `leveldat[8][10][16]` | int8 | built-in levels | read_levf, openplay (record.c:171-181, **not restored**), edrf_level |
| `gtime` | int | 0→INI 120 | inir, `/G`, openplay (restored) |
| `gauntlet, netsim, timeout, unlimlives` | bool | false | inir/parsecmd/mode switch; `timeout` by dodigger (digger.c:339) and game() |
| `ftime` | uint32 | 0→INI 80000 | inir, `/S`, +/- keys, playtakeover |
| `cgtime` | uint32 | | game() start, dodigger (digger.c:338-340), addscore bonus (scores.c:222) |
| `ticks` | uint32 | 0 | game_tick, game(), mainprog |

main.c statics: `gamedat[2] {int16 level; bool levdone}` (main.c:45-49), `levnotdrawn`, `alldead`,
`penalty` (int16), `started` (global), `twoplayers`, `quiet`, `sound_rate/sound_length` (uint16).

scores.c: `scdat[DIGGERS] {int32 score, nextbs, tscore}` (27-30), `scorehigh[12]` int32, `scoreinit[11][4]`,
`scoret` int32, `bonusscore` **uint16** = 20000, buffers `highbuf[10]`, `hsbuf[36]`, `scorebuf[512]`.

drawing.c: `int16_t field[150]` (global, extern), `static field1[150], field2[150]` per-player saves;
sprite buffers; `bitmasks[12]`; animation state `digspr/digspd/firespr`.

digger.c (world-model part): `static int8_t emfield[150]`, `static int16_t emmask` (digger.c:61-63).

**[QUIRK]** `openplay` restores `gauntlet/gtime/startlev/diggers/nplayers` after a `/P` playback but
**not** `bonusscore` nor `leveldat` — a subsequent live game uses the recording's levels and bonus step.

### 5.3 Field encoding (`field[v*15+h]`, int16)

- `makefield()` (drawing.c:65-81): every cell starts as `-1` (0xFFFF). For level char `S` or `V`:
  `&= 0xd03f` (clear bits 6..11 and bit 13). For `S` or `H`: `&= 0xdfe0` (clear bits 0..4 and bit 13).
  Result copied into `field1` (curplayer 0) or `field2`. Iteration order is x-outer, y-inner (irrelevant
  for the result, but `levplan()`/`getlevch` are re-evaluated per cell).
- Bit meaning: bits **0..4** = five horizontal 4-pixel slices of earth in the cell (1 = earth present);
  bits **6..11** = six vertical 3-pixel slices; bit **13 (0x2000)** = "cell completely untouched";
  bits 5, 12, 14, 15 are never cleared here (monster.c may use them — see the monster report).
- `bitmasks[i] = ~(1<<i)` for i in 0..11 (drawing.c:27-28).
- `eatfield(x,y,dir)` (drawing.c:144-185), pixel coords of the digger:
  `h=(x-12)/20; xr=((x-12)%20)/4; v=(y-18)/18; yr=((y-18)%18)/3` (C truncating division), `incpenalty()`, then
  - RIGHT: `h++; f[v,h] &= bitmasks[xr]; if ((f & 0x1f)==0) f &= 0xdfff`
  - UP: `yr--; if (yr<0) { yr+=6; v--; } f[v,h] &= bitmasks[6+yr]; if ((f & 0xfc0)==0) f &= 0xdfff`
  - LEFT: `xr--; if (xr<0) { xr+=5; h--; } f[v,h] &= bitmasks[xr]; if ((f & 0x1f)==0) f &= 0xdfff`
  - DOWN: `v++; f[v,h] &= bitmasks[6+yr]; if ((f & 0xfc0)==0) f &= 0xdfff`
  No bounds checks (`h` can reach 15 → next row's cell 0, `v` 10 → past the array): callers must keep
  the digger inside; in Rust decide between wrapping index arithmetic on a padded array and asserting.
- `drawstatics()` (drawing.c:83-96) copies `field1/2 → field`, then `drawfield()` (drawing.c:109-142):
  for each cell with bit 13 clear: if `(f&0xfc0)!=0xfc0` → `f &= 0xd03f` (**all** vertical bits
  cleared) and draw; if `(f&0x1f)!=0x1f` → `f &= 0xdfe0`; plus joins to right/below neighbours when
  `(neighbour & 0xfdf) != 0xfdf`; `redrawdelay()` per such cell.
  **[QUIRK]** so a redraw (new level, or the player switch in alternating two-player games)
  *normalises partially dug cells to fully dug*. This changes simulation state and the state hash; a
  headless port must still perform the mutation.
- `savefield()` (drawing.c:98-107) copies `field → field1/2` at the end of each round.
- `getfield(x,y)` = `field[y*15+x]` (monster.c:653).

### 5.4 Emerald map

`emfield[150]` int8, bit `1<<curplayer` per player (`emmask`). `makeemfield()` (digger.c:930-940) sets the
bit where the level char is `'C'`, clears it elsewhere. `countem()` (digger.c:985-993) counts set bits
for the current mask; `hitemerald`/`killemerald` clear them. `drawemeralds()` re-sets `emmask`.

### 5.5 Level data

- Storage: `dgstate.leveldat[8][MHEIGHT][MWIDTH+1]` — 8 plans × 10 rows × 15 chars (+NUL), game.c:18-97.
  Characters: `' '` earth, `S` = tunnel in both directions ("start"/junction), `H` horizontal tunnel,
  `V` vertical tunnel, `B` bag (consumed by `initbags`, bags.c:40), `C` emerald. Plan 1 sample:

  ```
  "S   B     HHHHS"
  "V  CC  C  V B  "
  "VB CC  C  V    "
  "V  CCB CB V CCC"
  "V  CC  C  V CCC"
  "HH CC  C  V CCC"
  " V    B B V    "
  " HHHH     V    "
  "C   V     V   C"
  "CC  HHHHHHH  CC"
  ```
- `getlevch(x,y,l)` (main.c:73-83), `l` = plan 1..8: returns `' '` if `l<1`; **[QUIRK]** returns `'H'`
  for plans 3 and 4 at `y==9, x in {6,8}` when `!levfflag && diggers==2` and not eDRF playing/feeding
  (opens the start cells of the two diggers, which start at h=8 and h=6, digger.c:104); else `leveldat[l-1][y][x]`.
- Level numbering: `gamedat[p].level` 1..1000. `levplan()` (main.c:784-790): `l<=8 → l`, else `(l&3)+5`
  → 9→6, 10→7, 11→8, 12→5, 13→6 …, 1000→5 ("12345678, 678, (5678)×247, 5 forever").
  `levof10()` (main.c:792-797) = `min(level,10)`, the difficulty index used by monsters/bags/digger
  (e.g. `mongaptime=45-2*l`, `totalmonsters=l+5`, `goldtime=150-10*l`, `bonustimeleft=250-20*l`, recharge `3*l+60`).
- Level file `.DLF` (`read_levf`, main.c:883-916): open `name`, else `name.DLF`; 2 bytes → `bonusscore`
  (native-endian uint16), then `fread(dgstate.leveldat, 1200, 1, f)`; high scores live in the same
  file at offset 1202 (512 bytes).
  **[BUG?]** the file holds 8×10×15 = 1200 bytes, but the array rows are 16 bytes wide, so the raw
  read lands misaligned (file byte k → row k/16, column k%16; the last 5 rows of plan 8 keep built-in
  data). The DRF reader/writer (record.c:171-181, 422-428) copy 15 chars per row correctly. A Rust
  port should read 15-byte rows (the evident intent) and note the deviation.
- DRF header carries all 8 plans; eDRF carries each map as played via `edrf_level`.

---

## 6. Scoring (scores.c)

### 6.1 Events

| Event | Points | Function / call site |
|---|---|---|
| Emerald | 25 | `scoreemerald` scores.c:475; digger.c:633 |
| 8 consecutive emeralds (octave) | +250 | `scoreoctave` scores.c:480; digger.c:640 |
| Gold collected | 500 | `scoregold` scores.c:485; bags.c:456 (digger `i-FIRSTDIGGER+curplayer`) |
| Monster shot (fireball) | 250 | `scorekill` scores.c:451; digger.c:479 |
| Monster crushed by bag, one digger | 250 to `curplayer` | monster.c:541 |
| Monster crushed by bag, two diggers | 125 + 125; if digger 0 has 0 lives: 0 / (250 if digger 1 has lives else 0); if digger 1 has 0 lives: 250 / 0. `addscore` is called for **both** diggers regardless (penalty +6). | `scorekill2` scores.c:459-473; monster.c:539 |
| Bonus (cherry) | 1000 | `scorebonus` scores.c:490; digger.c:690 |
| Monster eaten in bonus mode | `msc*200`, `msc` doubles per eat (200, 400, 800, 1600, …), `msc` reset to 1 at initdigger / reincarnation / bonus start (digger.c:127, 840, 875) | `scoreeatm` scores.c:495; digger.c:699-703 |

### 6.2 `addscore(n, int16_t score)` (scores.c:205-232) — exact behaviour

```
scdat[n].score += score;
if (scdat[n].score > 999999) { scdat[n].tscore += scdat[n].score; scdat[n].score = 0; }
draw score (colour 1): n==0 at x=0; n==1 at x=236 if score<100000 else x=248; width 6
if (scdat[n].score >= scdat[n].nextbs + n) {          // "+n to reproduce original bug"
  if (getlives(n) < 5 || unlimlives) {
    if (gauntlet) cgtime += 17897715;                 // 15 s
    else addlife(n);                                  // lives++, sound1up, musicrefresh
    drawlives();
  }
  scdat[n].nextbs += bonusscore;
}
incpenalty(); incpenalty(); incpenalty();
```

- `zeroscores()` (scores.c:175-181): `score=tscore=0`, `nextbs=bonusscore` for both, `scoret=0`.
- **[QUIRK]** Player/digger 2 gets the extra life only when *exceeding* a multiple of 20000 by ≥1 (`+n`).
- **[QUIRK]** Max 5 lives total (4 spare) unless `/U`; `nextbs` advances even when no life is given.
- **[QUIRK]** At >999999 the visible score resets to 0 and the old value moves to `tscore`; `nextbs`
  is not reset, so no further bonus lives are ever awarded (manual digger.txt:239-243). High-score
  comparison and the table use `score` only (scores.c:268); `gettscore(n) = tscore+score`
  (scores.c:86-89) is used for the CI line and the state hash.
- Only one bonus step is processed per call (`if`, not `while`).
- **[QUIRK]** the parameter is `int16_t`: `scoreeatm` passes `msc*200` (int) which truncates; with
  `msc` = 256 (ninth eat in one bonus mode) 51200 becomes −14336, and `msc` itself is int16 `<<= 1`.
  Port as `(msc as i32 * 200) as i16` and `msc = msc.wrapping_shl(1)`.
- `writenum` (scores.c:500-512) draws `w` digits right-aligned from `x+(w-1)*12`; every position is
  drawn (leading zeros included) except the leftmost (`w==1`), drawn only if non-zero → scores show as
  `00000`, 6th digit only when needed. `numtostring` (scores.c:514-529): 6 chars, right-aligned,
  space-padded, `0` → `"     0"`.
- `writecurscore(col)` (scores.c:183-192), `drawscores()` (194-203; second score when `nplayers==2 || diggers==2`),
  `initscores()` (144-149) = `addscore(i,0)` for each digger.

### 6.3 Game over and high-score entry (`endofgame`, scores.c:234-292)

1. `addscore(i,0)` for each digger.
2. Return immediately (no "GAME OVER", no table) if `playing || edrf_feeding || !drfvalid || playtaken`.
3. NetSim: drain and end the session; only the local player's score is considered.
4. Gauntlet: `cleartopline(); "TIME UP"` at (120,0) colour 3 for 50 frames (abort on escape), erase.
5. For `i` in `curplayer .. curplayer+diggers` (NetSim: local player only): `scoret = scdat[i].score`;
   if `scoret > scorehigh[11]` (strictly beats 10th place): `gclear; drawscores;` "PLAYER n" at (108,0)
   colour 2; `" NEW HIGH SCORE "` at (64,40) colour 2; `getinitials(); shufflehigh(); savescores()`.
6. If nobody entered initials and not gauntlet: `"GAME OVER"` at (104,0) colour 3 for 50 frames.

`getinitials` (scores.c:365-396): `newframe()`; texts "ENTER YOUR" (100,70), " INITIALS" (100,90),
"_ _ _" (128,130); `scoreinit[0]="..."`; `killsound()`; for i in 0..3: `k=getinitial(i*24+128,130)`;
if `k==8 || k==127` → step back one position (if i>0) and retry; else draw the char and store it.
Then 20×`flashywait(15)`, `setupsound()`, `gclear, gpal(0), ginten(0)`, `recputinit(scoreinit[0])`
(writes `*ABC` into the DRF).

`getinitial` (scores.c:412-434): draw `_`; loop forever: phase A 40×: if key available — non-alphanumeric
keys are swallowed (`continue`), alphanumerics returned; else `flashywait(15)`. Phase B 40×: if key —
redraw `_` and return it **unfiltered**; else `flashywait(15)`. **[QUIRK]** so Backspace/Delete (and
any other non-alphanumeric character) are only accepted during phase B. No case folding here.

`flashywait(n)` (scores.c:398-410): `gethrt(true,1)`, then `2n × volume` palette toggles
`gpal(p=1-p); gflush_sync()` (+ empty 19-iteration loop). Net palette is restored (even count).

`shufflehigh` (scores.c:436-449): `for (j=10;j>1;j--) if (scoret<scorehigh[j]) break;` then shift
entries `j..9` down one (`scorehigh[i+1]=scorehigh[i]; scoreinit[i]=scoreinit[i-1]` for i=10..j+1) and
insert at table position `j` (`scorehigh[j+1]=scoret; scoreinit[j]=scoreinit[0]`). Ties go **above**
existing equal scores. Indexing: table position `i` (1..10) is `scoreinit[i]` / `scorehigh[i+1]`;
`scoreinit[0]` is the entry being typed; `scorehigh[0..1]` are unused.

`showtable` (scores.c:294-342): erases 11 chars at (16,25) and at (16, 44+13i) for i in 0..9; "HIGH SCORES"
at (16,25) colour 3; row i (1..10) at (16, 31+13i) = `initials + "  " + 6-char score`, colour 2 for the
first, 1 for the rest. In NetSim mode it shows the "FRIENDS" list instead.

### 6.4 Score file

- Name: `$HOME/.digger.sco` (`./.digger.sco` without HOME) on Unix/SDL; `DIGGER.SCO` elsewhere;
  `/var/games/digger/digger.sco` on FreeBSD VGL (scores.c:56-77). With a level file (`levfflag`) the
  scores are stored **inside the level file at offset 1202** (scores.c:110-114, 137-141).
- Format: a raw 512-byte buffer `scorebuf`, read/written whole (`fread(…,512,1)`; a file shorter than
  512 bytes counts as a failed read: `scorebuf[0]=0` only — the other sections keep whatever was read).
- Four 111-byte sections selected by `p = (gauntlet ? 111 : 0) + (diggers==2 ? 222 : 0)`
  (offsets 0, 111, 222, 333; NetSim shares the two-digger section). Section layout:
  byte 0 = `'s'` (validity marker); then 10 entries × 11 bytes at `p + i*11 - 10` (i = 1..10):
  3 initials, 2 spaces, 6-char right-aligned space-padded decimal score. No terminators; bytes
  444..511 unused.
- `loadscores` (scores.c:151-173): if marker missing → all ten entries `"..."`/0 (it also resets
  `scoreinit[0]`); else parse 3 chars, skip 2, 6 chars → `atol`.
- `savescores` (scores.c:344-363) rebuilds the section and `writescores()` (`"wb"` for the score file,
  `"r+b"` + seek for a level file).

### 6.5 Lives display (`drawlives`, drawing.c:361-411) — summary

Gauntlet: `g = (int16)(cgtime/1193181)`; text `"%3i:%02i"` (g/60, g%60) at (124,0) colour 3, nothing else.
Otherwise player 1: `n = lives-1`; if `n>4` → life icon at (80,0) + `"X<n>"` at (100,0); else four slots
at x = 80,100,120,140 (icon while n>0, blank sprite 2 otherwise). Second player/digger mirrored at
x = 224…164 (icon type 1 for alternating player 2, type 3 for simultaneous digger 2).

---

## 7. RNG, seeds, determinism

### 7.1 Game RNG — must be bit-exact

```c
int16_t randno(int16_t n) {                 /* main.c:1226-1230 */
  dgstate.randv = dgstate.randv*0x15a4e35l + 1;      /* int32 state, wraps mod 2^32 */
  return (int16_t)((dgstate.randv & 0x7fffffffl) % n);
}
```
Rust: `randv = randv.wrapping_mul(0x015A_4E35).wrapping_add(1); ((randv & 0x7fff_ffff) % n as i32) as i16`.
`n==0` would divide by zero (cannot happen with levels ≥1: arguments are `15-levof10()` ∈ 5..14 and
`levof10()+5` ∈ 6..15; it *can* with a non-positive `/I`).

Call sites (the only two in the game logic):
- `monster.c:113` — `randno(15-levof10())==0` → nobbin gets a second `monai` step. Called once per live
  monster with `t==0`, *before* testing nobbin/alive ("Need to split for determinism").
- `monster.c:273` — `randno(levof10()+5)==1` → swap direction priorities, only acted on when `levof10()<6`,
  but the draw always happens.

The sound code has its own identical LCG `randnos`/`randvs` (sound_backend.c:71-80, reset to 0 at
1140) precisely so that sound does not perturb the game stream. Nothing else in my area consumes `randv`.

### 7.2 Seeding

- One seed **per round** (per life / per level start), set at the top of the inner loop (main.c:163-175)
  and recorded with `recputrand(randv)` (DRF: `%08lX` line; eDRF: `R %08X`).
- Playback: `playgetrand()` (record.c:504-523) reads the 8 hex digits back → playback sync depends only
  on the LCG, not on the seed generator.
- Live: `roundseed(sbase, round++)` = `(int32)(uint32) splitmix64(sbase, round)` (main.c:123-128), where
  `splitmix64(base,n)` (digger_math.h:70-79):
  `z = base + (u64)(n+1)*0x9e3779b97f4a7c15; z=(z^(z>>30))*0xbf58476d1ce4e5b9; z=(z^(z>>27))*0x94d049bb133111eb; return z^(z>>31)`
  (all wrapping u64; `n+1` is computed in u32).
- `seedbase()` (main.c:112-121): NetSim → `netsim_session_nonce()` (same on both peers, so splitmix64 must
  be exact for C↔Rust NetSim interop); otherwise `time(NULL) ^ (clock()<<32) ^ (++ngames<<48)`.

### 7.3 What else must be exact for recorded-game sync (in this area)

- Order of subsystem updates per tick (`dodigger, domonsters, dobags`) and in the wind-down loop
  (`dobags, dodigger, domonsters`), the 20-frame wind-down rule, `checklevdone`.
- The penalty counter and `incmont` (section 4), including the three increments per `addscore` call.
- Level plan/`levof10` mapping, the `'H'` two-digger kludge, `bonusscore` taken from the recording
  header (record.c:169), the 5-life cap, the `+n` bonus bug, the 1,000,000 rollover.
- `drawfield`'s normalisation of `field` on redraw.
- Death/bonus-life side effects go through `getlives`: a different life count changes when the game ends.
- Recording header mode parsing sets `nplayers/diggers/gauntlet/gtime/startlev` (record.c:133-164);
  `kludge=true` for recordings with version date ≤ 19981125 (record.c:131, see the recording report).
- `unlimlives` is not stored: playback uses whatever the current process setting is.

### 7.4 Gauntlet clock

`cgtime` (uint32) is in PIT units, 1193181 per second: start `gtime*1193181` (max 3599 →
4,294,258,419, just under 2^32); each `dodigger` tick (digger.c:334-341):
`drawlives(); if (cgtime < 80000) timeout = true; cgtime -= 80000;` — the final subtraction **wraps**
(use `wrapping_sub`; the wrapped value is hashed). Bonus: `cgtime += 17897715` — can wrap past 2^32
for `gtime` near 3599 (`wrapping_add`). The decrement is the constant `DEFAULT_FTIME`, independent of
the actual speed, so the clock counts ticks: one displayed "second" = 1193181/80000 ≈ 14.9 ticks ≈
1.19 real seconds at the default 12.5 Hz. (The manual's remark about replaying gauntlet games at the
recorded speed, digger.txt:457-459, is obsolete.) The clock does not run during pause, redraws, or
between rounds beyond the wind-down ticks; it does run while dying.

---

## 8. state_hash (state_hash.c/.h)

- Algorithm: **MD5** (mackron md5, `microsippy/src/external/mackron_md5/md5.h`) over a byte stream;
  result = first 4 digest bytes as a big-endian u32 (`d[0]<<24|d[1]<<16|d[2]<<8|d[3]`, state_hash.c:100-108)
  so it prints like the start of the MD5 hex.
- Each value is fed at its C size, little-endian (`STATE_HASH_VAR(v)` = `sizeof(v)` bytes;
  `bool` = 1 byte 0/1; `int16_t` = 2; `int32_t/uint32_t` = 4). No padding, no struct dumps.
- `game_state_append` (state_hash.c:155-174), in order:
  1. `field[150]` int16 (300 bytes) — the live field only, not `field1/field2`.
  2. `digger_debug_hash_append` (digger.c:168-211): `startbonustimeleft, bonustimeleft, emmask` (i16),
     `bonusvisible, bonusmode, digvisible` (bool), `emfield[150]` (i8); then for each of the 2 diggers:
     `h,v,rx,ry,mdir,bagtime,rechargetime,deathstage,deathbag,deathani,deathtime,emocttime,emn,msc,lives,ivt`
     (16 × i16), `notfiring,firepressed,dead,levdone,invin` (5 × bool), `dob.x,dob.y,dob.dir` (i16),
     `dob.alive` (bool), `bob.x,bob.y,bob.dir,bob.expsn` (i16). Not hashed: `deathmusicdone`,
     `dirgetime`, `dob.zombie/can_fire`.
  3. `monster_debug_hash_append` (monster.c:124-163): `nextmonster,totalmonsters,maxmononscr,nextmontime,
     mongaptime,chase` (i16), `unbonusflag` (bool); for each of 6 monsters:
     `h,v,xr,yr,dir,t,hnt,death,bag,dtime,stime,chase` (12 × i16), `flag` (bool), and **only if the
     monster object exists**: `pos.x,pos.y,pos.dir` (i16), `isalive` (bool), `isnobbin` (bool) —
     variable-length stream.
  4. `bags_debug_hash_append` (bags.c:95-117): `pushcount, goldtime` (i16); for each of 7 bags:
     `x,y,h,v,xr,yr,dir,wt,gt,fallh` (10 × i16), `wobbling,unfallen,exist` (bool). Live `bagdat` only.
  5. `dgstate.randv` (i32).
  6. `gettscore(0)`, `gettscore(1)` (i32 each).
  7. If gauntlet: `cgtime` (u32), `timeout` (bool).
- `game_state_hash_tick(tick)` appends `tick` (u32) to the same stream.
- Module hashes `field_hash, digger_debug_hash, monster_debug_hash, bags_debug_hash` hash one part each
  (used by `netsim_trace_state`, netsim_debug.c:24-47, under env `DIGGER_NETSIM_TRACE`).
- When/what for (eDRF, edrf.c): `C <hash>` checkpoint whenever `ticks % 256 == 0` in `edrf_tick`
  (i.e. after tick 256, 512, …; `EDRF_CKPT_EVERY = 8*EDRF_CHUNK(32)`, edrf.c:54-55, 700-704);
  `E <hash>` at each round end using `game_state_hash_tick(ticks)` (edrf.c:711-723, called from
  `recputeol`, i.e. **after** `cleanupbags/savefield/erasemonsters`); `Z <tick> <hash>` at game end
  (edrf.c:726-734). On playback each is compared and a mismatch sets `edrf_failed` ("the game diverges
  from the recording before tick N") → exit status 2 with `/E`. This is the upstream test oracle
  (`tests/`), and it makes an excellent conformance harness for the Rust port: bit-exact hash
  compatibility lets the port verify itself against existing `.edrf` recordings.

Consequences for Rust: keep the hashed variables with the same widths and the same "is there a monster
object" condition; hash by explicit `to_le_bytes()` in the listed order; any MD5 crate will do.

---

## 9. Integer width / signedness / overflow checklist

| Where | C behaviour to reproduce |
|---|---|
| `randv` (int32) LCG | wrapping mul/add; mask to 31 bits before `%`; result cast to i16 |
| `randno(n)` | `n` is i16; `%` on non-negative i32 |
| `splitmix64` | wrapping u64; `(n+1)` in u32; seed = low 32 bits reinterpreted as i32 |
| `penalty` | i16, reset each tick; `incmont(penalty-8)` clamps to 6, loop from m=1 |
| `addscore(score: i16)` | caller's `int` truncated to i16 (`msc*200`); `scdat.score` i32 |
| `msc` | i16, `<<= 1` each eat (wraps/sign-flips after 15 eats; theoretical) |
| `bonusscore` | u16 (read as 2 raw LE bytes from DLF; `atoi` from DRF header truncated to u16) |
| `nextbs + n` | i32 |
| `cgtime` | u32 wrapping sub of 80000 and add of 17897715; initial `gtime*1193181` truncated to u32 |
| `drawlives` gauntlet | `(int16)(cgtime/1193181)` |
| `ftime` | u32; `speedmul` i16 parse `10*s + c - '0'` with wrap, no validation; `ftime = speedmul*2000` (long) |
| `gtime` | int; parse without validation, clamp 3599, 0→120 |
| `gamedat.level` | i16, cap 1000; `levplan` uses `(l&3)+5` |
| `field[]` | i16 used as a bit set; masks are 16-bit patterns (0xd03f etc. are *negative* as i16 — use u16 in Rust and hash as 2 LE bytes, identical bytes) |
| `emfield[]` | i8 bit set (`&= ~emmask`) — use u8 |
| `eatfield` | C truncating `/` and `%` on i16 (inputs are ≥ origin in practice); unchecked indices |
| `dgstate.ticks`, `frame` | u32, `ticks % 256` |
| `scorehigh`, `scoret` | i32; strict `>` for qualifying, `<` for placement |
| `writenum`/`numtostring` | `n%10`, `n/=10` on non-negative i32 |
| `startlev` | i16 via `%hi`, unvalidated (≤0 gives blank levels and can make `randno` divide by zero) |
| `bool` in hash | exactly one byte, 0 or 1 |
| `getlives`/`digdat[]`, `scdat[]` | indexed with `curplayer + k`; only valid when `nplayers + diggers <= 3` |

---

## 10. Quirks kept from the 1983 original / compatibility kludges (summary)

1. Player 2's bonus life arrives one point late (`nextbs+n`) — scores.c:219, digger.txt:212-214.
2. Max 4 spare lives unless `/U`; the 20000 step still advances — scores.c:220-227.
3. No bonus lives after 1,000,000; visible score wraps to 0 — scores.c:208-211, digger.txt:239-243.
4. Level plan 1-8, 6-7-8, then 5-6-7-8 repeating, level number saturating at 1000 → plan 5 forever — main.c:784-790, 293.
5. Difficulty frozen at level 10 (`levof10`).
6. Penalty / `incmont` slowdown emulation; monster 0 exempt — main.c:227, monster.c:644-651.
7. `'H'` cells added to plans 3/4 for two diggers — main.c:79-81.
8. Redraw fully opens partially dug cells — drawing.c:117-128.
9. Two RNG draws that are taken even when their result is unused — monster.c:113, 273.
10. Wind-down loop update order differs from the main loop — main.c:249-255.
11. Level ends on `countem()==0` even if the digger just died; the dead digger also loses a life — main.c:284-296.
12. Score display with leading zeros in a 5/6-digit field — scores.c:500-512.
13. High-score ties: newest first; table per mode (4 sections) in one 512-byte blob.
14. Initials entry: non-alphanumerics/backspace only accepted in the second blink phase — scores.c:412-434.
15. "GAME OVER" palette flash of the original was removed (digger.txt:223-227); only 50 frames of text remain.
16. Gauntlet clock in PIT units decremented by a microsecond constant (≈1.19 s per displayed second).
17. Kludge flag for DRFs recorded by versions ≤ 19981125 (record.c:131) — behaviour lives in other modules.
18. Things I flag as probably accidental: DLF raw read into 16-wide rows (5.5); bare-word speed parsing
    after a speed was given (1.1); stale `gamedat[1].level` (3.1); `leveldat`/`bonusscore` not restored
    after `/P` (5.2); `flashplayer` early `return` on escape skipping `edrf_gameend()` (main.c:192-193);
    `/V` accepted but inert, `/B` handled but not accepted; `nplayers=2 && diggers=2` reachable via INI + `/2`.

Manual facts worth keeping next to the code (digger.txt:126-243): scoring table; music = Popcorn /
William Tell / Chopin funeral march; gold vanishes fast if dug under; 6 monster slots but ≤5 on screen;
level-done jingle is polyphonic if no music has played yet; in simultaneous mode a dead digger is
reincarnated after a while and is invincible while flashing (digger.txt:472-474; `ivt=50`, digger.c:828-829).

---

## 11. Suggested Rust decomposition (this area)

- `consts.rs` — `MWIDTH/MHEIGHT/MSIZE`, entity counts, sprite index ranges, `Dir` (`#[repr(i16)]` with
  −1/0/2/4/6 values kept numerically, since direction arithmetic and hashing use the numbers), `DEFAULT_FTIME`.
- `config.rs` — `Config { nplayers, diggers, gauntlet, gtime, netsim, unlimlives, startlev, ftime, levfile, sound… }`;
  `ini.rs` (reader/writer compatible with `.digger.rc`); `cli.rs` producing an ordered list of *actions*
  (`Play(file)`, `PlayAndExit(file)`, `Loop`, `RedefineKeys{all}`…) so the in-order semantics and
  `playbacklast` reordering survive without executing games inside the parser.
- `levels.rs` — built-in `[[[u8; 15]; 10]; 8]`, DLF loader (15-byte rows), `getlevch` (incl. the `'H'`
  kludge), `levplan`, `levof10`.
- `field.rs` — `Field([u16; 150])` with `make`, `eat(x,y,dir)`, `normalize_for_redraw()` (the mutating
  part of `drawfield`, separated from blob drawing which calls into the renderer), per-player saves;
  `EmeraldMap([u8;150])` with mask ops (or leave it with the digger module, but it is hashed with it).
- `rng.rs` — `Lcg { v: i32 }` + `splitmix64`, `round_seed`.
- `scores.rs` — `ScoreState { score, nextbs, tscore }×2`, `add_score` returning effects (`BonusLife`,
  `BonusTime`) or taking `&mut` game; `HighScores` (4-section 512-byte blob codec, shuffle), initials-entry
  state machine (make it non-blocking: the C code blocks in `getinitial`/`getkey`).
- `game.rs` — `Game` owning everything mutable that C keeps in statics (`dgstate`, `gamedat`, `penalty`,
  `alldead`, `levnotdrawn`, digger/monster/bag state). Model `game()` as an explicit state machine
  (`RoundStart → FlashPlayer(n) → Tick → WindDown(t) → RoundEnd → {NextLevel | Died | GameOver | SwitchPlayer}`)
  with a `step()` per frame, so that SDL/wasm front-ends and a headless verifier share it.
- `shell.rs` (title loop, mode table, pause, key redefinition hand-off), `timing.rs` (frame pacer;
  simple deadline scheduler is enough), `state_hash.rs` (MD5 stream, `trait HashState { fn append(&self, &mut Hasher) }`
  implemented by field/digger/monster/bags), `log.rs`.

Trickiest parts to port:

1. **Blocking control flow → state machine.** `game()`, `flashplayer`, `run_pause`, `getinitials`,
   `redrawdelay` inside drawing loops, `soundlevdone`, and `/P` playing a whole game from inside the
   argument parser all block and call `newframe()` from nested loops. Tick counts (`ticks`) versus
   frame counts must stay as in C.
2. **Global mutable state with index aliasing**: `curplayer` doubles as digger/score index offset
   (`for i in cur..cur+diggers`), per-player `field1/2`, emerald bit masks, `bagdat1/2`.
3. **Drawing that mutates the simulation** (`drawfield`), and `addscore`/draw calls that feed `penalty`.
   Headless mode cannot simply skip "drawing" functions.
4. **Hash compatibility** — exact field order/width, the optional monster-object block, hashing after
   the same cleanup steps, checkpoint cadence relative to `ticks++`.
5. **Deliberate wrap/truncation** (section 9), especially `cgtime` and the i16 score argument.
6. **Deciding on the accidental bugs** in 10.18 (reproduce, fix, or fix-with-flag) and documenting each.
7. **Score file / level file dual storage** and the lenient 512-byte blob parsing.
