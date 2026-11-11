# 02 - Entity gameplay logic (Digger, fireballs, monsters, bags, collisions)

Source: `https://github.com/sobomax/digger` (HEAD 638125e).
Files read completely: `digger.c/.h`, `digger_obj.c/.h`, `bullet_obj.c/.h`, `monster.c/.h`, `monster_obj.c/.h`,
`bags.c/.h`, `sprite.c/.h`, `drawing.c/.h`, `digger_types.h`, `def.h`; plus the relevant parts of `main.c`,
`scores.c`, `input.c`, `sdl_vid.c` that this area calls.

All line references are `file:line`. "Tick" = one iteration of the main game loop (12.5 Hz at default speed).

---------------------------------------------------------------------------------------------------

## 0. Shared constants and coordinate system

`def.h:23-52`

| Name | Value | Notes |
|---|---|---|
| `DIR_NONE` | -1 | |
| `DIR_RIGHT` | 0 | |
| `DIR_UP` | 2 | |
| `DIR_LEFT` | 4 | |
| `DIR_DOWN` | 6 | Directions are even numbers; `embox[dir]`, `embox[dir+1]` index pairs (digger.c:954) |
| `TYPES` | 5 | collision classes: 0 bonus, 1 bag, 2 monster, 3 fireball, 4 digger |
| `BONUSES` | 1 | sprite 0 |
| `BAGS` | 7 | sprites 1..7 (`FIRSTBAG`=1) |
| `MONSTERS` | 6 | sprites 8..13 (`FIRSTMONSTER`=8) |
| `FIREBALLS` | 2 | sprites 14..15 (`FIRSTFIREBALL`=14) |
| `DIGGERS` | 2 | sprites 16..17 (`FIRSTDIGGER`=16) |
| `SPRITES` | 18 | index 18 is the extra "misc" sprite slot (arrays are `SPRITES+1`) |
| `MWIDTH`x`MHEIGHT` | 15 x 10 | `MSIZE`=150 |

Grid to pixel: cell (h,v) has top-left pixel `x = h*20 + 12`, `y = v*18 + 18`.
Playfield pixel limits for every entity: `x in [12, 292]`, `y in [18, 180]`.
Inverse (used everywhere): `h=(x-12)/20`, `xr=(x-12)%20`, `v=(y-18)/18`, `yr=(y-18)%18` (C truncating division; operands
are always non-negative here).
Step sizes: horizontal 4 px (5 steps per cell), vertical 3 px (6 steps per cell) for Digger and monsters;
falling bags 6 px (3 steps per cell); pushed bags 4 px; fireballs 8 px horizontal / 7 px vertical.

`reversedir` (digger.c:895): R<->L, U<->D, anything else (incl. -1) is returned unchanged.

### The `field[]` tunnel map (drawing.c:22, 65-81, 144-185)

`int16_t field[150]`, one word per cell, bit set = earth still present:

- bits 0..4 (`0x001f`): five horizontal 4-px slices of the cell
- bit 5 (`0x0020`): unused, always set (hence the `0xfdf` mask that skips it)
- bits 6..11 (`0x0fc0`): six vertical 3-px slices of the cell
- bit 13 (`0x2000`): "cell completely untouched" flag; cleared as soon as all horizontal bits or all vertical bits are clear
- bits 12, 14, 15: always set (initial value is -1 = 0xffff)

`makefield` (drawing.c:65): starts at -1; level char `S` or `V` -> `&= 0xd03f` (vertical tunnel);
`S` or `H` -> `&= 0xdfe0` (horizontal tunnel). Per-player copies `field1/field2`.

`eatfield(x,y,dir)` (drawing.c:144) - **always calls `incpenalty()` first, even for `DIR_NONE`**:
```
h=(x-12)/20; xr=((x-12)%20)/4; v=(y-18)/18; yr=((y-18)%18)/3;
RIGHT: h++;                       field[v][h] &= bitmasks[xr];   if (field&0x1f)==0  field &= 0xdfff
UP:    yr--; if yr<0 {yr+=6; v--} field[v][h] &= bitmasks[6+yr]; if (field&0xfc0)==0 field &= 0xdfff
LEFT:  xr--; if xr<0 {xr+=5; h--} field[v][h] &= bitmasks[xr];   if (field&0x1f)==0  field &= 0xdfff
DOWN:  v++;                       field[v][h] &= bitmasks[6+yr]; if (field&0xfc0)==0 field &= 0xdfff
```
`bitmasks[i] = ~(1<<i)` for i=0..11 (drawing.c:27). So a cell's bits are cleared by whoever is *entering* it;
x/y passed is the position *before* the move. No bounds checks: callers guarantee the mover is not at the edge
(Digger/monster `dir` is forced to NONE at the edge; bags do not fall when `y==180`).

Predicates used by game logic:
- "cell has any tunnel in it": `(field & 0xfdf) != 0xfdf` (bags.c:192, 204, 219)
- "cell is untouched": `(field & 0x2000) != 0` (monster.c:552..571, bags.c:144)
- `fieldclear(dir,x,y)` (monster.c:546-575) - can a nobbin go from cell (x,y) in `dir`:
  - RIGHT: `x<14` && target `(x+1,y)` has 0x2000 clear && (target bit0 clear || own bit4 (0x10) clear)
  - UP:    `y>0`  && target `(x,y-1)` 0x2000 clear && (target bit11 (0x800) clear || own bit6 (0x40) clear)
  - LEFT:  `x>0`  && target `(x-1,y)` 0x2000 clear && (target bit4 clear || own bit0 clear)
  - DOWN:  `y<9`  && target `(x,y+1)` 0x2000 clear && (target bit6 clear || own bit11 clear)
  - any other dir (incl. NONE): false

Blob drawing (`drawrightblob` etc., drawing.c:266-306) is graphics only, **but** it changes framebuffer pixels
which the fireball logic reads (section 3.3). Offsets: right blob at (x+16,y-1) 8x18; left (x-8,y-1) 8x18;
top (x-4,y-6) 24x6; bottom (x-4,y+15) 24x6; furry (x-4,y+15) 24x8; square (x-4,y+17) 24x6.

---------------------------------------------------------------------------------------------------

## 1. Collision detection (sprite.c) - exact semantics that logic depends on

Per-sprite state (sprite.c:12-27): enabled flag `sprenf`, current `x,y,wid,hei,bwid,bhei,ch` and "next"
`nch,nwid,nhei,nbwid,nbhei` (set by `initspr`, latched on the next draw). Width is in 4-pixel units.

- `initspr(n,ch,wid,hei,bwid,bhei)` (sprite.c:153): only sets the "next" values.
- `movedrawspr(n,x,y)` (sprite.c:89): `x &= -4`, latch next->current, enable. **No collision computation.**
- `erasespr(n)` (sprite.c:107): disable (no-op if already disabled). **No collision computation, `first/coll` untouched.**
- `drawspr(n,x,y)` (sprite.c:118): `x &= -4`, latch, enable, then `bcollides(n)` -> **the only producer of collision lists.**

`bcollides(spr)` (sprite.c:281-299): resets global `first[5] = -1`, `coll[18] = -1`; for each type i in 0..4, scans
sprite indices `firstt[i]..lastt[i]-1` ascending; every *enabled* sprite != spr that `bcollide`s is appended:
`first[i]` = lowest colliding index of that type, `coll[idx]` = next colliding index of the same type, -1 terminated.

`bcollide(bx,si)` (sprite.c:242-259) with border sizes; **all game sprites use bwid=bhei=0** (every `initspr`/`createspr`
call in drawing.c and monster_obj.c passes 0,0), so it reduces to an inclusive AABB test:
```
x-overlap: !(bx.x >= si.x ? bx.x > si.x + si.wid*4 - 1 : si.x > bx.x + bx.wid*4 - 1)
y-overlap:  (bx.y >= si.y ? bx.y <= si.y + si.hei - 1 : si.y <= bx.y + bx.hei - 1)
```
Sizes (drawing.c:187-264, monster_obj.c:106-137): bag 16x15, monster 16x15 (dead left-facing hobbin: **16x14**,
monster_obj.c:133), digger 16x15, bonus 16x15, fireball 8x8. Sizes of *other* sprites are their last drawn sizes.

Rules game logic relies on:
1. `first/coll` are globals overwritten by every `drawspr`; callers copy them into local `clfirst/clcoll`
   immediately after the draw (digger.c:471-474, 645-648, 746-749; monster.c:391-394; bags.c:244-247, 272-275,
   313-316, 332-335). Exception: `getgold` (bags.c:453) reads the *globals* right after its own `drawgold`.
2. A list reflects positions at the instant of that draw: entities updated earlier in the tick are at new
   positions, later ones at old positions.
3. Lists contain sprites that are *enabled*, not necessarily "alive": zombie monsters, tombstones and exploding
   fireballs are in lists; a blinking invincible Digger that was just `erasespr`'d (digger.c:322) is not.
4. `drawdigger` with an invalid frame `t` (not 0/2/4/6 and not 10..15) draws nothing and sets `first[0..4] = -1`
   but leaves `coll[]` alone (drawing.c:358).
5. `x & -4` alignment: all gameplay x values are already multiples of 4.

Type index -> entity index mapping: bag `i-FIRSTBAG`; monster `i-FIRSTMONSTER`; fireball owner
`i-FIRSTFIREBALL+curplayer`; digger `i-FIRSTDIGGER+curplayer` (digger/fire sprites are numbered from 0 for the
current player, `digdat[]` is indexed from `curplayer`).

**The sprite layer must be ported as state (positions/sizes/enabled) even in a headless build**; collisions are
not derivable from entity structs alone (e.g. sprite stays at last drawn position when an entity struct is reverted).

---------------------------------------------------------------------------------------------------

## 2. The "object" layer (digger_obj, bullet_obj, monster_obj)

A half-finished OO refactor: C structs with function-pointer members called through
`CALL_METHOD(obj, method, args...)` = `(obj)->method(obj, args)` (digger_types.h:40). Only one implementation
of each method exists; there is no polymorphism. Gameplay state is split between these objects and the older
parallel structs (`digdat[]`, `mondat[]`).

### digger_obj (digger_obj.h:35-49, digger_obj.c)
Fields: `d_id` (sprite slot 0/1), `alive`, `zombie` (never used), `can_fire`, `dir`, `x`, `y`.
Methods: `put` (movedrawspr FIRSTDIGGER+d_id; .c:40), `animate` (`drawdigger(d_id, dir, x, y, can_fire)`; .c:48),
`discharge` (assert can_fire; can_fire=false; .c:56), `recharge` (assert !can_fire; can_fire=true; .c:65).
`damage`/`kill` slots exist but are never assigned (NULL). `digger_obj_init` zeroes, sets alive=true, can_fire=true (.c:74).

Frame selection `drawdigger(n,t,x,y,f)` (drawing.c:338-359): per-digger ping-pong counter `digspr` 0,1,2,1,0...
(`digspr += digspd; if digspr==2||digspr==0 digspd=-digspd`, clamped to 0..2), advanced on **every** call including
death frames. `nn = n==0 ? 0 : 31`.
- t in {0,2,4,6}: sprite `(t + (f?0:1))*3 + digspr + 1 + nn` (f = fireball available)
- t in 10..15: sprite `40 + nn - t` (15 -> 25 "dead digger", 14..10 -> 26..30 tombstone growth)
- otherwise: nothing drawn, `first[]` cleared.
`digspr/digspd` reset by `initmbspr/creatembspr` (drawing.c:249-256). Visual only (no size change).

### bullet_obj (bullet_obj.h:35-45, bullet_obj.c)
Fields: `f_id`, `expsn` (0 = flying, 1..3 = explosion frame to draw next, 4 = finished), `dir`, `x`, `y`.
- `put` (.c:41): movedrawspr + `soundfire(f_id)`
- `animate` (.c:49): assert expsn<4; `drawfire(f_id,x,y,expsn)`; if expsn>0 { if expsn==1 soundexplode; expsn++ }
- `remove` (.c:65): erasespr; if expsn>1 soundfireoff; expsn=0
- `explode` (.c:77): expsn=1 (idempotent restart; the assert is commented out)
`drawfire` (drawing.c:317): t==0: `firespr` cycles 1,2,0,1,...; sprite `82+firespr(+32 for n=1)`; t>0: sprite
`84+t(+32)`. Always 8x8, then `drawspr` (collision lists produced).
Note `digdat[].bob` is memset to 0 at `initdigger` (digger.c:123): method pointers are NULL until the first shot.

### monster_obj (monster_obj.h:40-55, monster_obj.c)
Heap object (`malloc`) with private part (monster_obj.c:45-54): `m_id`, `nobf` (true = nobbin), `alive`, `zombie`,
`pos {dir,x,y}`, `monspr`, `monspd`. Ctor (.c:270): alive=true, zombie=false, monspr=0, monspd=1.
- `put` (.c:140): update sprite def, movedrawspr
- `mutate` (.c:152): toggle `nobf`, update sprite def, `drawspr` (clobbers `first/coll`; callers ignore)
- `damage` (.c:165): zombie=true, alive=false, redraw with `drawspr` (dead frame)
- `kill` (.c:183): alive=false, zombie=false, erasespr
- `animate` (.c:200): alive -> `__drawmon` (advance ping-pong `monspr` 0..2 exactly like digspr, then drawspr);
  zombie -> `__drawmondie` (drawspr without advancing); neither -> nothing (lists NOT refreshed)
- `getpos/setpos` (.c:215/223): copy `obj_position`
- `isalive`, `isnobbin`, `dtor` (free)
Sprite selection `monster_obj_updspr` (.c:106-137): alive nobbin `69+monspr`; alive hobbin `73+monspr` if
`pos.dir==RIGHT`, `77+monspr` if LEFT (no change for other values); zombie nobbin 72; zombie hobbin right 76;
zombie hobbin left 80 with **height 14** instead of 15 (affects collisions).
`pos.dir` is only ever RIGHT or LEFT (initial LEFT; monster.c:318-321 writes it only for horizontal moves).

---------------------------------------------------------------------------------------------------

## 3. Digger

### 3.1 State (digger.c:30-39)

```c
struct digger {
  int16_t h,v,rx,ry,      // cell and remainder, recomputed at end of updatedigger
          mdir,           // direction actually moved this tick (or NONE)
          bagtime,        // ticks to pause after pushing a bag
          rechargetime,   // fireball recharge countdown
          deathstage,     // enum 1..6
          deathbag,       // bag index that killed it
          deathani, deathtime,
          emocttime,      // emerald-octave window countdown
          emn,            // consecutive emeralds so far (0..7)
          msc,            // monster-eating score multiplier (1,2,4,...)
          lives, ivt;     // ivt = invincibility ticks left
  bool notfiring, firepressed(unused), dead, levdone(unused), invin;
  uint16_t deathmusicdone; int16_t dirgetime;   // dirge wait (see 3.6)
  struct digger_obj dob;  // x,y,dir(facing),alive,can_fire
  struct bullet_obj bob;  // fireball
} digdat[DIGGERS];
```
Module globals: `startbonustimeleft`, `bonustimeleft` (int16), `emmask`, `int8_t emfield[150]` (bit per player),
`bonusvisible`, `bonusmode`, `digvisible`, `deathffwd` (timing only).

Three "liveness" notions: `dob.alive` (controllable), `dead` (death sequence finished), `lives`.
`isalive()` (digger.c:1036) = any current-player digger with `dob.alive`.

### 3.2 Init (`initdigger`, digger.c:75-131) - once per life/round
Loop `dig = curplayer .. curplayer+diggers-1`. If `lives==0`: mark dead, `deathstage=DONE`, erase sprites, skip.
Else: `v=9`, `mdir=4` (LEFT, for both), `h = diggers==1 ? 7 : 8-dig*2` (two-digger: digger0 col 8, digger1 col 6),
facing `dig==0 ? RIGHT : LEFT`, `rx=ry=0`, `bagtime=0`, `dead=false`, `invin=false`, `ivt=0`,
`deathstage=DGR_DEATH_BAG(1)`, `notfiring=true`, `emocttime=0`, bullet zeroed, `rechargetime=0`, `emn=0`, `msc=1`,
obj init + `put` at (h*20+12, 180). Then `digvisible=true; bonusvisible=bonusmode=false`.
Quirk: in alternating two-player mode player 2 is `dig==1` with `diggers==1` -> column 7 but **facing LEFT**,
using sprite slot 0.

### 3.3 Per-tick: `dodigger` (digger.c:326-397)

```
if escape||timeout return
gauntlet: drawlives; if cgtime<80000 timeout=true; cgtime-=80000 (uint32, wraps on the last tick)
for n in curplayer..curplayer+diggers-1:
    readdirect(n-curplayer)                      // samples keys (input layer)
    if bob.expsn!=0  drawexplosion(n) else updatefire(n)
    if digvisible:
        if dob.alive:
            if bagtime!=0: { draw with dir=mdir (then restore dob.dir); incpenalty; bagtime-- }
            else updatedigger(n)
        else diggerdie(n)
    if emocttime>0 emocttime--
bonus-mode timers (3.8)
```
Fire is processed *before* movement for each digger; digger 0 completely before digger 1.

### 3.4 Movement: `updatedigger` (digger.c:582-697)

1. `dir = getdirect(n-curplayer)` - **this is the call that consumes one direction record from a playback /
   writes one to a recording** (input.c:536-563). It happens only on ticks when the digger is alive, visible and
   `bagtime==0`. Getting this call pattern exactly right is essential for DRF playback.
2. `ddir = dir` if one of the four directions, else NONE.
3. Turning rules (use `rx/ry` stored at the end of the previous update):
   - `rx==0 && ddir in {UP,DOWN}` -> `dob.dir = mdir = ddir`
   - `ry==0 && ddir in {RIGHT,LEFT}` -> `dob.dir = mdir = ddir`
   - then `mdir = (dir==NONE) ? NONE : dob.dir`
   So: with no key the digger stops instantly (even mid-cell); with a key held that cannot be obeyed yet
   (perpendicular while off-grid) it keeps moving in its current facing until aligned, then turns. Reversal
   along the current axis is always immediate.
4. Edge stop: `(x==292 && R) || (x==12 && L) || (y==180 && D) || (y==18 && U)` -> `mdir=NONE` (facing kept).
5. Save `diggerox,diggeroy`. If `mdir!=NONE`: `eatfield(ox,oy,mdir)` (1 penalty).
6. Draw tunnel blob for mdir and move: R `x+=4`, U `y-=3`, L `x-=4`, D `y+=3`.
7. Emerald test at the *new* position (3.5).
8. `drawdig(n)` (digger.c:313): `animate` (drawspr -> collision lists) then invincibility blink:
   `if invin { ivt--; if ivt==0 invin=false; else if ivt%10<5 erasespr(digger sprite) }`.
9. Copy `first/coll`; `incpenalty()`.
10. Bags: walk `clfirst[1]`; `bagf=true` at the first bag with `bagexist(bag)` unless
    `(invin && getbagdir(bag)==DIR_DOWN)` (invincible diggers ignore falling bags). If `bagf`:
    - `mdir` R or L: `push = pushbags(mdir, clfirst, clcoll)`; `bagtime++` (always, success or not)
    - else (U, D **or NONE**): `if !pushudbags(...) push=false`
    - if `!push`: restore x,y; `dob.dir=mdir; drawdig(n); incpenalty(); dob.dir=reversedir(mdir)` (digger bounces:
      faces the opposite way). Note this second `drawdig` also decrements `ivt` a second time.
11. `if clfirst[2]!=-1 && bonusmode && dob.alive`: `nmon = killmonsters(clfirst,clcoll)`; for each: `soundeatm();
    sceatm(n)` (3.8). **Outside bonus mode a digger moving into a monster does nothing here** - the digger is only
    killed from the monster's own `monai` (4.5).
12. `if clfirst[0]!=-1`: `scorebonus(n)` (1000) and `initbonusmode()`.
13. Recompute `h,rx,v,ry` from `dob.x,dob.y`.

Effect of `bagtime`: pushing a bag halves the digger's speed (move tick, pause tick). `digresettime(n)` (called
when gold is collected) zeroes it.

Quirk (push with `mdir==NONE`): if a stationary digger overlaps a non-gold bag (possible e.g. for an invincible
respawned digger a bag has fallen through and landed on with fall height <=1), `pushudbags` fails, `dob.dir` becomes
`reversedir(-1) = -1`. The digger then draws nothing (invalid frame) and has empty collision lists until a new
direction is accepted; **firing with `dob.dir==-1` hits `default: abort()` (digger.c:434)**. Rare, two-digger only;
decide explicitly how to treat it (suggest: do not fire, document deviation).

### 3.5 Emeralds and the 8-in-a-row bonus

`emfield[y*15+x]` bit `emmask = 1<<curplayer`; `makeemfield` sets it where the level char is `'C'` (digger.c:930).
`hitemerald(x,y,rx,ry,dir)` (digger.c:956-983), `embox = {8,12, 12,9, 16,12, 6,9}`:
```
if dir not R/U/L/D return false
if dir==RIGHT && rx!=0 x++;   if dir==DOWN && ry!=0 y++;
r = (dir horizontal) ? rx : ry
if emfield[y][x] has emerald:
    if r==embox[dir]:   redraw emerald, incpenalty            (R:8  U:12 L:16 D:6)
    if r==embox[dir+1]: erase emerald, incpenalty, clear bit, return true   (R:12 U:9 L:12 D:9)
```
(x,y,rx,ry) are derived from the mover's new pixel position. For LEFT/UP truncating division already yields the
cell being entered. An emerald is eaten 3 steps into the cell regardless of direction.
`countem()` counts remaining; `killemerald(x,y)` (digger.c:995) silently removes the emerald in cell **(x, y+1)**
(used by falling bags).

On a hit (digger.c:628-643): `if emocttime==0 emn=0; scoreemerald (25); soundem(); soundemerald(emn); emn++;
if emn==8 { emn=0; scoreoctave (250) }; emocttime=9`.
`emocttime` is decremented at the end of the same tick (digger.c:363), so the next emerald must be eaten within 8
ticks to continue the run (adjacent emeralds are 5 ticks apart horizontally, 6 vertically). The eighth
consecutive emerald pays 25+250.

Hobbins call `hitemerald` too (monster.c:356) and destroy emeralds with no score - this counts toward level
completion (`countem()==0`).

### 3.6 Firing (digger.c:399-553, 563-580)

State machine per digger: `notfiring` + `rechargetime` + `bob.expsn`.

Ready -> fire (in `updatefire` when `notfiring`):
- `if rechargetime!=0 { rechargetime--; if ==0 dob.recharge() }`
- `else if getfirepflag(n-curplayer) && dob.alive`: `discharge`; `rechargetime = levof10()*3 + 60`;
  `notfiring=false`; spawn bullet with `dir = dob.dir` at
  R `(x+8, y+4)`, U `(x+4, y)`, L `(x, y+4)`, D `(x+4, y+8)`; `put` (no collision test this tick).
  Fire input is `input_get_fire_active`, the fire flag latched at frame start (`input_advance_fire_state`,
  digger.c:280) - i.e. the flag produced by the previous tick's `getdirect/readdirect`.
- The recharge countdown only runs while `notfiring`, so effective cooldown =
  flight ticks + 4 explosion ticks + `levof10*3+60`.

Flying (each tick, `expsn==0 && !notfiring`):
1. Move and sample the **framebuffer** (`ggetpix`):
   - R: `x+=8`; `pix = getpix(x,y+4) | getpix(x+4,y+4)`
   - U: `y-=7`; `pix = OR_{i=0..6} getpix(x+4,y+i)`; `pix &= 0xc0`
   - L: `x-=8`; `pix = getpix(x,y+4) | getpix(x+4,y+4)`
   - D: `y+=7`; `pix = OR_{i=0..6} getpix(x,y+i)`; `pix &= 0x3`
   `vgagetpix(x,y)` (sdl_vid.c:649-673): returns 0xff if `x>319||y>199`; else examines the 8x2 hi-res pixels that
   correspond to virtual pixels x..x+3 on row y, sets bit `0x80>>xi` for each non-zero (non-black) hi-res column
   xi=0..7, and masks with `0xee`. So: horizontal shots test an 8-virtual-pixel strip on row y+4 (minus the
   right half of virtual pixels x+1, x+3, x+5, x+7); UP tests only virtual pixel x+4 on 7 rows; DOWN tests only
   the left hi-res half of virtual pixel x+3 on 7 rows (`0x3 & 0xee = 0x02`). "Earth" = any non-black pixel
   (background pattern, emeralds, text...). **Game logic depends on rendered pixels here** - coordinate with the
   graphics agent: the port needs a pixel-exact virtual framebuffer (or at least a pixel-exact non-zero mask) even
   for headless playback. CGA mode uses a different `cgagetpix`.
   Note the sampling happens before the fireball sprite is redrawn; for DOWN the sampled rows start at old y+7,
   which is the last row of the still-drawn old fireball sprite.
2. `bob.animate` (drawspr -> lists), copy lists, `incpenalty()`.
3. Monsters: for every `i` in `clfirst[2]` list: `killmon(i-FIRSTMONSTER)`; `scorekill(n)` (+250); `explode`.
4. Diggers: for every `i` in `clfirst[4]` list with `dig = i-FIRSTDIGGER+curplayer != n && !invin && dob.alive`:
   `killdigger(dig,3,0)`; `explode` (friendly fire in two-digger mode).
5. `clflag = any of clfirst[0..4] != -1` (includes the firer's own sprite, tombstones, etc.).
6. If `clfirst[0]`, `[1]` or `[3]` != -1 (bonus, bag, other fireball): `explode`; every fireball in the `[3]` list
   whose `expsn==0` also explodes.
7. Range/earth: R `x>296`, U `y<15`, L `x<16`, D `y>183` -> `explode`; else `if pix!=0 && !clflag`: step back
   (undo the 8/7 px), `animate` again (redraw at the old position, clobbering lists), `explode`.

Explosion (`drawexplosion`, digger.c:563): each tick with `expsn!=0`: if `expsn<4`: `animate` (draws frame
`84+expsn`, expsn++), `incpenalty`; else `killfire(n)`: `notfiring=true`, `remove` (erase, expsn=0). So frames at
expsn 1,2,3 then removal on the 4th tick; recharge countdown starts the tick after.
An exploding fireball remains an enabled 8x8 sprite (other fireballs hitting it explode; `explode` on an already
exploding one restarts it at 1).
`killfire` is also called for every current digger at round end (main.c:266).

### 3.7 Death (digger.c:705-857, 918-928)

`killdigger(n, stage, bag)`: ignored if `invin`; applied only if `deathstage < 2 || deathstage > 4` (i.e. not
already in ANIM/MONSTER/TOMBSTONE): `dob.alive=false; deathstage=stage; deathbag=bag`. (A digger in stage
BAG=1 or ARC=5 can be re-killed and restart with a new stage.)

`diggerdie(n)` runs once per tick while `!dob.alive && digvisible`; first `if dirgetime!=0 dirgetime--`. Stages:

| Stage | Behaviour |
|---|---|
| 1 `DGR_DEATH_BAG` | `if bagy(deathbag)+6 > y: y = bagy+6`; draw frame 15; `incpenalty`; `if getbagdir(deathbag)+1==0` (bag stopped - dir NONE is -1 - or bag gone): `soundddie; deathtime=5; stage=ANIM; deathani=0; y-=6` |
| 3 `DGR_DEATH_MONSTER` | (monster or fireball) `stage=ARC; deathani=0; deathtime=0` (one tick, nothing drawn) |
| 5 `DGR_DEATH_ARC` | 7 ticks, `deatharc={3,5,6,6,5,3,0}`: draw frame 15 at `y - deatharc[deathani]`; `if deathani==6 && !isalive() musicoff`; `incpenalty`; `deathani++`; `if ==1 soundddie`; `if ==7 { deathtime=5; deathani=0; stage=ANIM }` |
| 2 `DGR_DEATH_ANIM` | `if deathtime!=0 { deathtime--; break }`. `if deathani==0`: start dirge, `dirgetime=DIRGE_TICKS(89)`. Draw frame `14-deathani` (tombstone growing), copy lists, `incpenalty`; `if deathani==0 && clfirst[2]!=-1 killmonsters(...)` (monsters touching the fresh grave are removed, **no score**); `if deathani<4 { deathani++; deathtime=2 } else { stage=TOMBSTONE; deathtime=60 }`. Five frames, 3 ticks apart. |
| 4 `DGR_DEATH_TOMBSTONE` | `deathffwd = ...` (real-time skipping only). `if deathtime!=0 deathtime--` (60 ticks) else resolution below |
| 6 `DGR_DEATH_DONE` | nothing |

Resolution at the end of TOMBSTONE (digger.c:795-852):
- one-digger mode: `if dirgetime!=0 break` (keep waiting, tick-counted: 89 ticks from the first ANIM frame, i.e.
  17 more ticks after the 12+60); `dirgehold(n)` is a real-time wait only.
- `dead=true`; `alldead = all digdat[i].dead for i in 0..diggers-1` (**absolute indices 0.., not curplayer-based**;
  works for alternating player 2 only because player 1's `digdat[0].dead` is always true by then).
- `alldead` -> `setdead(true)` (main loop ends the round; lives are decremented in main.c:298-305).
- else `if isalive() && lives>0` (two-digger mode, partner alive): `lives--` (unless gauntlet), `drawlives`; if
  `lives>0`: **respawn in place**: `v=9, mdir=4, h=8-n*2, x=h*20+12, y=180, dir=(n==0?R:L), rx=ry=0, bagtime=0,
  alive=true, dead=false, invin=true, ivt=50, deathstage=BAG, notfiring=true, emocttime=0, bob.expsn=0,
  rechargetime=0, emn=0, msc=1`, `erasespr` + `put`; (note `can_fire` is NOT reset). Then `clearfire(n)` and
  music restart. If lives hit 0 the digger stays in TOMBSTONE with `dead=true`; next tick it falls to the
  `else` branch below.
- else: erase sprite, `deathstage=DONE`, `clearfire(n)`.

Invincibility: 50 `drawdig` calls; blink (sprite erased when `ivt%10<5`); cannot be killed (`killdigger` returns),
ignores falling bags, is ignored by `checkdiggerunderbag`, cannot be shot.

The tombstone is the digger sprite (enabled): monsters colliding with it see `clfirst[4]!=-1` but skip it because
`!digalive`.

### 3.8 Bonus mode (cherry)

- Appearance (monster.c:96-100): once per round, in `domonsters`, when `unbonusflag && nextmonster==totalmonsters
  && nextmontime==0 && isalive()` -> `createbonus()`: `bonusvisible=true; drawbonus(292,18)` (sprite 0 at the
  top-right cell, `movedrawspr`). Happens `mongaptime+1` ticks after the last monster of the round has spawned.
  `unbonusflag` is reset to true by `initmonsters` (every life).
- Pickup: digger collision `clfirst[0]!=-1` -> +1000, `initbonusmode` (digger.c:865): `bonusmode=true`,
  `erasebonus`, `bonustimeleft = 250 - levof10()*20`, `startbonustimeleft = 20`, `msc=1` for `digdat[0..diggers-1]`.
- Timer (end of `dodigger`, digger.c:366-396), runs in the same tick as the pickup:
```
if bonusmode && isalive():
    if bonustimeleft!=0:
        bonustimeleft--
        if startbonustimeleft!=0 || bonustimeleft<20:
            startbonustimeleft--            // goes negative during the final-20 flashing; harmless
            flash palette intensity by (bonustimeleft&1); soundbonus
            if startbonustimeleft==0: music(BONUS); soundbonusoff; ginten(1)
    else: endbonusmode; soundbonusoff; music(MAIN)
if bonusmode && !isalive(): endbonusmode ...
```
  Duration = `bonustimeleft+1` ticks while a digger is alive (ends immediately when all are dead).
- Eating: via digger move (3.4 step 11: every monster in the list is killed, one `sceatm` each) or via the
  monster's move (4.5). `sceatm(n)` (digger.c:699): `scoreeatm(n, msc)` = `msc*200`, then `msc <<= 1`.
  Progression 200, 400, 800, 1600, 3200, 6400... `msc` resets only in `initbonusmode`, `initdigger`, respawn.
- In bonus mode `killmon` does `totalmonsters++` (monster.c:590) for *any* kill (eaten, shot, crushed), and no
  monsters spawn while `bonusmode`; so those monsters come back after bonus mode ends.
- Monsters flee (4.4).

### 3.9 Extra lives (scores.c:205-232; scores agent owns the detail)
`addscore(n, int16 score)`: after adding, `if score >= nextbs + n` ("+n to reproduce original bug"): if
`getlives(n)<5 || unlimlives` -> `addlife(n)` (gauntlet: +15 s instead); `nextbs += 20000`.
**Every `addscore` call does `incpenalty()` three times** - scoring events slow monsters (section 6).
`initlives` (digger.c:1057): 3 lives for `digdat[0 .. diggers+nplayers-2]`.

### 3.10 Two-digger simultaneous mode differences (summary)
- Start columns 8 (digger 0, facing right) and 6 (digger 1, facing left); sprite sets offset by 31 (fire: 32).
- Both updated every tick, digger 0 first (fire then move), so digger 0 has priority in races.
- Fireballs kill the other digger; fireballs collide with each other.
- Death: dirge at 0.7 volume without the 89-tick wait; respawn in place after the tombstone with 50 ticks of
  invincibility while the partner is alive; life decremented at that point. Round ends only when both are `dead`.
- Monsters alternate their chase target at spawn (`chase` toggles), falling back to the other digger when the
  target is not alive (monster.c:225-229).
- Bag-kill score split: `scorekill2` 125/125 (or 250 to the one with lives left) - **two `addscore` calls = 6
  penalties**, even with 0 points.
- Gold: every alive digger overlapping the gold scores 500.
- Bonus mode eaten monster via `monai`: each alive colliding digger gets its own `sceatm`.
- Lives display / score display for player index 1.

---------------------------------------------------------------------------------------------------

## 4. Monsters (monster.c)

### 4.1 State (monster.c:22-32)
```c
struct monster {
  int16_t h,v,xr,yr,   // cell + remainder (recomputed at end of monai)
          dir,         // movement direction (may be DIR_NONE)
          t,           // delay ticks before next move
          hnt,         // nobbin: "crossings" counter; hobbin: time as hobbin
          death,       // 1 = being carried down by bag, 4 = final delay
          bag,         // bag that squashed it
          dtime,       // death delay
          stime,       // start delay (moves suppressed)
          chase;       // digdat index to chase
  bool flag;           // slot in use
  struct monster_obj *mop;  // pos{x,y,hdir}, alive, zombie, nobbin flag, anim
} mondat[6];
int16_t nextmonster, totalmonsters, maxmononscr, nextmontime, mongaptime, chase;
bool unbonusflag;  bool mongotgold;
```

### 4.2 Level parameters and spawn schedule (`initmonsters`, monster.c:44-77; every life)
- `mongaptime = 45 - 2*levof10()` (43, 41, ..., 25)
- `totalmonsters = levof10() + 5` (6..15)
- `maxmononscr`: level 1 -> 3; levels 2-7 -> 4; levels 8+ -> 5
- `nextmontime = 10`, `nextmonster = 0`, `chase = 0`, `unbonusflag = true`; all slots cleared.

`domonsters` head (monster.c:87-101):
```
if nextmontime>0: nextmontime--
else:
    if nextmonster<totalmonsters && nmononscr()<maxmononscr && isalive() && !bonusmode: createmonster()
    if unbonusflag && nextmonster==totalmonsters && nextmontime==0 && isalive(): unbonusflag=false; createbonus()
```
First monster on the 11th tick, then one every `mongaptime+1` ticks while fewer than `maxmononscr` slots have
`flag` (zombies count). `createmonster` (monster.c:165): first free slot; `t=0, hnt=0, h=14, v=0, xr=yr=0,
dir=LEFT, chase = chase+curplayer; chase=(chase+1)%diggers; nextmonster++; nextmontime=mongaptime; stime=5`;
new nobbin object at (292,18); `put`.
`monleft() = nmononscr() + totalmonsters - nextmonster` (level is done when 0).

### 4.3 Per-tick scheduling and speed (monster.c:102-121)
For each slot i = 0..5 with `flag`:
```
if hnt > 10 - levof10() && isnobbin: mutate (-> hobbin); hnt=0        // checked every tick, even for zombies
if mop.isalive:
    if t==0:
        monai(i)
        if randno(15 - levof10()) == 0:            // RNG always consumed here
            if isnobbin && isalive: monai(i)       // second move this tick
    else t--
else mondie(i)
```
Base speed equals Digger's (4/3 px per tick). Speed scaling by level is only: (a) nobbins get a double move with
probability `1/(15-lev)` (1/14 at level 1 ... 1/5 at level 10); (b) `t` delays: +1 on each direction change,
+1 on colliding with a monster, +1 (+1 more if horizontal) on touching a bag, reset to 0 on eating gold;
(c) global penalty: `incmont(penalty-8)` (section 6).

### 4.4 `monai` in detail (monster.c:200-512)

A. Read `mopos`; save `monox,monoy`.

B. **Only when `xr==0 && yr==0`** (on a cell corner; uses values stored at the end of the previous `monai`):
1. Hobbin timeout: `if hnt > 30 + 2*levof10() && ishobbin: hnt=0; mutate` (back to nobbin).
2. Target: `dig = chase; if !digalive(dig) dig = 2*curplayer + (diggers-1) - dig`.
3. Priorities p1..p4 (dx = diggerx-monx, dy = diggery-mony):
   - if `abs(dy) > abs(dx)`: p1 = vertical toward (`diggery<mony ? UP : DOWN`), p4 = its opposite;
     p2 = horizontal toward (`diggerx<monx ? LEFT : RIGHT`), p3 = its opposite
   - else: p1 = horizontal toward, p4 = opposite; p2 = vertical toward, p3 = opposite
   - ties resolve to DOWN / RIGHT (strict `<` tests)
4. Bonus mode: swap p1<->p4 and p2<->p3 (flee).
5. No-reverse rule: `dir = reversedir(mondat.dir)`; three sequential (not else-if) tests: if `dir==p1` rotate
   p1<-p2<-p3<-p4<-dir; if `dir==p2` rotate p2<-p3<-p4<-dir; if `dir==p3` swap into p4. (Reverse ends up last.)
6. Random element: `if randno(levof10()+5) == 1` **(RNG always consumed)** then `if levof10()<6` swap p1<->p3.
7. Choose: first of p1..p4 for which `fieldclear(p, h, v)`; **if none is clear `dir` keeps the value
   `reversedir(mondat.dir)`** (possibly NONE).
8. Hobbins: `dir = p1` unconditionally.
9. `if mondat.dir != dir: t++`. `mondat.dir = dir`.

C. Edge stop: same four tests as Digger -> `mondat.dir = NONE`.
D. `if dir is LEFT/RIGHT`: `mopos.dir = dir; setpos` (hobbin facing).
E. Hobbin: `eatfield(mopos.x, mopos.y, mondat.dir)` (always 1 penalty, even with NONE).
F. Move: R x+=4, U y-=3, L x-=4, D y+=3 (hobbins draw the blob first); `mopos_changed=false` for NONE.
G. Hobbin: `hitemerald(...)` at the new position (destroys emeralds, result ignored).
H. `if !isalive() && changed`: revert position (monsters freeze while no digger is alive).
I. `if stime != 0 { stime--; revert position if changed }` (5 `monai` calls of standing still after spawn; note
   that steps B-G, including digging/emerald/RNG, were still executed).
J. `if ishobbin && hnt<100: hnt++`.
K. `setpos` if changed. `push=true`; `animate` (drawspr -> lists); copy lists; `incpenalty()`.
L. Monster collision (`clfirst[2] != -1`): `t++`; for each colliding monster m (do-while over the list):
   `if mondat[mon].dir == mondat[m].dir && mondat[m].stime==0 && mondat[mon].stime==0: mondat[m].dir =
   reversedir(mondat[m].dir)` (the *other* monster is turned around, even mid-cell); then `incpenalty()` once per
   m - or, in `kludge` mode (playback of recordings from version <= 19981125), only for even m, plus one more
   if `clfirst[0] != -1`.
M. Bag collision: `bagf` = any bag in `clfirst[1]` list with `bagexist`. If `bagf`:
   `t++; mongotgold=false;` horizontal dir: `push = pushbags(dir, clfirst, clcoll); t++`; otherwise
   (U/D/NONE) `if !pushudbags(...) push=false`; `if mongotgold t=0`;
   `if ishobbin && hnt>1: removebags(clfirst, clcoll)` (hobbins eat every bag/gold they touch).
N. `if isnobbin && clfirst[2]!=-1 && isalive(): hnt++` (the "crossing" counter that creates hobbins).
O. `if !push`: revert position if changed (+`setpos`), `animate` again, `incpenalty()`;
   `if isnobbin hnt++`; `if dir is UP/DOWN && isnobbin: mondat.dir = reversedir(dir)`.
P. Digger collision (`clfirst[4] != -1 && isalive()`):
   - bonus mode: for each digger sprite in the list with `digalive(dig)`: `killmon(mon)` once, `sceatm(dig)`
     for each; `soundeatm()` once if eaten.
   - otherwise: `killdigger(dig, 3, 0)` for each alive one (no-op for invincible).
Q. Recompute `h,v,xr,yr` from `mopos`.

Note the lists in L-P come from the *first* `animate` at the attempted position, even if the monster was then
reverted in O.

### 4.5 Nobbin <-> hobbin
- Nobbin -> hobbin when `hnt > 10 - levof10()` (level 1: 10 increments; level 9: 2; level 10: 1). `hnt` rises by 1
  per `monai` in which it overlaps another monster while a digger is alive (N), and by 1 per failed bag push (O).
- Hobbin -> nobbin at a cell corner when `hnt > 30 + 2*levof10()` (33..51 `monai` calls as a hobbin; ~capped at 100).
- Hobbins ignore the tunnel map (always take p1), dig (`eatfield` + blobs), eat emeralds, eat bags after their
  second move, never get the random double move, and only stop at the screen edge. A hobbin whose p1 points off
  screen (typical when fleeing in bonus mode) stays put with `dir=NONE`, paying `t++` on each re-decision.

### 4.6 Getting stuck / blocking
- Nobbin with no clear exit: reverses (rule B7).
- Vertical bag block: reverts, `hnt++`, reverses. Horizontal bag that cannot be pushed: reverts each attempt
  with `hnt++` and 2 ticks of `t`, soon becoming a hobbin that eats the bag.
- `checkmonscared(h)` (monster.c:577): a falling bag turns every slot (flag not checked) in column `h` moving UP
  to DOWN.
- Bags pushed by monsters obey the same `pushbag` rules; a monster or alive digger on the far side blocks.

### 4.7 Death
- `killmon(mon)` (monster.c:585): if `flag`: `flag=false`, `mop.kill` (erase), `if bonusmode totalmonsters++`.
  Used for: fireball (digger.c:478, +250 via `scorekill`), eaten (bonus), touching a fresh grave (no score), end of
  squash.
- Squash: `squashmonsters(bag, clfirst, clcoll)` (monster.c:595): each monster in the list with `mon.y >= bagy(bag)`
  -> `damage` (zombie), `death=1`, `bag`. Zombies keep `flag` (count as on screen, stay collidable).
- `mondie` (monster.c:514): `death==1`: `if bagy(bag)+6 > mon.y: mon.y = bagy(bag)` (**no +6, unlike Digger**);
  `animate`; `incpenalty`; `if getbagdir(bag) == -1` (stopped or gone): `dtime=1; death=4`.
  `death==4`: `if dtime!=0 dtime-- else { killmon; diggers==2 ? scorekill2() : scorekill(curplayer) }` (+250).
- `killmonsters(clfirst, clcoll)` kills every monster in the list and returns the count (counts list entries even
  if a slot's `flag` was already false).

---------------------------------------------------------------------------------------------------

## 5. Gold bags (bags.c)

### 5.1 State (bags.c:18-23)
```c
struct bag { int16_t x,y,h,v,xr,yr,
             dir,     // DIR_NONE still, DIR_LEFT/RIGHT sliding, DIR_DOWN falling
             wt,      // wobble countdown (15)
             gt,      // 0 = bag; >0 = broken, gold timer
             fallh;   // cells fallen
             bool wobbling, unfallen, exist; } bagdat[7], bagdat1[7], bagdat2[7];
int16_t pushcount, goldtime;
```
`initbags` (bags.c:31): `pushcount=0`, `goldtime = 150 - 10*levof10()` (140..50). Bags created from level chars
`'B'` scanning **x outer, y inner**, max 7 (extras silently dropped): `dir=NONE, wt=15, unfallen=true`.
Per-player saved copies (`bagdat1/2`), restored in `drawbags`, saved in `cleanupbags`.

### 5.2 `dobags` per tick (bags.c:119-160)
For bag 0..6 with `exist`:
- `gt != 0` (gold): `gt==1`: soundbreak, frame 4, penalty; `gt==3`: frame 5, penalty; `gt==5`: frame 6, penalty;
  `gt++`; `if gt==goldtime removebag` else `if v<9 && gt<goldtime-10 && (field(h,v+1)&0x2000)==0: gt=goldtime-10`
  (gold over a dug cell vanishes within 10 ticks).
- else `updatebag(bag)`.
Then sound flags (fall / wobble) from all bags.

### 5.3 `updatebag` (bags.c:164-230)
- `dir==NONE`: if `y<180 && xr==0`:
  - wobbling: `if wt==0 { dir=DOWN; soundfall; }` (falls this same tick via the tail) else
    `wt--; wbl=wt%8; if wbl even: drawgold(wblanim[wbl>>1]) (wblanim={2,0,1,0}), incpenalty, soundwobble`.
    16 ticks from wobble start to first fall step.
  - not wobbling: `if (field(h,v+1)&0xfdf)!=0xfdf && !checkdiggerunderbag(h,v+1): wobbling=true`.
  else: `wt=15; wobbling=false`.
- `dir==LEFT/RIGHT`: `if xr==0`: `if y<180 && cell below dug: dir=DOWN, wt=0, soundfall` else `baghitground`.
- `dir==DOWN`: `if yr==0 fallh++`; `if y>=180 baghitground` else `if cell below fully solid (==0xfdf) && yr==0
  baghitground`; then `checkmonscared(h)` (always).
- Tail: `if dir!=NONE: if dir!=DOWN && pushcount!=0 pushcount-- else pushbag(bag, dir)`.
  A sliding bag keeps moving 4 px/tick on its own until `xr==0`; `pushcount` (a single global shared by all bags,
  set to 1 by any horizontal push) suppresses one self-move so a bag pushed earlier in the tick does not move twice.

`checkdiggerunderbag(h,v)` (digger.c:906): true if some alive, non-invincible digger has `mdir` UP or DOWN, is in
column h and its row is `v` or `v-1`. So a bag does not start wobbling while a digger is tunnelling vertically
right under it; it starts once the digger stops or moves sideways.

### 5.4 Landing: `baghitground` (bags.c:232-254)
`if dir==DOWN && fallh>1: gt=1 (break) else fallh=0`; `dir=NONE; wt=15; wobbling=false`; `drawgold(frame 0)`,
copy lists, `incpenalty`; **every other bag overlapping it is removed** (`removebag` for the `clfirst[1]` list).
Fall distance: `fallh` is incremented at each cell boundary reached *while* falling (not at the start row), so a
one-cell drop leaves the bag intact and two or more cells break it. The frame-0 redraw replaces the broken look
until `gt==1` is processed next tick.

### 5.5 `pushbag(bag, dir)` (bags.c:256-373) - used for external pushes and self-movement
1. `gt != 0` -> `getgold(bag)`; return true.
2. Bag is falling and `dir` is L/R: redraw frame 3 in place, copy lists, penalty; kill diggers in the list with
   `diggery >= bag.y` (`killdigger(dig,1,bag)`); `squashmonsters`; **return true** (the pusher is not blocked).
3. Edge: `(x==292&&R)||(x==12&&L)||(y==180&&D)||(y==18&&U)` -> return false, nothing changes.
4. Move: R `x+=4`; L `x-=4`; D: tunnel graphics (first fall: square blob + top blob at y+21, `unfallen=false`;
   later furry blob), `eatfield(x,y,DOWN)` (1 penalty), `killemerald(h,v)`, `y+=6`.
5. D: draw frame 3 at new pos, copy lists, penalty; for digger sprites in the list with `diggery >= y`:
   `killdigger(dig,1,bag)`; `if clfirst[2]!=-1 squashmonsters(bag,...)`.
   L/R: `wt=15; wobbling=false`; draw frame 0 at new pos, copy lists, penalty; `pushcount=1`;
   `if clfirst[1]!=-1 && !pushbags(dir, clfirst, clcoll)` (recursive chain push): revert to (ox,oy), redraw,
   penalty, `push=false`; then `digf` = any *alive* digger in the list; `if digf || clfirst[2]!=-1`: revert,
   redraw, penalty, `push=false` (may happen in addition to the first revert: two redraws, two penalties).
6. `bag.dir = push ? dir : reversedir(dir)`; store x,y and recompute h,v,xr,yr.
   A failed push therefore leaves the bag "sliding backwards": if it was mid-cell it rolls back to the cell
   boundary by itself; at a boundary it stops (or drops) on its next update.

`pushbags(dir, clfirst, clcoll)` (bags.c:375): calls `pushbag` for **every** bag in the list (no short circuit);
true only if all succeeded. `pushudbags` (bags.c:387): for each bag in the list: gold -> `getgold`, else result
false (bags cannot be pushed vertically).

Pushers: Digger (3.4 step 10) and monsters (4.4 M) use identical rules; chains of bags are pushed recursively and
fail as a whole if the last one hits the edge, a monster, or an alive digger.

### 5.6 Gold: `getgold(bag)` (bags.c:446-466)
`drawgold(frame 6)`, penalty; then using the **global** lists from that draw: for each digger sprite overlapping
the gold with `digalive`: `scoregold` (+500), `soundgold`, `digresettime`; if none: `mongold()` (sets
`mongotgold`, monster gets no delay; no score). `removebag`.
So when a monster and a digger both touch gold, the digger gets it regardless of who triggered.

### 5.7 Crushing
Only from a falling bag's draw (5.5 step 2 and 5): diggers whose `y >= bag.y` (stage BAG, carried down with the
bag at `bag.y+6` until it stops), monsters whose `y >= bag.y`. +250 per monster when the bag stops; nothing for
a digger. Invincible diggers are immune.

### 5.8 Misc
- `getbagdir(bag)`: `exist ? dir : -1` (so "stopped" and "gone" are indistinguishable: both -1).
- `getnmovingbags()` (bags.c:436): bags with `exist && gt<10 && (gt!=0 || wobbling)` - used by the round-end loop.
- `cleanupbags()` (bags.c:77): at round end removes bags that are at cell (7,9), off-grid, gold, `fallh!=0` or
  wobbling; saves the rest for the player.
- `removebags(clfirst, clcoll)`: remove all bags in the list (hobbin eating).

### 5.9 Scores (scores.c:451-498)
Emerald 25; 8th consecutive emerald +250; gold 500; cherry 1000; monster killed by fireball or bag 250;
monster eaten `200 * msc` (200, 400, 800, ...). Each is one `addscore` (3 penalties).

---------------------------------------------------------------------------------------------------

## 6. Order of updates in one tick, counters and timers

Main level loop (main.c:211-236):
```
penalty = 0
newframe()                 // frame sync, key polling, input_advance_fire_state() latches fire flags
dodigger()                 // per digger: readdirect, fire, move/die, emocttime; then bonus timers
domonsters()               // spawn/bonus creation, then slots 0..5
dobags()                   // bags 0..6
if penalty > 8: incmont(penalty - 8)
checklevdone()             // levdone = (countem()==0 || monleft()==0) && isalive()
game_tick()                // dgstate.ticks++
```
Round-end loop (main.c:237-263) runs with a **different order**: `erasediggers()` (sprites disabled,
`digvisible=false`), then while `getnmovingbags()!=0 || t!=0` (t starts at 20): `penalty=0; newframe();
dobags(); dodigger(); domonsters(); if penalty<8 t=0`. In it `dodigger` still runs fire/explosions,
`readdirect`, `emocttime` and bonus timers but not movement (so **no `getdirect` calls**), and `domonsters` still
spawns, moves monsters and **consumes RNG**.

`incmont(n)` (monster.c:644): `n = min(n, 6); for m in 1..n-1: mondat[m].t++` - **slot 0 is never penalised**,
slots are hit regardless of `flag`. With penalty p: p=9 nothing, p=10 slot 1, ... p>=14 slots 1..5.

`incpenalty()` call sites (every one must be reproduced):
- drawing.c:147 `eatfield` (digger move, hobbin every `monai`, bag fall step)
- digger.c: 355 (bag-push pause), 475 (fireball flight), 568 (explosion frame), 649 (digger move), 680 (failed
  push redraw), 719 (death by bag), 750 (grave frame), 775 (death arc), 973 / 977 (emerald redraw / eaten -
  also for hobbins)
- monster.c: 395 (`monai` draw), 411/414/419 (per collided monster / kludge variants), 465 (blocked redraw),
  527 (zombie carried)
- bags.c: 129, 133, 137 (gold frames), 187 (wobble frame), 248 (landing), 276 (falling bag pushed sideways),
  317 (fall step), 336 (slide step), 343 / 357 (revert), 452 (`getgold`)
- scores.c:229-231: three per `addscore`

Timers/counters: Digger `bagtime`, `rechargetime` (`levof10*3+60`), `emocttime` (9), `ivt` (50), `deathtime`
(5 / 2 / 60), `deathani`, `dirgetime` (89), `bob.expsn`; global `bonustimeleft` (`250-20*lev`),
`startbonustimeleft` (20); monsters `t`, `hnt`, `stime` (5), `dtime` (1), `nextmontime` (10, then `mongaptime`);
bags `wt` (15), `gt` vs `goldtime`, `fallh`, `pushcount`; main `penalty`.

---------------------------------------------------------------------------------------------------

## 7. Level-dependent parameters

`levof10()` (main.c:792) = `min(level, 10)` for the current player; level itself is capped at 1000 (main.c:293).

| Parameter | Formula | L1 | L2 | L3 | L4 | L5 | L6 | L7 | L8 | L9 | L10+ |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Fire recharge (ticks after fireball gone) | `lev*3+60` | 63 | 66 | 69 | 72 | 75 | 78 | 81 | 84 | 87 | 90 |
| Bonus mode length | `250-lev*20` | 230 | 210 | 190 | 170 | 150 | 130 | 110 | 90 | 70 | 50 |
| Monster spawn gap | `45-2*lev` | 43 | 41 | 39 | 37 | 35 | 33 | 31 | 29 | 27 | 25 |
| Monsters per round | `lev+5` | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 |
| Max on screen | switch | 3 | 4 | 4 | 4 | 4 | 4 | 4 | 5 | 5 | 5 |
| Nobbin->hobbin when `hnt >` | `10-lev` | 9 | 8 | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| Hobbin->nobbin when `hnt >` | `30+2*lev` | 32 | 34 | 36 | 38 | 40 | 42 | 44 | 46 | 48 | 50 |
| Double-move RNG modulus | `15-lev` | 14 | 13 | 12 | 11 | 10 | 9 | 8 | 7 | 6 | 5 |
| Direction-noise RNG modulus | `lev+5` | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 |
| Direction noise applied | `lev<6` | yes | yes | yes | yes | yes | no | no | no | no | no |
| Gold lifetime (`goldtime`) | `150-10*lev` | 140 | 130 | 120 | 110 | 100 | 90 | 80 | 70 | 60 | 50 |

Fixed: first spawn delay 10; spawn standstill 5; wobble 15; invincibility 50; bonus start flash 20; extra life
every 20000; max 7 bags, 6 monster slots.
`maxmononscr` has no `default:` - a level of 0 would leave the previous value.

---------------------------------------------------------------------------------------------------

## 8. RNG use (determinism)

`randno(n)` (main.c:1226): `randv = randv*0x15a4e35 + 1` (int32, wrapping); returns
`(int16)((randv & 0x7fffffff) % n)`. `randv` is reseeded at the start of every round (life) from the recording
or `roundseed` (main.c:163-175). The sound code has its own separate generator (`randnos`).

The **only** callers in the whole game are in monster.c. Per tick, in `domonsters`, for slots i = 0..5 in
ascending order, for each slot with `flag && mop.alive && t==0`:
1. inside the first `monai(i)`: `randno(levof10()+5)` - only if the monster is on a cell corner
   (`xr==0 && yr==0` as stored), consumed regardless of level (the `<6` test comes after);
2. `randno(15-levof10())` - always;
3. if that returned 0 and the monster is a nobbin and still alive: second `monai(i)`, which again consumes
   `randno(levof10()+5)` if (still/now) on a cell corner.
Nothing else consumes RNG: not Digger, bags, fireballs, spawning, or bonus. Slots that are waiting (`t>0`),
zombies, and empty slots consume nothing. The same applies in the round-end loop.

Because RNG consumption depends on `t`, which depends on `penalty`, every `incpenalty` site and every
collision-list detail feeds back into the random stream: any divergence anywhere desynchronises playback.

---------------------------------------------------------------------------------------------------

## 9. Integer widths, signedness, quirks to preserve

Widths:
- Nearly everything is `int16_t`; arithmetic happens in C `int` (32-bit) and is truncated on store. No value in
  this area overflows 16 bits in practice. Use `i16`/`i32` freely but keep truncating `/` and `%` (Rust's are the
  same for these non-negative operands).
- `field[]` is `int16_t` initialised to -1; all uses are bit tests. Port as `u16` (0xffff). `emfield` is `int8_t`
  bitmask (bits 0,1).
- `randv` is `int32_t`: use `i32::wrapping_mul/wrapping_add`, mask, `%` by a positive `i16`.
- `addscore` takes `int16_t`: `msc*200` would wrap at msc=256 (unreachable: at most 6 monsters per bonus mode).
  Use wrapping arithmetic rather than panicking. `msc <<= 1` likewise.
- `startbonustimeleft` legitimately goes negative. `dgstate.cgtime` (uint32) wraps on the timeout tick.
- `first[]/coll[]` are `int` with -1 sentinel.
- Rust debug builds panic on overflow/out-of-range indexing: note `mondat[m].h` etc. are used for slots with
  `flag==false`, `bagy(deathbag)` for removed bags (data still valid), and `digdat[0..diggers-1]` absolute indexing.

Quirks deliberately (or accidentally) part of behaviour:
1. `incmont` never delays monster slot 0 and ignores `flag` (monster.c:649).
2. Penalty system as a whole (emulates original CPU-load slowdown), including 3 penalties per score event.
3. `kludge` mode for recordings dated <= 19981125 (monster.c:408-419; record.c:131).
4. RNG calls split from their conditions ("Need to split for determinism", monster.c:113, 273).
5. `scdat[n].nextbs + n` extra-life off-by-n (scores.c:219, "original bug").
6. Zombie monster snapped to `bagy` without +6 (monster.c:523) vs Digger `bagy+6`.
7. Dead left-facing hobbin sprite is 14 px tall (monster_obj.c:133).
8. Monster direction "none clear" falls back to reverse; hobbins stuck at edges; mid-cell reversal of the *other*
   monster on collision.
9. Global `pushcount` shared across bags; failed push sets bag `dir` to the reverse; double revert in `pushbag`.
10. Pushing a falling bag sideways returns success (bags.c:285).
11. Landing bag deletes any overlapping bag; gold over a dug cell expires early.
12. `getgold` reads the global collision lists; digger wins ties with monsters.
13. Outside bonus mode only the monster's move kills Digger; in bonus mode both sides' moves can eat.
14. Monsters touching a fresh grave (first ANIM frame) are removed without score.
15. `killdigger` restart rule (`stage<2 || stage>4`).
16. Absolute-index loops: `alldead` test (digger.c:805), `initbonusmode` msc reset (digger.c:874), `clearfire(n)`
    (digger.c:842), facing for `dig==0` (digger.c:106, 822).
17. Respawn does not reset `dob.can_fire` nor remove an in-flight fireball sprite (cannot be in flight in
    practice: death takes > 70 ticks).
18. Stationary-digger/bag overlap sets facing to -1; firing then `abort()`s (3.4).
19. Hobbin `eatfield` with `DIR_NONE` still costs a penalty; spawn-standstill hobbin logic still digs.
20. Emerald eaten thresholds per direction (`embox`), emeralds removed under falling bags and by hobbins without
    score.
21. Fireball/earth detection through framebuffer pixels with the odd `0xee`, `0xc0`, `0x3` masks (3.6).
22. `getdirect` (recording stream) is called only from `updatedigger`; fire is latched one frame late.
23. One-digger death waits a tick-counted 89 ticks for the dirge (`DIRGE_TICKS`) - a Remastered-era change, part of
    current recordings' timing; real-time `dirgehold`/`deathffwd` do not affect tick counts.
24. `mutate` check runs for zombie monsters too (could flip a dead monster's sprite type).
25. Animation counters advance per draw call (double draws on revert), purely visual except quirk 7.

---------------------------------------------------------------------------------------------------

## 10. Suggested Rust design

Principle: reproduce the procedural flow and data layout; do not "clean up" semantics. One owning `Game` struct
replaces the C globals; no trait objects are needed (each "object" has exactly one implementation).

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(i16)]
pub enum Dir { None = -1, Right = 0, Up = 2, Left = 4, Down = 6 }   // keep numeric values (embox, recordings)

pub struct SpriteSys {            // port of sprite.c state: needed for logic
    en: [bool; 18], x: [i16; 19], y: [i16; 19], wid: [i16; 19], hei: [i16; 19],
    nwid: [i16; 18], nhei: [i16; 18], ch/nch ...,
    pub first: [i32; 5], pub coll: [i32; 18],
}
pub struct Coll { first: [i32; 5], next: [i32; 18] }                // the copied clfirst/clcoll, Copy
impl Coll { fn iter(&self, ty: usize) -> impl Iterator<Item = usize> }  // lowest index first

pub struct Digger { h,v,rx,ry: i16, mdir: Dir, bagtime, rechargetime: i16, death: DeathStage, deathbag,
                    deathani, deathtime, emocttime, emn, msc, lives, ivt: i16, notfiring, dead, invin: bool,
                    dirgetime: i16, x, y: i16, dir: Dir /* may be None: quirk 18 */, alive, can_fire: bool,
                    fire: Fireball }
pub struct Fireball { expsn: i16, dir: Dir, x: i16, y: i16 }
pub struct Monster { h,v,xr,yr: i16, dir: Dir, t,hnt,death,bag,dtime,stime: i16, chase: usize, flag: bool,
                     x,y: i16, hdir: Dir, alive, zombie, nobbin: bool, spr, spd: i16 }
pub struct Bag { x,y,h,v,xr,yr: i16, dir: Dir, wt,gt,fallh: i16, wobbling, unfallen, exist: bool }

pub struct Entities {             // or fields of Game
    diggers: [Digger; 2], monsters: [Monster; 6], bags: [Bag; 7], saved_bags: [[Bag; 7]; 2],
    field: [u16; 150], saved_field: [[u16; 150]; 2], emfield: [u8; 150],
    bonus_visible, bonus_mode, dig_visible: bool, bonus_time_left, start_bonus_time_left: i16,
    next_monster, total_monsters, max_mon_on_scr, next_mon_time, mon_gap_time: i16, chase: usize,
    unbonus_flag, mon_got_gold: bool, push_count, gold_time: i16,
}
```
- Functions as `impl Game` methods taking `&mut self` (C functions freely cross-call between digger/monster/bag
  modules, e.g. `monai -> pushbags -> pushbag -> killdigger/squashmonsters -> ...`); splitting into separately
  borrowed sub-structs will fight the borrow checker. Index-based access (`self.bags[i]`) with short borrows works;
  recursion in `pushbag` is naturally `&mut self`.
- Keep `monster_obj`'s fields inline in `Monster` (no heap object). Keep "dead but object retained" semantics by
  never resetting x/y on kill.
- Side-effect interfaces as traits or plain structs injected into `Game`: `Gfx` (sprite images, blobs, `getpix`,
  intensity), `Sound` (sound*/music calls), `Scores` (`addscore` - which must call back `incpenalty` 3x), `Input`
  (`readdirect/getdirect/fire`). A headless implementation of `Gfx` must still maintain the framebuffer for
  `getpix` unless playback of fireball shots is not required.
- `penalty` lives in `Game`; `incpenalty` is a method.
- State hashing: the C `*_debug_hash_append` functions (digger.c:168, monster.c:124, bags.c:95) define exactly
  which fields constitute state - mirror them to enable cross-checking C vs Rust per tick (very valuable test
  oracle: run the C binary and the port on the same DRF and compare hashes each tick).
- `DeathStage` enum with values 1..6 (`Bag, Anim, Monster, Tombstone, Arc, Done`); `killdigger` compares
  numerically.

### Trickiest parts to port
1. **Collision-list timing**: lists reflect sprite state at the exact draw; several code paths use stale lists
   after reverting positions, and `getgold` reads the global lists. Port `sprite.c` state literally and copy lists
   where C copies them.
2. **Penalty accounting** and `incmont`: any missed/extra `incpenalty` changes monster timing and then RNG.
3. **RNG call pattern** in `domonsters`/`monai` (unconditional consumption, double move).
4. **Fireball vs earth through `ggetpix`**: requires a pixel-accurate framebuffer including sprite draw order
   and the hi-res bit masks; cross-team dependency with graphics.
5. **`pushbag` recursion** with shared `pushcount`, revert logic and `dir=reversedir` on failure; mutually
   recursive with `pushbags`, calling into digger/monster kill paths mid-update.
6. **`monai` ordering**: decisions use `xr/yr` stored from the previous call; many "revert but keep side effects"
   steps (stime, !isalive, !push).
7. **Digger death state machine** and two-digger respawn, incl. tick-counted dirge wait and absolute-index quirks.
8. **Input coupling**: `getdirect` consumed only in `updatedigger` (skipped during bagtime pauses, death, and the
   round-end loop); fire latched per frame.
9. **Direction -1 as a legal stored value** (`mdir`, monster `dir`, even digger facing): a Rust enum must
   include `None` everywhere and `reversedir(None)=None`.
10. **Round-end loop** reorders subsystems and keeps monsters/RNG running with diggers hidden.
