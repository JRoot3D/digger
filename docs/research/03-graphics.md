# 03 — Graphics and video (porting reference)

Source tree: `https://github.com/sobomax/digger` (HEAD 638125e).
All `file:line` references are relative to that directory. Facts marked **[verified]** were checked by
parsing the C tables with a script (sizes, value sets, zlib inflate), not just by reading.

Contents
1. Layering and the video function table
2. Coordinate systems
3. Palettes
4. SDL backend primitives — exact contracts
5. Sprite data (VGA and CGA) and the sprite index table
6. Font (alpha.c) and text/number drawing
7. Sprite engine (sprite.c) — algorithmic detail, collisions
8. drawing.c — field, tunnels, emeralds, lives, status line
9. Title screen (title_gz.c, title_anim.c)
10. SDL backend plumbing (window, texture, threads, fullscreen, icon)
11. Emscripten and FreeBSD backends — what the portable contract is
12. Suggested Rust design
13. Trickiest parts / gotchas checklist

---

## 1. Layering and the video function table

```
game logic (digger.c, monster*.c, bags.c, scores.c, main.c, title_anim.c)
   |  drawing.c  (drawdigger/drawgold/drawemerald/blobs/drawlives/outtext, field bitmap)
   |  sprite.c   (18 sprites + 1 "misc" pseudo sprite, bg save/restore, collision lists)
   v
struct digger_draw_api *ddap      (draw_api.h:6-20)
   v
vga* functions in sdl_vid.c (or fbsd_vid.c)  -> 640x400 8-bit indexed surface -> present
```

`struct digger_draw_api` (draw_api.h:6-20) and the constants `MAX_W 320`, `MAX_H 200`, `CHR_W 12`, `CHR_H 12` (draw_api.h:1-4):

| slot | signature | default binding (sprite.c:44-61) |
|---|---|---|
| `ginit` | `void(void)` | `vgainit` |
| `gclear` | `void(void)` | `vgaclear` |
| `gpal` | `void(int16_t pal)` | `vgapal` |
| `ginten` | `void(int16_t inten)` | `vgainten` |
| `gputi` | `void(x,y,uint8_t *p,w,h)` | `vgaputi` |
| `ggeti` | `void(x,y,uint8_t *p,w,h)` | `vgageti` |
| `gputim` | `void(x,y,int16_t ch,w,h)` | `vgaputim` |
| `ggetpix` | `int16_t(x,y)` | `vgagetpix` |
| `gtitle` | `void(void)` | `vgatitle` |
| `gwrite` | `void(x,y,int16_t ch,int16_t c)` | `vgawrite` (`gwrite_debug` with bounds asserts under `DIGGER_DEBUG`, sprite.c:301-309) |
| `gflush` | `void(void)` | `gflush_async` -> `doscreenupdate(false)` (sprite.c:65-69) |
| `gflush_sync` | `void(void)` | `gflush_sync` -> `doscreenupdate(true)` (sprite.c:71-75) |

The single global instance is `ddap` (sprite.c:63; declared `const` there but `extern struct digger_draw_api *ddap` and
mutated in main.c:89, 1147-1158, 1296-1307). Not in the table but part of the backend surface (hardware.h:20-21,
sdl_vid.h:16-18): `graphicsoff()`, `gretrace()` (no-op), `doscreenupdate(bool wait)`, `switchmode()` (fullscreen toggle),
`sdl_enable_fullscreen()` (set flag before init, `/F` option main.c:1020).

### CGA vs VGA "modes"
- `/C` or `/B` on the command line (main.c:1146-1159) or `CGA=true` in the INI `[Graphics]` section (main.c:1294-1308)
  rebinds every slot except `gflush*` to the `cga*` functions, then calls `ginit(); gpal(0)` again.
- **Surprise:** in every backend still in the tree (SDL sdl_vid.c:795-808, FreeBSD fbsd_vid.c:457-497) the `cga*`
  functions are empty stubs (`cgagetpix` returns 0). So "CGA mode" in the SDL build is a black, non-functional screen in
  which fireballs never hit walls (`ggetpix` always 0). `cgagrafx.c` (compiled, CMakeLists.txt:214) and the `ascii2cga`
  table are dead data: `cgatable` is referenced by nothing.
- Recommendation: the port only needs the VGA path. CGA data can be dropped or kept as an optional asset set (format in §5.3).

### Who calls what (for the contract)
- `ginit`+`gpal(0)`: once at startup main.c:358-359.
- `gclear`: new game main.c:149, title loop main.c:431, drawscreen main.c:749, key redefinition keyboard.c:47, scores.c:270, 392.
- `gtitle`: main.c:432 only.
- `gpal(0); ginten(0)`: start of every level draw, drawing.c:92-93; scores.c:393-394.
- `gpal(1-p)` toggling: `flashywait` scores.c:398-410 (high-score entry flash), each followed by `gflush_sync`.
- `ginten(1/0)`: bonus mode only, digger.c:372-382 (blink at start/end), 871 (on), 882/892 (off).
- `ggetpix`: fireball flight only, digger.c:447-466 (see §4.6). **Gameplay depends on framebuffer contents.**
- `gflush`: main.c:272, 533, 839; `gflush_sync`: main.c:453, scores.c:407. The regular per-frame present is not through
  `ddap` at all: `gethrt()` calls `doscreenupdate(false)` directly (sdl_timer.c:61, 86).

---

## 2. Coordinate systems

- Game logic and all `ddap` calls use the **logical 320x200** CGA-era coordinate system. `x`,`y` are logical pixels.
- Widths `w` passed to `gputi/ggeti/gputim` and stored in sprites are in **units of 4 logical pixels** ("CGA bytes");
  heights `h` are in logical pixels. So `w=4,h=15` is a 16x15 logical sprite.
- The real framebuffer is **640x400, 8 bits per pixel, palette-indexed, 16 colours used** (sdl_vid.c:42-43, 543).
  Mapping (sdl_vid.c:37-48): `realx = 2*x`, `realy = 2*y + 0`, `realw = 8*w`, `realh = 2*h`.
- VGA sprite/glyph assets are authored at the real resolution (a 4x15 sprite is 32x30 bytes). They are genuine hi-res
  art, not pixel-doubled CGA: e.g. 130 of 240 2x2 blocks in `vgardigger1` are non-uniform **[verified]**. The tunnel
  blob masks, by contrast, are exactly 2x2-doubled **[verified]**.
- Playfield grid: 15x10 cells (`MWIDTH`,`MHEIGHT`, def.h:51-52). Cell (cx,cy) has its sprite origin at
  `x = cx*20+12`, `y = cy*18+18` (drawing.c:115-116, bags.c:49-50). Emeralds are drawn at `(cx*20+12, cy*18+21)`
  (digger.c:949). Status line occupies y 0..11; background tiles start at y=14 (rows 12,13 stay black).
- `x` is forced to a multiple of 4 by the sprite engine (`x & -4`, sprite.c:91, 121, 184) but **not** by `gwrite`,
  `ggetpix`, or `initmiscspr`.

---

## 3. Palettes (SDL backend, sdl_vid.c:50-72)

Four 16-entry tables, RGB 0..255 (the 4th `SDL_Color` field, alpha, is 0 in all entries):

| idx | pal1 normal | pal1 intense | pal2 normal | pal2 intense |
|---|---|---|---|---|
| 0 | 0,0,0 | 0,0,0 | 0,0,0 | 0,0,0 |
| 1 | 0,0,128 | 0,0,255 | 0,128,0 | 0,255,0 |
| 2 | 0,128,0 | 0,255,0 | 128,0,0 | 255,0,0 |
| 3 | 0,128,128 | 0,255,255 | 128,64,0 | 255,128,0 |
| 4 | 128,0,0 | 255,0,0 | 0,0,128 | 0,0,255 |
| 5 | 128,0,128 | 255,0,255 | 0,128,128 | 0,255,255 |
| 6 | 128,64,0 | 255,128,0 | 128,0,128 | 255,0,255 |
| 7 | 128,128,128 | 196,196,196 | 128,128,128 | 196,196,196 |
| 8 | 64,64,64 | 128,128,128 | 64,64,64 | 128,128,128 |
| 9 | 0,0,255 | 128,128,255 | 0,255,0 | 128,255,128 |
| 10 | 0,255,0 | 128,255,128 | 255,0,0 | 255,128,128 |
| 11 | 0,255,255 | 128,255,255 | 255,255,0 | 255,255,128 |
| 12 | 255,0,0 | 255,128,128 | 0,0,255 | 128,128,255 |
| 13 | 255,0,255 | 255,128,255 | 0,255,255 | 128,255,255 |
| 14 | 255,255,0 | 255,255,128 | 255,0,255 | 255,128,255 |
| 15 | 255,255,255 | 255,255,255 | 255,255,255 | 255,255,255 |

(pal2 is pal1 with the index bits rotated 1->2->4->1: pal2[1]=pal1[2], pal2[2]=pal1[4], pal2[4]=pal1[1], pal2[3]=pal1[6], ...)

State and semantics:
- `currpal` (sdl_vid.c:73), initially 0.
- `vgapal(pal)` (sdl_vid.c:590-594): select **normal** table `npalettes[pal]`, set `currpal=pal`. I.e. `gpal` also resets intensity.
- `vgainten(inten)` (sdl_vid.c:582-588): `inten==1` -> `ipalettes[currpal]`, anything else -> `npalettes[currpal]`.
- Palette changes only touch the palette attached to the 8-bit framebuffer (`setpal`, sdl_vid.c:575-580); pixels are
  untouched. The change becomes visible at the next present, where the whole frame is converted through the current palette.
- Colours actually used by assets **[verified]**: sprites {0,2,3,4,6,7,8,10,12,14,15}, font {0,10,12,15} (+ recolour
  targets 2,4,6,14), title {0,4,7,8,15}. Indices 1,5,9,11,13 never appear.

Usage: game always runs in pal 0. Intense palette = bonus mode (digger.c:871), blinking in the first/last 20 bonus
ticks (digger.c:367-384). Pal 1 only appears in `flashywait` (scores.c:398-410) alternating pal 0/1 every sync flush.

The FreeBSD table (fbsd_vid.c:45-64) has different numbers (225/210/240 instead of 128/64/196, stored BGR and fed to
`VGLSetPaletteIndex(i, pal[2], pal[1], pal[0])`, fbsd_vid.c:169-177), which shows exact RGB values are a backend
detail; the contract is "2 palettes x 2 intensities, 16 entries". Use the SDL values.

---

## 4. SDL backend primitives — exact contracts

Framebuffer: `screen16`, 640x400 8-bit (sdl_vid.c:543). Scratch conversion target: `screen`, 640x400 32-bit
ARGB masks (sdl_vid.c:536).

An "image buffer" `p` in `gputi/ggeti` is nominally a byte buffer (`monbufs[..][480]` etc., drawing.c:24-25) but the
SDL and VGL backends store **a pointer to a heap surface in the first `sizeof(void*)` bytes** (sdl_vid.c:621, 633, 646).
Buffers start zeroed (static storage) = NULL = "nothing saved yet".

### 4.1 `vgainit` (sdl_vid.c:498-563)
SDL_Init(VIDEO), create window, icon, the two surfaces, then either start the async display thread or call `setmode()`.
Does not clear or set a palette (main.c:359 calls `gpal(0)` right after). See §10.

### 4.2 `vgaclear` (sdl_vid.c:565-573)
Fills the whole 640x400 framebuffer with index 0 (implemented as get full screen, memset, put). Does not touch
sprite state or palette.

### 4.3 `vgageti(x,y,p,w,h)` (sdl_vid.c:628-647)
Free the surface previously stored in `p` (if non-NULL); allocate a new zero-filled 8-bit surface of `8w x 2h`; copy
the framebuffer rectangle at `(2x,2y)` into it; store its pointer into `p`. `SDL_BlitSurface` clips: any part of the
rectangle outside 640x400 is left as 0 in the new surface and the in-bounds part lands at the correct offset.

### 4.4 `vgaputi(x,y,p,w,h)` (sdl_vid.c:610-626)
Blit the surface stored in `p` to the framebuffer at `(2x,2y)`, opaque (every byte, including 0, overwrites), clipped
to the framebuffer. **`w`/`h` are ignored by SDL (`SDL_BlitSurface` ignores dst w/h); the stored surface's own size is
used.** (FreeBSD uses the passed w/h, fbsd_vid.c:264 — they agree in all normal call patterns.) The palette pointer swap
(sdl_vid.c:622-625) exists to force a raw index copy with no palette remapping; port as plain byte copy. A NULL `p`
would crash in C; it never happens in practice (see §7.6).

### 4.5 `vgaputim(x,y,ch,w,h)` — masked sprite draw (sdl_vid.c:675-698)
`sprite = vgatable[2*ch]`, `mask = vgatable[2*ch+1]`, both read as `8w x 2h` row-major byte arrays. Implemented as
`geti` of the destination rect into a temp, then for every byte `i`:

```
if sprite[i] != 0xff { scr[i] = (scr[i] & mask[i]) | sprite[i]; }
```

then `puti` back. Net effect: a clipped masked blit; out-of-screen parts are discarded.
**[verified]** over the whole table: masks contain only 0x00/0xff; a non-0xff, non-zero sprite byte always has mask 0;
sprite 0 with mask 0xff (1656 bytes) is a no-op. So the rule reduces exactly to
**"if sprite[i] != 0xff && mask[i] == 0 then scr[i] = sprite[i]"**. Keep the original formula anyway; it is as cheap.
- 0xff in sprite data = transparent. 0x00 in sprite + 0x00 in mask = opaque black (used for outlines and for the
  "eraser" pseudo-sprites whose image is `vgazero480`).
- Sprite surfaces are cached per `ch*2` index in `ch2bmap` (sdl_vid.c:130-147) with the w/h of the first call; purely an
  optimisation (every table entry is always used with one size).

### 4.6 `vgagetpix(x,y)` (sdl_vid.c:649-673)
```
if x > 319 || y > 199 return 0xff
rval = 0
for yi in 0..2, xi in 0..8:  if fb[(2y+yi)*640 + 2x + xi] != 0  rval |= 0x80 >> xi
return rval & 0xee
```
Emulates reading one CGA video byte (4 logical pixels starting **at x, not at x&~3**): bit (7-xi) is set when real column
`xi` of the 8x2 block is non-black in either row; columns 3 and 7 are then masked off (0xee). Callers (digger.c:447-466):
- RIGHT/LEFT: `pix = getpix(x,y+4) | getpix(x+4,y+4)` (any of bits 0xee);
- UP: OR over `getpix(x+4, y+i)`, i=0..6, then `& 0xc0` (real columns 0,1 = logical pixel x+4);
- DOWN: OR over `getpix(x, y+i)`, then `& 0x03` (after the 0xee mask only bit 1 = real column 6 = left half of logical pixel x+3).

Hazards: no lower-bound check (negative x or y reads before the buffer: UB in C); for x in 317..319 the 8-wide read
runs past the row end into the next row (and 6 bytes past the buffer on the last row). The port must pick a defined
behaviour: recommended = treat the framebuffer as a flat `[u8; 256000]`, index linearly (reproduces row wrap), and
return 0 for out-of-range indices. Ask the game-logic agent whether fireballs can reach x>=317 or y<0 (digger at x=292
firing right: fireball x=300.., `+4` -> at most ~316, so probably never).

The result depends on **all** pixels on screen at that moment, including other sprites, text and emeralds. Therefore
every backend, including the headless one, must maintain a bit-exact 640x400 index buffer.

### 4.7 `vgawrite(x,y,ch,c)` — one glyph (sdl_vid.c:700-744)
If `!isvalchar(ch)` return (alpha.h:6). Glyph = `ascii2vga[ch-32]`, 24x24 bytes. Copy, recolour per byte, then opaque
`puti` at `(2x,2y)` (so a glyph always overwrites a full 12x12 logical cell, background included):

| glyph byte | c=0 | c=1 | c=2 | c=3 |
|---|---|---|---|---|
| 10 (body) | 10 | 10 | 12 | 14 |
| 12 (shadow) | 12 | 2 | 4 | 6 |
| other (0, 15) | unchanged | | | |

With pal 0: c=0 green/red-shadow, c=1 green, c=2 red, c=3 yellow. Any other `c` behaves like 0.

### 4.8 `vgatitle` (sdl_vid.c:746-754)
Inflate the title picture straight into a full-screen temp and put it at (0,0): replaces the entire framebuffer.

### 4.9 `doscreenupdate(wait)` (sdl_vid.c:596-608) and `graphicsoff` (sdl_vid.c:149-173)
Present the current framebuffer through the current palette (§10). `graphicsoff` stops the thread and destroys everything.
`gretrace` is a no-op (sdl_vid.c:756-758).

---

## 5. Sprite data

### 5.1 VGA format (vgagrafx.c) — the one that matters
- One `static const uint8_t vgaNAME[]` per image and one `vgaNAMEmask[]` per mask. **Uncompressed, one byte per real
  pixel, row-major, value = palette index 0..15, 0xff = transparent** (image) / 0x00 = replace, 0xff = keep (mask).
  No bit planes, no packing, no RLE.
- Size = `8w * 2h` bytes. **[verified]** sizes:

| bytes | real px | logical (w,h) | arrays |
|---|---|---|---|
| 960 | 32x30 | 4,15 | all digger/bigger frames, graves, bags, gold, nobbin/hobbin, bonus, `vgaediggermask`, `vgazero480` |
| 896 | 32x28 | 4,14 | `vgalhobbind` + mask (drawn with h=14, monster_obj.c:133) |
| 768 | 32x24 | 4,12 | `vgaliferight`, `vgalifeleft`, `vgaleftlife` (+masks), `vgaelifemask` |
| 768 | 48x16 | 6,8 | `vgafurryblobmask` |
| 640 | 32x20 | 4,10 | `vgaemerald` + mask |
| 576 | 16x36 | 2,18 | `vgarightblobmask`, `vgaleftblobmask` |
| 576 | 48x12 | 6,6 | `vgatopblobmask`, `vgabottomblobmask`, `vgasquareblobmask` |
| 320 | 40x8 | 5,4 | `vgaback1..8` |
| 256 | 16x16 | 2,8 | `vgafire1..3`, `vgaexp1..3`, `vgafire1b..3b`, `vgaexp1b..3b` (+masks) |

- `vgazero480` (vgagrafx.c:6) is 960 zero bytes, used as the image of mask-only "eraser" entries and as the (all-zero =
  fully opaque) mask of background tiles. `vgaediggermask` (vgagrafx.c:5374) is 960 zero bytes too (opaque 16x15 block);
  it is the mask of the bonus sprite (opaque rectangle).
- 190 data arrays in total; definitions at vgagrafx.c:6-5917; index table `vgatable[240]` at vgagrafx.c:5918-6049.

Naming scheme: `vga` + direction (`r`,`l`,`u`,`d`) + optional `x` ("eXhausted": digger that cannot fire / no eye) +
`digger` (player 1) or `bigger` (player 2 / second digger) + frame 1..3; `...d` suffix = dead/dying frame;
`s/r/l/f bag` = still/right-wobble/left-wobble/falling; `gold1..3` = bag breaking; `nobbin1..3`, `rhobbin1..3`,
`lhobbin1..3`; `grave1..5`; `fire/exp` + optional `b` (second digger's fireball, green instead of red).

### 5.2 Sprite index table (`ch` numbers; `vgatable[2ch]` image, `[2ch+1]` mask) **[verified]**

| ch | image / mask | w,h | meaning |
|---|---|---|---|
| 0 | zero480 / ediggermask | 4,15 | blank digger (black block); initial digger char (drawing.c:242) |
| 1-3 | rdigger1-3 | 4,15 | digger right, frames |
| 4-6 | rxdigger1-3 | 4,15 | right, cannot fire |
| 7-9 | udigger1-3 | | up |
| 10-12 | uxdigger1-3 | | up, cannot fire |
| 13-15 | ldigger1-3 | | left |
| 16-18 | lxdigger1-3 | | left, cannot fire |
| 19-21 | ddigger1-3 | | down |
| 22-24 | dxdigger1-3 | | down, cannot fire |
| 25 | diggerd | | dead digger (upside down) |
| 26-30 | grave1-5 | | gravestone rising |
| 31 | zero480 / ediggermask | | blank (digger 2 base) |
| 32-55 | r,rx,u,ux,l,lx,d,dx `bigger`1-3 | | second digger, same layout as 1-24 |
| 56 | biggerd | | dead second digger |
| 57-61 | grave1-5 (same arrays as 26-30) | | |
| 62 | sbag | 4,15 | bag still |
| 63 | rbag | | bag wobble right |
| 64 | lbag | | bag wobble left |
| 65 | fbag | | bag falling |
| 66-68 | gold1-3 | | bag breaking -> gold pile |
| 69-71 | nobbin1-3 | 4,15 | |
| 72 | nobbind | | dead nobbin |
| 73-75 | rhobbin1-3 | | |
| 76 | rhobbind | | dead hobbin facing right |
| 77-79 | lhobbin1-3 | | |
| 80 | lhobbind | **4,14** | dead hobbin facing left (data is only 28 rows) |
| 81 | bonus / ediggermask | 4,15 | cherry, opaque block |
| 82-84 | fire1-3 | 2,8 | fireball digger 0 |
| 85-87 | exp1-3 | 2,8 | explosion digger 0 |
| 88-93 | fire1-3, exp1-3 again | | duplicates (unused padding so that 82+32=114) |
| 94-101 | back1-8 / zero480 | 5,4 | background tile for level plan 1..8 (`93+l`, drawing.c:313), opaque |
| 102 | zero480 / rightblobmask | 2,18 | tunnel carve, moving right |
| 103 | zero480 / topblobmask | 6,6 | moving up |
| 104 | zero480 / leftblobmask | 2,18 | moving left |
| 105 | zero480 / bottomblobmask | 6,6 | moving down |
| 106 | zero480 / squareblobmask | 6,6 | bag-fall "square" |
| 107 | zero480 / furryblobmask | 6,8 | bag-fall "furry" |
| 108 | emerald / emeraldmask | 4,10 | emerald |
| 109 | zero480 / emeraldmask | 4,10 | erase emerald (paint its silhouette black) |
| 110 | liferight | 4,12 | life icon player 1 |
| 111 | lifeleft | 4,12 | life icon player 2 (alternating 2-player) |
| 112 | zero480 / elifemask | 4,12 | blank life slot (black block) |
| 113 | leftlife | 4,12 | life icon of second digger (simultaneous mode) |
| 114-116 | fire1b-3b | 2,8 | fireball digger 1 |
| 117-119 | exp1b-3b | 2,8 | explosion digger 1 |

Offsets used by callers: digger 2 = digger 1 + 31 (drawing.c:340); fireball 2 = +32 (drawing.c:319).
Colours per family **[verified]**: diggers/monsters/lives {0,2,4,6,10,12,14,15}; graves {0,3,7,8}; bags {0,6};
gold {0,6,14,15}; bonus {0,2,4,10,12,14}; fire {4,6,14,15}, fire-b {2,6,14,15}; emerald {0,2,10,14,15};
backgrounds: back1 {4,6}, back2 {2,4}, back3 {4,6}, back4 {2,6}, back5 {2,4,6}, back6 {2,4}, back7 {2,4,6}, back8 {4,6}.

### 5.3 CGA format (cgagrafx.c) — dead in this codebase, for completeness
- Same symbol set with prefix `cga`, same table layout (`cgatable[240]`, cgagrafx.c:3055-3186).
- **2 bits per pixel packed, 4 pixels per byte, MSB = leftmost pixel**, `w` bytes per row, `h` rows; size `w*h`
  (4,15 -> 60 bytes; emerald 40; life 48; fire 16; back 20; blobs 36/48; `cgalhobbind` 56) **[verified]**.
  E.g. `cgaback1` = `0xaf,0xff,0xaa,0xff,0xfa, ...` (cgagrafx.c:2767-2771).
- Masks are AND masks (bit pair 11 = keep, 00 = clear), image is ORed: `scr = (scr & mask) | img` on video bytes — the
  origin of the VGA formula and of `x & -4` alignment and `w` in bytes. No 0xff transparency convention here.
- CGA font: `cgalet*`, 3 bytes x 12 rows = 36 bytes each (alpha.c:7-566), index table `ascii2cga` (alpha.c:1607-1618).

---

## 6. Font (alpha.c)

- 40 VGA glyphs `vgaletA..Z`, `vganum0..9`, `vgasymdot`, `vgasymcolon`, `vgasymline`, `vgasymspace`
  (alpha.c:567-1606), each **576 bytes = 24x24 real pixels (12x12 logical), one palette index per byte**, values
  {0,10,12,15} **[verified]**: 10 = body, 12 = right/bottom shadow, 15 = top/left highlight, 0 = background (drawn
  opaque). Ink occupies at most real columns 0..19, so there is built-in spacing.
- `ascii2vga[0x5f]` (alpha.c:1619-1630) maps `ch-32` -> glyph or NULL. Valid characters **[verified]**:
  space, `.`, `0`-`9`, `:`, `A`-`Z`, `_` (a low underline, `vgasymline`), and `a`-`z` mapped to the **same glyphs as
  upper case**. Everything else (including `-`, `!`, `/`) is NULL.
- `isvalchar(ch)` (alpha.h:6): `32 <= ch < 32+0x5f && ascii2vga[ch-32] != NULL`.

Text routines (drawing.c):
- `outtextl(ddap,p,x,y,c,l)` (drawing.c:40-51): for each of `l` chars, `gwrite(x,y, isvalchar(ch)?ch:' ', c)`, `x += 12`.
  Invalid characters are rendered as a space (so they erase the cell).
- `outtext(ddap,p,x,y,c)` (drawing.c:53-57): `l = strlen(p)`.
- `erasetext(ddap,n,x,y,c)` (drawing.c:59-63): writes `n` spaces (`n <= MAX_TEXT_LEN = 26`, drawing.h:4).
- `cleartopline()` (main.c:778-782): `erasetext(26, 0,0)` then `erasetext(1, 308,0)` to cover x 0..319.
- Debug builds assert `x+12 <= 320 && y+12 <= 200` per glyph (sprite.c:301-309).

Number drawing, `writenum(n,x,y,w,c)` (scores.c:499-511): right-aligned in `w` cells, writes digits from the right at
`xp = x+(w-1)*12` stepping -12; a digit is drawn if `w>1 || d>0`, i.e. **the lower w-1 positions are always drawn
(zero-padded) and the leftmost position only when its digit is non-zero** (nothing is erased there). Score 0 with w=6
shows `00000` at x+12..x+71.

---

## 7. Sprite engine (sprite.c)

### 7.1 Sprite numbering (def.h:29-49)
`SPRITES = 18`: 0 = bonus; 1..7 = bags; 8..13 = monsters; 14,15 = fireballs; 16,17 = diggers.
Index 18 (`SPRITES`) is the "misc" pseudo-sprite used for static drawing (arrays that include it are sized `SPRITES+1`).
`TYPES = 5` in the order of `firstt/lastt` (sprite.c:278-279): **0 bonus, 1 bag, 2 monster, 3 fireball, 4 digger**
(`lastt` is exclusive).

### 7.2 Per-sprite record (sprite.c:12-27)
| field | size | meaning |
|---|---|---|
| `sprrdrwf[19]` | bool | "needs redraw" flag for the current operation |
| `sprrecf[19]` | bool | visited flag for the recursive overlap walk |
| `sprenf[18]` | bool | enabled (currently on screen) |
| `sprch[19]` | i16 | current image number |
| `sprmov[18]` | ptr | background save buffer supplied by the owner (drawing.c:24-25) |
| `sprx, spry[19]` | i16 | current position (x multiple of 4) |
| `sprwid, sprhei[19]` | i16 | current size (w in 4-px units) |
| `sprbwid, sprbhei[18]` | i16 | current collision border (shrinks the box on every side) |
| `sprnch, sprnwid, sprnhei, sprnbwid, sprnbhei[18]` | i16 | "next" attributes set by `initspr`, latched on the next draw |

Outputs: `int first[TYPES], coll[SPRITES]` (sprite.c:277).

**Borders are always 0**: every `createspr`/`initspr` call in the tree passes `bwid=0,bhei=0` (drawing.c:191-263,
monster_obj.c:114-133). `bcollide` therefore equals `collide` in practice; port the general form anyway.

### 7.3 Primitives
- `createspr(n,ch,mov,wid,hei,bwid,bhei)` (sprite.c:77-87): set both current and next attributes, attach buffer,
  `sprenf[n]=false`. Does not touch x/y or the screen. Called for all sprites by `creatembspr` (drawing.c:187-195,
  232-247) on every title loop and every `drawscreen`; this silently "forgets" enabled sprites without erasing them
  (the screen is cleared right before/after).
- `initspr(n,ch,wid,hei,bwid,bhei)` (sprite.c:153-160): set **next** attributes only.
- `clearrecf()` (sprite.c:201-206): `sprrecf[0..18]=false`. `clearrdrwf()` (sprite.c:193-199): `clearrecf()` + `sprrdrwf[0..18]=false`.
- `collide(bx,si)` (sprite.c:223-240), plain rectangle overlap on current rects, inclusive:
  ```
  if x[bx] >= x[si] { if x[bx] > (wid[si]<<2) + x[si] - 1 return false }
  else              { if x[si] > (wid[bx]<<2) + x[bx] - 1 return false }
  if y[bx] >= y[si] { return y[bx] <= hei[si] + y[si] - 1 }
  return y[si] <= hei[bx] + y[bx] - 1
  ```
- `bcollide(bx,si)` (sprite.c:242-259), same with borders:
  ```
  if x[bx] >= x[si] { if x[bx]+bw[bx] > (wid[si]<<2) + x[si] - bw[si] - 1 return false }
  else              { if x[si]+bw[si] > (wid[bx]<<2) + x[bx] - bw[bx] - 1 return false }
  if y[bx] >= y[si] { return y[bx]+bh[bx] <= hei[si] + y[si] - bh[si] - 1 }
  return y[si]+bh[si] <= hei[bx] + y[bx] - bh[bx] - 1
  ```
  All int16 promoted to int; use i32 in Rust. Note the asymmetric roles: which sprite's width is used depends on which
  is further left; which height on which is higher.
- `setrdrwflgs(n)` (sprite.c:208-221), transitive closure of overlap:
  ```
  if !recf[n] { recf[n]=true;
     for i in 0..18 { if enf[i] && i != n && collide(i,n) { rdrwf[i]=true; setrdrwflgs(i) } } }
  ```
  Consequences: (a) `n` itself becomes flagged if it is enabled and overlaps anything (the recursion from `i` finds `n`);
  (b) the misc sprite (18) is never flagged (loop stops at 17, and there is no `sprenf[18]`); (c) the closure uses whatever
  rect `n` currently has, whether or not `n` is enabled.
- `putis()` (sprite.c:269-275): for i in 0..18 ascending, if `rdrwf[i]`: `gputi(x[i],y[i],mov[i],wid[i],hei[i])` (restore background).
- `putims()` (sprite.c:261-267): for i in 0..18 ascending, if `rdrwf[i]`: `gputim(x[i],y[i],ch[i],wid[i],hei[i])`.
  **Z-order is therefore fixed by sprite index: bonus < bags < monsters < fireballs < diggers.**

### 7.4 Operations (exact step order)

`movedrawspr(n,x,y)` (sprite.c:89-105) — place a sprite, no collision output:
1. `x[n] = x & -4; y[n] = y`; latch next -> current (`ch,wid,hei,bwid,bhei`).
2. `clearrdrwf(); setrdrwflgs(n)` (overlaps at the **new** rect).
3. `putis()` — erase all flagged sprites (restore their saved backgrounds).
4. `ggeti(x[n],y[n],mov[n],wid[n],hei[n])` — save clean background under `n`.
5. `enf[n]=true; rdrwf[n]=true; putims()` — draw flagged sprites + `n` in index order.
- Does **not** update `first[]/coll[]`. Does not erase `n`'s old image if it was already enabled elsewhere (callers use
  it only for sprites that are off-screen). Quirk: if `n` was enabled and gets flagged in step 2, step 3 pastes its old
  buffer at the new position before step 4 re-grabs it.

`erasespr(n)` (sprite.c:107-116):
1. `if !enf[n] return`.
2. `gputi(n)` restore background; `enf[n]=false`.
3. `clearrdrwf(); setrdrwflgs(n); putims()` — redraw (not re-save) everything that overlapped it, transitively.
- Does not touch `first[]/coll[]`.

`drawspr(n,x,y)` (sprite.c:118-151) — move/animate a sprite and compute collisions:
1. `x &= -4; clearrdrwf(); setrdrwflgs(n)` — closure around the **old** rect (old x,y,wid,hei).
2. Save old `x,y,wid,hei`; temporarily set `x,y` new and `wid,hei = next wid,hei`; `clearrecf()` (keep rdrw flags);
   `setrdrwflgs(n)` — add closure around the **new** rect; restore old `x,y,wid,hei`.
3. `rdrwf[n]=true; putis()` — erase all flagged, including `n` at its old rect. **This happens even if `n` was not
   enabled** (stale buffer pasted at stale position); callers always `movedrawspr` first (e.g. title_anim.c:366-367,
   bags.c:71, monster_obj.c:147, digger_obj.c:44, bullet_obj.c:44).
4. `enf[n]=true`; set new `x,y`; latch next -> current (`ch,wid,hei,bwid,bhei`).
5. `ggeti(n)` at the new rect (clean background, since every overlapping sprite was erased in step 3).
6. `putims()` — redraw all flagged in index order.
7. `bcollides(n)`.

`bcollides(spr)` (sprite.c:281-299) — the collision output:
```
first[0..5] = -1; coll[0..18] = -1
for t in 0..5:
   next = -1
   for spc in firstt[t]..lastt[t]:
      if enf[spc] && spc != spr && bcollide(spr, spc):
         if next == -1 { first[t] = spc; next = spc }
         else { coll[next] = spc; next = spc; coll[spc] = -1 }
```
So `first[t]` is the lowest-numbered enabled sprite of type `t` whose box overlaps `spr`'s **new** box, and `coll[]`
chains the remaining ones of the same type in ascending order, -1 terminated. Other sprites' boxes are their current
on-screen rects. Results are global and are overwritten by the next `drawspr`; callers copy them immediately
(e.g. bags.c:245-247). `drawdigger` with an invalid `t` just sets `first[0..4] = -1` without touching `coll`
(drawing.c:358).

Since x is a multiple of 4 and widths are 16 (or 8 for fireballs) and all borders are 0, two 4x15 sprites collide iff
`|dx| <= 12 && |dy| <= 14`; for a fireball (8x8 at fx,fy) vs a 16x15 sprite at (sx,sy): `-4 <= fx-sx <= 12` (i.e.
`sx-fx<8 && fx-sx<16` on the 4-grid) and `sy-7 <= fy <= sy+14`.

### 7.5 Misc (static) drawing protocol
`initmiscspr(x,y,wid,hei)` (sprite.c:162-171): set rect of pseudo-sprite 18 (**x is not aligned here**),
`clearrdrwf(); setrdrwflgs(18); putis()` — erase every real sprite overlapping the area (transitively).
`drawmiscspr(x,y,ch,wid,hei)` (sprite.c:182-191): set rect with `x & -4`, set ch, `gputim(...)` — draw into the
(now clean) background. No flag handling.
`getis()` (sprite.c:173-180): for each flagged sprite ascending, `ggeti` (re-save its background, which now includes the
new static pixels); then `putims()` redraw them.
The standard sequence is `initmiscspr; drawmiscspr; getis` (drawing.c:218-306). `drawmiscspr` alone is used where no
sprite can overlap (background tiles drawing.c:313, life icons drawing.c:215).
All call sites pass x values that are already multiples of 4 (x*20+12 offsets by multiples of 4), so the alignment
mismatch between init and draw is latent.

### 7.6 Invariants worth asserting in the port
- A sprite's saved buffer always holds clean background (no other sprite's pixels), because every overlapping sprite
  (transitive closure) is erased before each `ggeti`.
- `rdrwf[i]` is only ever set for enabled sprites or for `n` after/just before its own `ggeti`; with the
  movedrawspr-before-drawspr discipline, `putis` never sees an unset buffer. In Rust make the buffer `Option<SavedRect>`
  and treat `None` as no-op (C would dereference NULL).
- Saved buffer carries its own dimensions (SDL behaviour, §4.4).

---

## 8. drawing.c

### 8.1 Field bitmap (shared with game logic)
`field[150]` of i16 plus per-player copies `field1/field2` (drawing.c:21-22). Bits: 0..4 = five horizontal sub-steps
(4 px each) still earth, 6..11 = six vertical sub-steps (3 px each), bit 13 (0x2000) = cell untouched;
`bitmasks[12]` clear bit 0..11 (drawing.c:27-28). `makefield` (drawing.c:65-81): -1, then level char 'S'/'V' -> `&= 0xd03f`
(vertical tunnel), 'S'/'H' -> `&= 0xdfe0` (horizontal tunnel). `eatfield(x,y,dir)` (drawing.c:144-185) clears sub-step bits as
things move and calls `incpenalty()`; this is game state, covered by the game-logic report; listed here because it
lives in drawing.c.

### 8.2 Level draw: `drawstatics` (drawing.c:83-96)
Copy the current player's saved field in, `gpal(0); ginten(0)`, `drawbackg(levplan())`, `drawfield()`.
- `drawbackg(l)` (drawing.c:308-315): `for y in (14..200).step_by(4) for x in (0..320).step_by(20) drawmiscspr(x,y,93+l,5,4)`.
  Tile = 20x4 logical (40x8 real), opaque. Last row y=198 is clipped at the bottom (rows 200,201 off-screen).
  `levplan()` in 1..8 (main.c:784-790).
- `drawfield()` (drawing.c:109-142): column-major (`for x { for y`), for each cell with bit 13 clear, with
  `xp=x*20+12, yp=y*18+18`:
  - if `(f & 0xfc0) != 0xfc0` (vertical tunnel): `f &= 0xd03f`; `drawbottomblob(xp, yp-15), (yp-12), (yp-9), (yp-6), (yp-3)`; `drawtopblob(xp, yp+3)`;
  - if `(f & 0x1f) != 0x1f` (horizontal): `f &= 0xdfe0`; `drawrightblob(xp-16,yp), (xp-12), (xp-8), (xp-4)`; `drawleftblob(xp+4,yp)`;
  - if `x<14` and right neighbour `(f & 0xfdf) != 0xfdf`: `drawrightblob(xp,yp)`;
  - if `y<9` and lower neighbour `(f & 0xfdf) != 0xfdf`: `drawbottomblob(xp,yp)`;
  - `redrawdelay()` (digger.c:300-311; a timed present, gives the "wipe-in" effect — coordinate with the timing agent).

### 8.3 Tunnel carving shapes (all mask-only, paint black; drawing.c:266-306)
| function | misc rect / draw at | ch | size (w,h) logical px |
|---|---|---|---|
| `drawrightblob(x,y)` | `(x+16, y-1)` | 102 | 2,18 = 8x18 |
| `drawleftblob(x,y)` | `(x-8, y-1)` | 104 | 2,18 = 8x18 |
| `drawtopblob(x,y)` | `(x-4, y-6)` | 103 | 6,6 = 24x6 |
| `drawbottomblob(x,y)` | `(x-4, y+15)` | 105 | 6,6 = 24x6 |
| `drawfurryblob(x,y)` | `(x-4, y+15)` | 107 | 6,8 = 24x8 |
| `drawsquareblob(x,y)` | `(x-4, y+17)` | 106 | 6,6 = 24x6 |
Each is `initmiscspr; drawmiscspr; getis`. Shapes (real pixels, `#` = carved; all 2x2-doubled) are rounded caps:
right/left blobs are half-ellipses 8 logical px wide; top/bottom are 24-wide tapering caps; "square" and "furry" have
a toothed top edge (furry also a toothed bottom). Bottom-row calls (`y=180`: rect y 195..200 / 195..202) are clipped
by the framebuffer.
Callers: digger.c:609-624, monster.c:326-348 (hobbin digging), bags.c:301-306 (falling bag: square+top, or furry).

### 8.4 Movable sprite helpers
- `creatembspr` / `initmbspr` (drawing.c:187-205, 232-264): bags ch 62 (4,15), monsters ch 71 (4,15), diggers ch 0 (4,15),
  bonus ch 81 (4,15), fireballs ch 82 (2,8); reset `digspr[]=0, digspd[]=1, firespr[]=0`.
- `drawdigger(n,t,x,y,f)` (drawing.c:338-359): animation counter ping-pongs 0,1,2,1,0...
  (`digspr += digspd; if digspr==2||digspr==0 digspd=-digspd;` clamp 0..2) — **advanced on every call, even for invalid t**.
  `nn = n==0 ? 0 : 31`. If `t` in {0,2,4,6}: `ch = (t + (f?0:1))*3 + digspr + 1 + nn` (t = direction code,
  f = can fire) -> `initspr(FIRSTDIGGER+n, ch,4,15,0,0); drawspr`. If `10 <= t <= 15`: `ch = 40 + nn - t`
  (t=15 -> 25 dead digger, 14..10 -> grave1..5 = 26..30) -> drawspr. Otherwise `first[0..4] = -1`.
- `drawgold(n,t,x,y)` (drawing.c:207-211): `initspr(FIRSTBAG+n, 62+t, 4,15,0,0); drawspr`.
- `drawfire(n,x,y,t)` (drawing.c:317-329): `nn = n==0 ? 0 : 32`; `t==0`: `firespr[n] = (firespr[n]+1) % 3` then
  `ch = 82+firespr+nn`; else `ch = 84+t+nn` (t=1..3 explosion); `drawspr(FIRSTFIREBALL+n,x,y)`.
- `drawbonus(x,y)` (drawing.c:331-336): `initspr(FIRSTBONUS,81,4,15,0,0); movedrawspr` (always at (292,18), digger.c:862).
- Monsters: `monster_obj_updspr` (monster_obj.c:105-137): alive nobbin `69+monspr`; alive hobbin right `73+monspr`,
  left `77+monspr`; dead nobbin 72; dead hobbin right 76; dead hobbin left 80 with **hei 14**; same ping-pong counter
  (monster_obj.c:76-92).
- `drawemerald(x,y)` / `eraseemerald(x,y)` (drawing.c:218-230): misc protocol with ch 108 / 109, size 4,10.

### 8.5 Status line (y = 0, glyph cells 12 px, all text opaque)
| x | content | source |
|---|---|---|
| 0..71 | player 1 score, `writenum(score,0,0,6,c)` (c=3 normal, 1 on update) | scores.c:214, 460-470 |
| 80,100,120,140 | P1 life slots `l*20+60`, l=1..4: ch 110 if a spare life remains else ch 112 (blank) | drawing.c:379-382 |
| 80 + text at 100 | if spare lives `n>4`: one icon at 80 and `"X%i"` at x=100 colour 2 | drawing.c:373-377 |
| 96..155 | erased first with `erasetext(5,96,0,2)` | drawing.c:372 |
| 108 | `"PLAYER 1"/"PLAYER 2"` flash at level start (8 cells), erased with `erasetext(8,108,0,3)` | main.c:181-204 |
| 124 | gauntlet mode: `"%3i:%02i"` minutes:seconds, colour 3, replaces lives entirely | drawing.c:365-370 |
| 164..223 | erased `erasetext(5,164,0,2)` when 2 players / 2 diggers | drawing.c:384, 398 |
| 224,204,184,164 | P2 life slots `244-l*20`: ch 111 (alternating 2-player) or ch 113 (simultaneous), else 112 | drawing.c:392-395, 405-408 |
| text ending at 220, icon at 224 | P2 with `n>4`: `"%iX"` right-aligned at `220-len*12` (colour 2, or 1 for simultaneous) | drawing.c:386-390, 400-404 |
| 236..307 or 248..319 | P2 score: x=236 if `<100000` else 248 | scores.c:215-218 |
Spare lives shown = `getlives(p)-1`. Other top-line messages: title `"D I G G E R"` at 100 (main.c:433), `"PRESS ANY KEY"`
at 80 colour 1 (main.c:838), `"GAME OVER"` at 104 (scores.c:287), `"TIME UP"` at 120 (scores.c:255).

---

## 9. Title screen

### 9.1 Picture (title_gz.c)
- `title_gz[2772]` (title_gz.c:8-155) is a **raw zlib stream** (header bytes `78 da`, despite the `_gz` name; not gzip,
  not RLE), inflated with zlib `uncompress()` to exactly **256000 bytes = 640x400, 8-bit palette indices, row-major**
  (title_gz.c:158-166) **[verified with Python zlib]**. zlib is a hard build dependency only for this (CMakeLists.txt:126-131).
- Content **[verified]**: colours {0,4,7,8,15}. A red (index 4) frame: horizontal bars at real rows 35..38 and 357..360,
  vertical bars at real columns 0..5, 320..327, 634..639 spanning rows 35..360 (two panels: high scores left,
  cast/animation right), and the "(C) Windmill Software" style copyright artwork in rows 363..398 (white/grey).
  Rows 0..34 are black (the top text line goes there).
- `vgatitle` overwrites the whole framebuffer with it (§4.8).

### 9.2 Title loop (main.c:425-490)
`gclear; gtitle; outtext("D I G G E R",100,0,3); shownplayers(); showtable()` then per frame:
`title_anim_step(&title_anim, ddap, frame); newframe(); frame++; if frame > title_anim_last_frame() frame = 0`.
Mode text at y=25 and y=39 (main.c:676-702); high-score table at x=16, y=25 header and `31+13*i` rows (scores.c:331-339).

### 9.3 title_anim.c — a frame-indexed script
- State `struct title_anim` (title_anim.h:10-21): two `monster_obj*`, an embedded `digger_obj` and `bullet_obj`, active
  flags and spawn frames.
- `title_anim_script[]` (title_anim.c:60-147): entries `{start,end,kind,obj,x,y,dx,dy,text}`; one-shots have start==end,
  spans apply every frame in [start,end]. Cycle length: frames 0..286 (`title_anim_cycle_last_frame`, title_anim.c:149).
- `title_anim_step(frame)` (title_anim.c:404-413): if `frame==0` reset (erase bullet, digger, monsters sprites 8 and 9);
  apply **all** script entries whose range contains `frame`, in array order; then `title_anim_animate`
  (title_anim.c:379-390): call `animate` on nobbin, hobbin, digger (only on frames after their spawn frame) and
  `drawfire` for an active bullet with `expsn < 4`.
- Timeline: 0 clear text (`erasetext(12,164,y,0)` for y=54..162 step 12, title_anim.c:176-182); 50 nobbin spawns at
  (292,63) facing left, 51-77 moves -4/frame to x=184, 78 turns right, 83 label "NOBBIN" at (216,64);
  90 second nobbin at (292,82), 91-117 left, 118-126 up 3/frame, 129-134 down, 137-142 up, 145 mutate to hobbin,
  146-154 down, 155 turn right, 156 "HOBBIN" (216,83); 130 digger at (292,101), 131-157 left, 163 "DIGGER" (216,102),
  166 turn right; 178 gold: `movedrawspr(FIRSTBAG,184,120); drawgold(0,0,184,120)`, 183 "GOLD" (216,121);
  198 `drawemerald(184,141)`, 203 "EMERALD" (216,140); 218 `drawbonus(184,158)`, 223 "BONUS" (216,159);
  228 digger turns up; 232 discharge + fire bullet (dir up, at digger.x+4, digger.y), 233-237 bullet -4/frame,
  237 kill hobbin, 238 explode, 243 remove bullet, 247 recharge; 251-270 same against the nobbin (bullet 252-260);
  274 digger turns right. Labels use colour 2.
- It exercises the real sprite engine and the monster/digger/bullet objects (other agents' areas) but never reads
  `first/coll` or `ggetpix`; it is purely cosmetic. The bag (sprite 1), emerald and bonus (sprite 0) drawn here are not
  erased at reset; they are wiped by the next `gclear`/`creatembspr` when the title loop restarts.

---

## 10. SDL backend plumbing (sdl_vid.c)

- **SDL version: SDL2 only** (CMakeLists.txt:137-144 `find_package(SDL2 REQUIRED)`, `-D_SDL`; `SDL_CreateWindow`,
  `SDL_Renderer`, `SDL_syswm.h`). There is a dead `#if defined(SDL_OLD)` SDL 1.2 fragment in `setmode`
  (sdl_vid.c:414-433). No SDL3.
- Window (sdl_vid.c:514-515): title `"D I G G E R"`, position undefined, **640x400**, flags 0 (not resizable;
  borderless only when reparented into an X11 window, sdl_vid.c:509-513, 175-193, option handled in main.c).
- Icon (sdl_vid.c:530-535): `Icon[4096]` in icon.c:3 is a 64x64 `uint32_t` ARGB8888 image (masks R=0x00FF0000,
  G=0x0000FF00, B=0x000000FF, A=0xFF000000) passed to `SDL_SetWindowIcon`.
- Surfaces: `screen16` 640x400x8 (the framebuffer), `screen` 640x400x32 ARGB (conversion target) (sdl_vid.c:536-549).
- `setmode()` (sdl_vid.c:411-464): `SDL_SetWindowFullscreen(window, FULLSCREEN_DESKTOP or 0)`; destroy and recreate
  `renderer = SDL_CreateRenderer(window,-1,0)` (no vsync flag, default driver) and
  `roottxt = SDL_CreateTexture(renderer, SDL_PIXELFORMAT_ARGB8888, SDL_TEXTUREACCESS_STREAMING, 640, 400)`;
  `SDL_ShowCursor(0)`.
- Present (`display_present_pixels_locked`, sdl_vid.c:231-238): `SDL_BlitSurface(screen16 -> screen)` (palette lookup to
  ARGB), `SDL_UpdateTexture(roottxt, NULL, pixels, pitch)`, `SDL_RenderClear`, `SDL_RenderCopy(renderer, roottxt, NULL, NULL)`,
  `SDL_RenderPresent`. **Scaling**: the texture is stretched to the whole output; no logical size, no integer scaling, no
  aspect-ratio correction, no scale-quality hint (SDL default = nearest). Windowed it is 1:1 at 640x400.
- **Fullscreen toggle**: Alt+Enter (sdl_kbd.c:68-74) -> `switchmode()` (sdl_vid.c:466-495) flips the `SDL_FULLSCREEN` bit
  in `addflag` and re-runs `setmode` (falls back to the previous mode on failure, exits if both fail). `/F` pre-sets the
  flag (`sdl_enable_fullscreen`, sdl_vid.c:772-778).
- **Async display thread** (default on; INI `[Graphics] Async`, main.c:1292): `display_async_start` (sdl_vid.c:305-342)
  creates thread `"digger-display"`, which owns the renderer/texture (it performs `setmode` itself via
  `mode_change_pending`). `display_submit_frame` (sdl_vid.c:385-400) copies palette + pixels of `screen16` into
  `pending16` under a mutex, bumps a sequence number and signals; the thread swaps `pending16/work16` and presents
  (sdl_vid.c:251-293). It is a single-slot mailbox: unpresented frames are overwritten (frame dropping).
  `doscreenupdate(true)` spins with `SDL_Delay(0)` until `completed_frame_seq >= seq` (sdl_vid.c:402-409). If thread
  creation fails, falls back to synchronous presenting.
- **When frames are presented**: once per game tick from `gethrt()` (sdl_timer.c:54-89: `doscreenupdate(false)` then a
  PLL-computed `SDL_Delay`), also from `redrawdelay()` during level wipe-in, and explicitly via `gflush`/`gflush_sync`
  (call sites in §1). Drawing primitives never present by themselves.
- Tests run with `SDL_VIDEODRIVER=dummy` (scripts/do-test-run.sh:24): the full pipeline still renders into `screen16`.

---

## 11. Emscripten and FreeBSD backends

### Emscripten (ems_vid.c, 117 lines)
Not a separate renderer: the browser build uses the same sdl_vid.c. Differences:
- async thread disabled (`use_async_screen_updates = false`, sdl_vid.c:551-553) — synchronous present;
- `setmode` skips `SDL_SetWindowFullscreen` (sdl_vid.c:434-440); `switchmode()` -> `ems_vid_switchmode()`
  (ems_vid.c:105-109) which asks the browser for fullscreen with `EMSCRIPTEN_FULLSCREEN_SCALE_STRETCH`
  (ems_vid.c:53-75); a fullscreenchange callback keeps the flag in sync and restores the canvas to 640x400
  (ems_vid.c:30-51); `digger_request_fullscreen()` is exported for the HTML shell (ems_vid.c:111-117);
- `sdl_enable_fullscreen` is a no-op (sdl_vid.c:775-777); `SDL_Delay` becomes `emscripten_sleep` (sdl_timer.c:9-13).

### FreeBSD console (fbsd_vid.c, libvgl)
Same `vga*` function set over an off-screen 640x400 `VGLBitmap` (`sVGLDisplay`, fbsd_vid.c:141):
- identical scaling (`wratio=2` applied to `w*4`, fbsd_vid.c:33-41, 100), identical `vgaputim` formula
  (fbsd_vid.c:353-379), identical `vgagetpix` (via a 1x1 `geti`, fbsd_vid.c:333-351), identical `vgawrite` recolouring
  (fbsd_vid.c:381-420), identical `vgatitle`;
- save buffers hold a `VGLBitmap*` in the caller's byte buffer, same trick (fbsd_vid.c:263, 317-330);
- `vgaputi` records a dirty rectangle (merge-on-overlap list, fbsd_vid.c:233-308) and `doscreenupdate` copies only
  dirty rects to the real display (fbsd_vid.c:210-231); `wait_for_present` ignored;
- 640x400 VESA mode, or 640x480 with `yoffset=40` centring applied only at present time (fbsd_vid.c:128-137, 220-221);
- different palette RGB values (§3); `vgaclear` clears both buffers immediately.

### What the portable contract really is
1. A 640x400 array of palette indices is the single source of truth; logical API is 320x200 with `w` in 4-px units.
2. `geti/puti` = save/restore a rectangle of indices (opaque, clipped); the buffer representation is private to the backend.
3. `putim` = `if img != 0xff { dst = (dst & mask) | img }` against `vgatable`.
4. `getpix` = the 8x2 bit-gather with `& 0xee` and the `>319/>199 -> 0xff` rule.
5. `write` = opaque 24x24 glyph with the 4-way recolour.
6. `pal`/`inten` select one of 4 palettes, applied to the whole frame at present time; exact RGB is backend-defined.
7. Presentation is decoupled (dirty rects, threads, sync/async are all optional); nothing in game logic observes it
   except for timing (`gflush_sync` blocks until shown).
8. Window management (fullscreen, scaling, icon) is entirely outside the contract.

---

## 12. Suggested Rust design

### 12.1 Assets
- Write a one-off converter (an `xtask` or a checked-in Python script, run manually; keep the generated files in the
  repo so `cargo build` needs neither the C tree nor a `build.rs` C parser). The C syntax is trivial:
  `static const uint8_t NAME[]={0x..,...};` plus the pointer tables. A regex parser handled all three files for this report.
- Suggested outputs:
  - `assets/vga_sprites.bin`: concatenation of the unique arrays; plus a generated `sprites.rs` with
    `pub struct SpriteDef { img: Range<u32>, mask: Range<u32>, w: u8 /*4px units*/, h: u8 }` and
    `pub const VGA_TABLE: [SpriteDef; 120]`. Take w,h from the table in §5.2 (they are not stored in the C data; derive
    from byte length and assert `len == 8w*2h`). Total unique sprite data is about 150 KB.
  - `assets/vga_font.bin`: 40 glyphs x 576 bytes, plus `const ASCII2GLYPH: [Option<u8>; 95]`.
  - `assets/title.zz`: the 2772-byte zlib stream verbatim, inflated at startup with `miniz_oxide` (pure Rust), or
    pre-inflated at conversion time and re-compressed any way you like (raw is 256 KB; the image is trivially RLE-able).
    Assert the output length is 256000.
  - `assets/icon.rgba` (64x64 ARGB u32, convert to the byte order the windowing crate wants).
  - Embed with `include_bytes!`. Add a unit test that hashes each blob so accidental regeneration differences are caught.
- Preserve the `vgazero480` aliasing semantics by value, not identity (several table entries share it with different sizes;
  slicing the first `8w*2h` bytes is enough).
- Do not port the CGA data unless a real CGA renderer is wanted (it would need new code: nothing in the C tree draws it).

### 12.2 Core types
```rust
pub const FB_W: usize = 640; pub const FB_H: usize = 400;

pub struct Framebuffer { pub pix: Box<[u8; FB_W*FB_H]>, pub pal: u8 /*0|1*/, pub intense: bool }

pub struct SavedRect { w: u16, h: u16, pix: Vec<u8> }      // real-pixel size; replaces the pointer-in-buffer hack

impl Framebuffer {                                           // pure, deterministic, no I/O — the "vga*" functions
    pub fn clear(&mut self);
    pub fn set_pal(&mut self, pal: i16);                     // also resets intensity
    pub fn set_inten(&mut self, inten: i16);
    pub fn get_image(&self, x: i16, y: i16, w: i16, h: i16) -> SavedRect;   // clipped, zero-filled outside
    pub fn put_image(&mut self, x: i16, y: i16, img: &SavedRect);           // clipped, opaque, uses img's own size
    pub fn put_masked(&mut self, x: i16, y: i16, ch: i16, w: i16, h: i16);  // clipped
    pub fn get_pix(&self, x: i16, y: i16) -> i16;
    pub fn write_char(&mut self, x: i16, y: i16, ch: i16, c: i16);
    pub fn title(&mut self);
    pub fn rgba_palette(&self) -> &'static [[u8; 3]; 16];
}

pub trait Video {                 // presentation only
    fn present(&mut self, fb: &Framebuffer, wait: bool);     // doscreenupdate
    fn toggle_fullscreen(&mut self) {}
    fn shutdown(&mut self) {}
}
```
Rationale: every backend in C re-implements identical pixel logic; in Rust keep all pixel logic in one
backend-independent `Framebuffer` and make the `Video` trait only "show these 256000 indices with this palette".
If a closer structural match to `digger_draw_api` is preferred, a `trait DrawApi` with the 12 methods can be a thin
wrapper struct `{ fb: Framebuffer, video: Box<dyn Video> }`.

- `SpriteEngine` struct owning the arrays of §7.2 (`[SpriteRec; 19]`, `saved: [Option<SavedRect>; 18]`,
  `first: [i32; 5]`, `coll: [i32; 18]`); methods mirror sprite.c one-to-one and take `&mut Framebuffer`. Keep the
  recursion in `setrdrwflgs` (depth <= 19) or an explicit stack; iteration order must stay ascending.
- `Drawing` (drawing.c state: `digspr/digspd/firespr`, field arrays) on top.
- Backends: `HeadlessVideo` (no-op `present`, optionally counts frames / dumps PPM or a CRC of `pix` + palette id for
  golden-image tests) and a windowed one. For a windowed backend either `sdl2` crate (closest to the original:
  ARGB8888 streaming texture 640x400, `canvas.copy(&tex, None, None)`) or `minifb`/`pixels`+`winit` (pure Rust, simpler
  build). Expand indices through the 16-entry palette into a `[u32; 256000]` each present; that costs microseconds, so
  the C async thread is unnecessary — present synchronously, maybe with vsync off to keep the game's own timing.
- Since the title screen/SDL dummy-driver tests in C render fully, the headless backend gives a direct equivalence
  check: dump `screen16` from an instrumented C build at chosen ticks and compare byte-for-byte with the Rust framebuffer.

### 12.3 Tests worth having
- Asset integrity (lengths, value sets as in §5.1/§6, table size 120/95).
- `put_masked` clipping at the bottom edge (bottom blob at y=180) and `get_image`/`put_image` round trip.
- `get_pix` vectors: black -> 0; full 8x2 block -> 0xee; x=320 -> 0xff.
- Sprite engine: property test against a direct transliteration (or against recorded `first/coll` traces from the C
  build) for random sequences of movedrawspr/drawspr/erasespr; invariant "after erasing all sprites the framebuffer
  equals the static background".
- Golden framebuffer CRCs for: title frame 0 and 286, level 1 after `drawscreen`, status line with 0..9 lives.

---

## 13. Trickiest parts / gotchas checklist

1. **Framebuffer is game state.** `ggetpix` (fireball vs wall/earth) reads rendered pixels (§4.6). The headless backend
   must render everything bit-exactly, including text and sprites; you cannot stub drawing in tests or netsim replays.
2. **`ggetpix` details**: 8 real columns starting at `2x` (unaligned), OR over 2 rows, `& 0xee`, `0xff` beyond
   right/bottom, undefined for negative coordinates and row wrap at x>=317. Decide and document the out-of-range behaviour.
3. **Sprite engine ordering** (§7.4): closure at old rect, then at new rect with only `recf` cleared; `putis` before
   `ggeti`; `putims` ascending index = z-order; `bcollides` only in `drawspr`. `first/coll` are globals overwritten by every
   `drawspr` and partially by `drawdigger`'s fallthrough (`first` only).
4. **Current vs "next" attributes**: `initspr` only sets next values; they are latched by `movedrawspr`/`drawspr`.
   Collision of other sprites against `n` uses `n`'s *current* latched size (matters for ch 80 with hei 14).
5. **`movedrawspr` does not compute collisions and does not erase an already-enabled sprite**; `drawspr` on a
   never-placed sprite would paste a stale/NULL buffer. Keep the C call discipline; make `None` buffers a no-op.
6. **Saved buffer keeps its own size** (SDL ignores w/h in `vgaputi`). Use `SavedRect` dimensions when restoring.
7. **Clipping**: bottom-row blobs and the last background tile row extend past y=199; `geti` must zero-fill outside,
   `puti`/`putim` must clip. Nothing extends past the left/right edges in normal play.
8. **x alignment** `x & -4` in `movedrawspr`/`drawspr`/`drawmiscspr` only (Rust: `x & !3` on i16, same for negatives).
   `initmiscspr`, `gwrite`, `ggetpix` do not align.
9. **Masked blit formula** uses 0xff in the *image* as the transparency key in addition to the mask; background tiles
   and the bonus use an all-zero mask (fully opaque rectangle); eraser sprites use an all-zero image.
10. **`gpal` resets intensity; `ginten` keeps the palette**; neither touches pixels; effect is visible only after the
    next present. `flashywait` relies on `gflush_sync` actually showing each toggle.
11. **Text is opaque** and unknown characters become spaces; lower case maps to upper case; `-` is not available;
    `writenum` never draws/erases a leading zero in the leftmost cell.
12. **CGA mode is a stub** in all surviving backends (black screen, `ggetpix`=0 changes gameplay). Recommend not
    porting the `/C` switch, or porting it as "ignored".
13. **The `_gz` title is zlib, not gzip**; inflate to 256000 bytes of indices at real resolution.
14. Animation counters live in the drawing layer (`digspr/digspd`, `firespr`, monster `monspr/monspd`) and advance per
    draw call — they affect which pixels are on screen and therefore (via item 1) potentially gameplay; port them exactly.
15. `creatembspr` re-creation marks all sprites disabled without erasing; it relies on a `gclear` around it
    (main.c:429-431, 749-750). Keep the same call order.
16. Async presenting, dirty rects, renderer/texture recreation on mode switch, X11 reparenting and the Emscripten
    fullscreen strategy are all presentation details with no effect on game state; do not replicate them.
