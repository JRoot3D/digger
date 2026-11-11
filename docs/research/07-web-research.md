# 07 — Web research: Digger sources, PortMaster packaging, Rust ecosystem (2026-11-11)

## 0. Access and how to read this report

**Update (second pass, same day):** network access was widened mid-task. `https://digger.org/` (apex host;
`www.` is still blocked) and `https://portmaster.games/` are now reachable and were fetched directly with
curl. Parts A and B below were rewritten from those primary pages. Still not reachable: `web.archive.org`,
`blog.rust-lang.org`, `en.wikipedia.org`. GitHub API/MCP is limited to `jroot3d/digger`; other repos were
read via raw URLs, HTML pages and `git clone`.

Evidence labels:

- **[P]** primary page/file fetched in this session from digger.org or portmaster.games (URL given).
- **[V]** verified in another primary file (upstream git repo, PortMaster repo raw files, crates.io API, Rust channel manifests).
- **[S]** search-engine snippet only — a lead, not a fact.
- **[U]** unverified / inference / from memory.

Local copies (untrusted downloads, reference only), all under
`a temporary download directory`:
- `sobomax/src` — sobomax/digger @ `638125e` (2026-10-14), shallow clone
- `pm/` — PortMaster `ports/digger` files
- `web/` — digger.org pages (`index`, `download`, `faq`, `dos`, `unix`, `windows`, `whatsnew`, `remaster`…),
  `web/digsrc/` = unpacked `https://digger.org/digsrc.zip` (Jenner's 2004 C source incl. `COPYING`,
  `digsrc.txt`, `drf.txt`), and PortMaster wiki pages + `port_details.json`, `device_info.json`

---

## Part A — digger.org and Digger Remastered

Sources fetched: https://digger.org/ , https://digger.org/download.html , https://digger.org/faq.html ,
https://digger.org/dos.html , https://digger.org/unix.html , https://digger.org/windows.html ,
https://digger.org/whatsnew.html , https://digger.org/remaster.html , https://digger.org/digsrc.zip [all P].
Supplement: `digger.txt` in the upstream repo https://github.com/sobomax/digger/blob/master/digger.txt [V] —
header dated 13/3/2002 but edited by sobomax since (documents eDRF, NetSim, `/T`), so it describes current
upstream rather than Jenner's last release.

### A.1 State of the site
The site is static and effectively frozen: the newest "What's New" entry is **31 Jan 2016** (link to a
ColecoVision version; `ramon.drf` added to the Hall of Fame) [P whatsnew]. The index page carries a "Help
wanted" notice: Jenner has no time and "the Digger community desparately needs someone new to take over the
project" [P index]. Active development today is in sobomax/digger (last commits 2026-10-14) [V git log].

### A.2 Versions / ports (download page unless noted) [P]

| Version | Author | File / note |
|---|---|---|
| Original Digger, 1983, copy-protected bootable 5.25" floppy, needs genuine CGA | Windmill Software | index; ripped copy `digrip.zip` (23K); original source `digsrc_orig.zip` (295K) |
| Digger Remastered for DOS (1998-), real mode | Andrew Jenner | `digger.zip` (71K), "latest version" — no version number on the page; source files dated Sep 2004, copyright 1998-2004 [P digsrc.zip] |
| DJGPP/Allegro protected-mode DOS port: joysticks, config menu, 256-colour graphics; "completely unsupported" | — | `digprot.zip` (299K) |
| WinDig, Windows 95 version | Tim Draper (last version 20020310, released 5 Apr 2003 [P whatsnew]) | `windig.zip` (87K), older `owindig.zip` (95K); sources `winsrc.zip` (160K) |
| Unix/SDL version, 20020314 | Maxim Sobolev | `digger-20020314.tar.gz` (107K); Red Hat binary by Jeroen Nees; Debian packages (Bijan Soleymani; Nikolay Morozov 2012, i386/amd64); PLF RPMs; "Per Oyvind Karlsen is updating the Unix version on gitorious" |
| Darwin/Mac OS X patches and package | Markus W. Weissmann | `darwin-digger-patches.tar.bz2`, `digger.sit` (405K), MacPorts |
| OS/2 and eComStation | Andrey Vasilkin | `o2digger.zip` (69K) + source `o2winsrc.zip` (177K) |
| RiscOS (out of date, no CGA, coarse sound) | — | `armdig.zip` (52K) |
| AmigaOS 68k / MorphOS PPC (from the SDL version) | LouiSe | `amigadig.lha` |
| Java applet / downloadable | Marek Futrega | `digjava.zip` (45K), source `digjsrc.zip` (34K) |
| Sega Dreamcast, GP2X, JavaME phones, Open Pandora (Roy Gillotti, based on Unix version) | third parties | external links |
| Dingoo, DiggerDS, Digger XP, Javascript (Futrega), Android (Runar Holen), Raspberry Pi, ColecoVision | third parties | links announced on whatsnew [P] |
| "Digger Reloaded" (SDL2; Linux/FreeBSD/Windows/macOS/WebAssembly; eDRF; SIP/UDP NetSim) | Maksym Sobolyev, Michael Knigge | https://github.com/sobomax/digger [V] — **not mentioned on digger.org** |

Upstream version string is still `DIGGER_VERSION "MS MPL 20190212"` (`def.h`) [V].

### A.3 Other things the download page offers [P]
- **C source** `digsrc.zip` (175K; "Please read the DIGSRC.TXT file first").
- **Graphics**: `digbmps.zip` (.BMP, 24K), `diggifs.zip` (.GIF, 35K); `gedit.zip` graphics editing package
  (62K) with BMPs of all game graphics.
- **Extras pack** `digextra.zip` (415K): level editor, extra levels, icons, Win95 file associations, hall of
  fame games (DRF recordings), screensaver, score-extraction utility.
- Styx Remastered (`styx.zip`, asm source `styxsrc.zip`, original source `styxsrc_orig.zip`); ripped
  Windmill games (Styx, Rollo, Conquest, Moonbugs, Exterminator, Floppy Frenzy, Video Trek 88, Attack on
  Altair), Windmill demo disk, PC BOSS; mailing-list archives; Ori Brost's "BHole" emulator.

### A.4 Version-history highlights [P whatsnew; older entries identical in digger.txt]
- 20 Jul 1998 site/What's New starts; 28 Jul 1998 DRF files; 15 Oct 1998 "Major new version" with the new
  recording/playback (needed for the Hall of Fame); 16 Oct 1998 RiscOS, speed control, playback cheat.
- 13 Nov 1998 `/K`; 25 Nov 1998 Gauntlet; 13 Dec 1998 two-player simultaneous (`/2`).
- Jan 1999 SoundBlaster support, `.DAT` -> `.DLF`, `/U` and `/I`; 28 Mar 1999 Windows 95 version.
- 6 May 1999 protected-mode version; **24 May 1999 source code published**; 29 Jul 1999 Java applet.
- 7 Apr 2000 SDL version (FreeBSD/Linux); 14 Mar 2002 new Unix/SDL release; 5 Apr 2003 last WinDig.
- 11 May 2003 Amiga/MorphOS; 18 Jan 2004 Darwin/Mac OS X; **4 Sep 2004 original Windmill source for Digger
  and Styx added**; 2006-2016 only links to third-party ports (GP2X, DS, Dingoo, Pandora, OS/2, Android,
  JS, ColecoVision).
- How the remaster was made [P remaster]: disassemble (DEBUG) -> hand-decompile to C -> "deshroud" ->
  rewrite; VGA graphics **redrawn by Jenner in 16-colour 640x400** with his CHARDES sprite editor; built
  with Borland C++ 4.52 + A86.

### A.5 Licence and copyright — important for a Rust repo

**Remastered code — GPL, version 2 or later [P].**
- Download page, on `digsrc.zip`: "This source code was produced without seeing the original Digger source
  code, and I own the copyright on it, not Windmill Software. I am hereby releasing this code under the
  terms of the GPL."
- `digsrc.zip` contains `COPYING` = GNU GPL **Version 2, June 1991**, and `digsrc.txt` section "Copyright":
  "With the permission of Rob Sleath, Digger Remastered is Copyright (c) Andrew Jenner 1998-2004", followed
  by the standard notice "either version 2 of the License, or (at your option) any later version".
  => SPDX **GPL-2.0-or-later** for Jenner's code. (This resolves the earlier open question.)
- FAQ "Is this legal?": Rob Sleath (original author) no longer owns the copyright but "did retain the right
  to use the code for development of other products", "has granted me copyright to Digger Remastered, which
  I have chosen to license under the GNU GPL."
- **Internal inconsistency to record:** the introduction of the same `digsrc.txt` still says "Windmill
  Software owns copyright on this source code ... You cannot make any money from any versions of Digger you
  create using this code unless you have permission ... All rights to any software created with this code
  must be relinquished to Windmill Software upon request." That reads as older text left in place beside
  the later GPL grant; the download page and the Copyright section are the clearer, later statements. A
  non-commercial free port is consistent with both.

**Original 1983 code/binaries — not licensed [P].** `digsrc_orig.zip`: "the copyright on this still belongs
to Windmill software, and ... we do not have permission to distribute Digger in its original form. This
source code is provided for historical interest only. If you are interested in making a derivative version
of Digger, please start with the Digger Remastered source code instead." FAQ: "My distributing of the
original games is, strictly speaking, not legal." => **Do not consult or copy `digsrc_orig.zip` or the
ripped binaries for the Rust port**; port from the Remastered/sobomax C only.

**Graphics and sound assets.**
- VGA sprites were redrawn by Jenner [P remaster] and live inside the GPL'd source (`vga.spr`,
  `vgatext.spr`, `vtitle.bmp` in digsrc.zip; `vgagrafx.c`, `title_gz.c` upstream) — covered by the GPL grant.
- CGA sprites (`cga.spr`, `cgagrafx.c`) and the sound/music tables (`sound.c`) are extracted from the 1983
  game; they ship inside the same GPL source archive with no separate statement. Their status rests on
  Sleath's permission as relayed by Jenner; Windmill's residual rights in the *original* artwork are not
  expressly waived anywhere on the site. The standalone `digbmps.zip` / `diggifs.zip` / `gedit.zip` archives
  carry **no licence statement on the page**.
- Music is third-party compositions [P faq / digger.txt]: "Popcorn" (Hot Butter hit; composition by Gershon
  Kingsley, still in copyright [U]), Rossini's William Tell Overture and Chopin's Funeral March (public
  domain compositions [U]). Every existing port ships the Popcorn note table; the risk is inherited.

**sobomax/digger additions [V].** No top-level LICENSE file; README: code is under "several licenses
including Public Domain, Beer-Ware, 2-clause BSD and GNU General Public License version 2". Headers:
Jenner files carry only the copyright line; `digger_obj.c`/`monster_obj.c`/`bullet_obj.c`/`digger_types.h`
are 2-clause BSD (Windmill/Jenner/Sobolev copyright lines); `sdl_vid.c`, `sdl_kbd.c`, `ems_*.c` Beer-Ware;
`soundgen.c`, `spinlock.c`, `digger_math.c` Sippy Software (licence body not read [U]); `edrf*.c`,
`netsim*.c`, `state_hash.c` "Copyright (c) Maksym Sobolyev" (licence body not read [U]).

**Practical reading (not legal advice):** license the Rust repo **GPL-2.0-or-later**, ship `COPYING`,
keep the notices "Copyright (c) Andrew Jenner 1998-2004", "Portions Copyright (c) 1983 Windmill Software
Inc." and Sobolev's BSD/Beer-Ware notices where code is derived from those files; embed asset tables
converted from the Remastered source only; state the asset provenance and the Popcorn caveat in the README.
Before porting eDRF/NetSim code, read those files' full headers.

### A.6 Recording (.drf) and command-line switches

Jenner's DOS switches [P dos.html]; sobomax additions [V digger.txt + `main.c` l.950-1170]. `/X` or `-X`,
case-insensitive, **processed in order** (`DIGGER 20 /P:A.DRF 10 /E:B.DRF`).

| Switch | Meaning | Source |
|---|---|---|
| `/S:n` or bare number | speed; default 40, lower = faster; `+`/`-` change by 5 in game | P |
| `/L:name` or bare name | level file (`.DLF`) | P |
| `/C` | CGA graphics | P |
| `/B` | CGA with BIOS palette (DOS; upstream treats as `/C`) | P / V |
| `/Q` | quiet (F9 still toggles) | P |
| `/M` | music off (F7 still toggles) | P |
| `/R:name` | record to file (upstream: `.edrf` suffix selects eDRF) | P / V |
| `/P:name` | play back then restart program | P |
| `/E:name` | play back then exit (upstream exit status: 0 complete, 1 unreadable, 3 stopped early; eDRF adds 2 diverged, 4 recording ends early) | P / V |
| `/O` | loop the command line (screensaver) | P |
| `/K`, `/KA` | redefine keys / all keys (stored in `DIGGER.INI`) | P |
| `/G:time` | Gauntlet, seconds; default 120, max 1 hour | P |
| `/2` | two simultaneous diggers | P |
| `/A:1,port,irq,dma,rate,length` | SoundBlaster (DOS); 2002 Unix build needed `/A` to enable sound [P unix] | P |
| `/V` | sync timing to screen refresh | P |
| `/U` | unlimited spare lives | P |
| `/I:level` | start level (no hall of fame) | P |
| `/?` | help | P |
| `/T:tick` | take over playback at tick N | V (upstream only) |
| `/N:...` | two-player NetSim over SIP/UDP | V (upstream only) |
| `/F`, `/X:xid` | fullscreen; embed in X11 window | V (upstream only) |

Recording behaviour [P dos / V digger.txt]: F8 on the title screen saves the last game (file name from score
+ initials) if no name was given; only the last game before exit is kept; `/R` before `/P`/`/E` re-records;
Ctrl-T takes over a playback (then no recording / hall of fame). WinDig uses T instead of Ctrl-T [P windows].

**DRF file format** (`drf.txt` inside digsrc.zip) [P] — text, max 79 chars/line:
1. `DRF`
2. version string, e.g. `AJ DOS 19981013` (compiler initials, platform, yyyymmdd)
3. mode: `1`, `2`, `M2` (simultaneous), `Gx` (gauntlet x seconds), `M2Gx`; optional `U` and `Ix`
4. bonus score (decimal)
5. lines 5-84: level data, 15 chars per line, 8 screens x 10 rows
6. per round (round ends on level complete, death or exit): 8-hex-digit **random seed**, then RLE input
   lines — `u d l r` directions, `s` stopped, capital = firing, optional repeat count; one letter per frame
   ("default speed is roughly 15 frames per second") — then `EOL`
7. `EOG`, then optionally the high-score initials.
It records inputs + seeds + level data, not output; pauses, title screen and initials entry are not recorded.

=> **Acceptance harness:** the engine must be deterministic from (level data, seed, per-frame input). Replay
hall-of-fame `.drf` files (from `digextra.zip` / fame page) and upstream `tests/` recordings with `/E:` and
compare final score / eDRF state hashes against the C build.

### A.7 Game mechanics usable as acceptance criteria [P faq.html unless noted; same text in digger.txt]

Scoring: emerald 25; eight consecutive emeralds +250; gold 500; Nobbin/Hobbin killed by shot or bag 250;
bonus 1000; in bonus mode eating monsters 200, 400, 800, 1600 (other kills still 250); extra life every
20,000.

- Level ends when all emeralds are collected or all monsters destroyed (original instructions [V digger.txt]).
- Level plan 1-2-3-4-5-6-7-8-6-7-8, then 5-6-7-8 repeated 247 times, then always layout 5.
- Difficulty scales to level 10 then stays: monster arrival rate, total monsters, max on screen,
  nobbin-crossings-to-hobbin, monster speed (random), chase persistence (always chase from level 6), hobbin
  duration, gold lifetime, fire recharge, bonus duration.
- Max 5 monsters on screen (original allocated 6).
- Max 4 spare lives without `/U`; no extra lives at or after 1,000,000 points (original bug kept).
- Player 1 gets the extra life *at* multiples of 20,000; player 2 only *after*.
- Gold disappears very quickly if you dig underneath it.
- Monsters going up reverse when a bag is falling in their column, even if the bag is below them.
- Level-complete sound is polyphonic if no background music has played since start.
- Max score at end of level 1: 8,650 (+3,900 per life used); end of level 2 without dying: 19,925.
- Gauntlet: infinite lives; timer stops on pause and between levels, not on death; +15 s instead of extra
  life; separate high-score table; replay at recorded speed.
- Two-player simultaneous: dead digger reincarnates after a while, flashing and invincible briefly.
- Timing is from the hardware timer, independent of screen load; music/sound independent of game speed.

Default keys [P dos / V digger.txt]: arrows move, F1 fire, Space pause, F7 music, F9 sound, F10 title,
`+`/`-` speed, Ctrl-T take over (Alt-Enter fullscreen upstream); player 2 W/A/S/Z + Tab; title screen N
(mode), K (keys), F8 (save recording), F10 (exit). Upstream SDL build has **no joystick/gamepad code** [V grep].

Upstream technical baseline [V source]: 640x400 ARGB8888 streaming texture, `SDL_CreateRenderer(window,-1,0)`,
`SDL_RenderCopy(..., NULL, NULL)` (stretch to output, no aspect handling), fullscreen =
`SDL_WINDOW_FULLSCREEN_DESKTOP`; audio = SDL callback, mono S16, 44100 Hz default from INI; config
`$HOME/.digger.rc`, scores `$HOME/.digger.sco` on Unix (`DIGGER.INI` / `DIGGER.SCO` on DOS/Windows).

---

## Part B — PortMaster port (`ports/digger`)

Sources: https://portmaster.games/port/?name=digger [P] — the page is a JavaScript shell that renders from
https://portmaster.games/assets/json/port_details.json (entry `digger`) and
https://portmaster.games/assets/json/device_info.json plus the repo README; I read those JSON files
directly. Repo files [V]: https://github.com/PortsMaster/PortMaster-New/tree/main/ports/digger
(`port.json`, `README.md`, `Digger.sh`, `gameinfo.xml`, `digger/digger.gptk`, `digger/licenses/`).
Guides [P]: https://portmaster.games/contribute/porting/packaging/ ,
https://portmaster.games/contribute/porting/build-environments/ ,
https://portmaster.games/contribute/porting/what-can-be-ported/ .

### B.1 Metadata
`port_details.json` entry [P]: title "Digger"; repo `PortsMaster/PortMaster-New`; porters **EvilLurker,
Slayer366**; genres action, arcade; `reqs: []`; `runtime: []`; **`arch: ["aarch64","x86_64"]`**; `rtr: true`;
**dateAdded 2024-12-07, dateUpdated 2025-08-14**; 4,206 downloads (45/week); download
`https://github.com/PortsMaster/PortMaster-New/releases/download/2025-08-14_1207/digger.zip`.
`port.json` [V] agrees: `version: 4`, `name: digger.zip`, `items: ["Digger.sh","digger"]`, desc "Digger port
based on the DOS version (remastered)", inst "Ready to run. Source: https://github.com/sobomax/digger",
`exp: false`, **`min_glibc: ""`**, `availability: "full"`. Commits: added Dec 2024, x86_64 added Aug 2025 [V].

### B.2 Upstream and build
Packages **sobomax/digger** (SDL2 build), compiled with `make -j4` after three edits [V README]:
1. `def.h`: add `#include <SDL2/SDL.h>`; in `ININAME` replace `getenv("HOME")` with `SDL_GetBasePath()`.
2. `scores.c`: same replacement in `SFNAME`.
3. `GNUmakefile`: drop `-DDIGGER_DEBUG`.

Runtime: plain **SDL2 from the device firmware** (no gl4es, no box86/64, no mono/godot runtime). The only
bundled library is `libz.so.1` in `digger/libs.aarch64/` (and `libs.x86_64/`) [V dir listing]. Binaries are
`digger.aarch64` and `digger.x86_64`. Licences folder: `LICENSE.GPL2.txt`, `LICENSE.digger.txt`,
`LICENSE.libz(zlib1g).txt` [V].

### B.3 Launch script (Digger.sh) — full contents [V]
```bash
#!/bin/bash
XDG_DATA_HOME=${XDG_DATA_HOME:-$HOME/.local/share}
if [ -d "/opt/system/Tools/PortMaster/" ]; then
  controlfolder="/opt/system/Tools/PortMaster"
elif [ -d "/opt/tools/PortMaster/" ]; then
  controlfolder="/opt/tools/PortMaster"
elif [ -d "$XDG_DATA_HOME/PortMaster/" ]; then
  controlfolder="$XDG_DATA_HOME/PortMaster"
else
  controlfolder="/roms/ports/PortMaster"
fi
source $controlfolder/control.txt
[ -f "${controlfolder}/mod_${CFW_NAME}.txt" ] && source "${controlfolder}/mod_${CFW_NAME}.txt"
get_controls
GAMEDIR="/$directory/ports/digger"
export LD_LIBRARY_PATH="$GAMEDIR/libs.${DEVICE_ARCH}:$LD_LIBRARY_PATH"
export SDL_GAMECONTROLLERCONFIG="$sdl_controllerconfig"
cd $GAMEDIR
$ESUDO chmod +x "$GAMEDIR/digger.${DEVICE_ARCH}"
> "$GAMEDIR/log.txt" && exec > >(tee "$GAMEDIR/log.txt") 2>&1
$GPTOKEYB "digger.${DEVICE_ARCH}" -c "$GAMEDIR/digger.gptk" &
pm_platform_helper "$GAMEDIR/digger.${DEVICE_ARCH}"
./digger.${DEVICE_ARCH}
pm_finish
```
- **No command-line flags** are passed to the game (no `/F`, no speed, no `/C`).
- No resolution/aspect handling in the script (`DISPLAY_WIDTH` etc. unused); no `SDL_VIDEODRIVER` override.
- stdout/stderr go to `$GAMEDIR/log.txt`.

### B.4 Controls — `digger.gptk` (gptokeyb pad -> keyboard) [V]
| Pad | Key | Game meaning |
|---|---|---|
| D-pad, left stick, right stick | arrow keys | move |
| A, B, R1 | F1 | fire |
| X | Space | pause |
| Start | Enter | start game |
| Select (`back`) | F10 | back to title (and exit from title) |
| Y | `y` | (for Y/N prompts) |
| L1 `e`, L2 `a`, R2 `b`, L3 `c`, R3 `d` | letters | high-score initials entry, presumably [U] |

README control table matches (X pause; B/A/R1 fire; Select title; Start start) [V]. The game itself has no
gamepad code; all input is synthesised key events. Not mapped: F7/F9 (sound toggles), `+`/`-`, N (mode), K.

### B.5 Supported devices, config/save locations, quirks
- **Devices/resolutions [P device_info.json]:** `reqs: []` means no capability filter, so the port is
  offered on every aarch64 (and x86_64) device PortMaster knows: 40 devices / 93 device-firmware
  combinations. Screen sizes in that list: 640x480 (52 combos), 854x480 (9), **480x320 (7)**, 720x480 (6),
  1280x720 (4), 1920x1080 (3), 960x544, 720x720, 1024x768, 1280x960, 1280x800, 1920x1152 (2 each).
  Analog sticks: 2 (64 combos), 0 (19), 1 (10) — so D-pad-only play is mandatory. Primary arch aarch64 on
  91 of 93. Firmwares: ROCKNIX, ArkOS, AmberELEC, muOS, Knulli, EmuELEC, Batocera, TrimUI, RetroDECK.
- **glibc per firmware [P device_info.json]:** 2.30 (27 combos — mostly ArkOS: RG351/RG353, R36S, OGA…),
  2.33 (2), 2.38 (32), 2.40 (32). Empty `min_glibc` = the binary is expected to run on the oldest.
- **Config/saves:** via the `SDL_GetBasePath()` patch the rc and score files are written next to the binary
  in `ports/digger/` instead of `$HOME` (inferred from patch + upstream path code; file names on device not
  observed [U]). The script does not use the guide's `conf/` + `bind_directories` convention.
- Game draws a 640x400 texture stretched to the full output with no aspect correction [V source]; scaling
  quality on 480x320 is whatever the SDL renderer default gives [U on-device appearance].
- No flags, no fullscreen request; KMSDRM/firmware SDL presents fullscreen anyway [U].
- Same `.gptk` for all stick counts (guide suggests `name.gptk.$ANALOG_STICKS` variants when needed [P]).

### B.6 PortMaster packaging rules [P packaging guide]
- Required per port: `port.json`, `README.md`, `screenshot.{jpg,png}` (**exactly 640x480**, showing
  gameplay), `gameinfo.xml`, launch script `Port Name.sh`, port directory named like its parent, optional
  `cover`. Port name `[a-z0-9][a-z0-9._]*`, unique.
- `licenses/` subfolder with licences "for all sources, libraries and assets you used" (game licence,
  gptokeyb, any shipped `.so`).
- README must thank the original developer and include Controls and Compile sections.
- Script template: source `control.txt`, optional `mod_${CFW_NAME}.txt`, `get_controls`,
  `GAMEDIR=/$directory/ports/<name>`, `CONFDIR="$GAMEDIR/conf/"`, log to `log.txt`, redirect saves with
  `XDG_DATA_HOME`/`bind_directories`, `LD_LIBRARY_PATH="$GAMEDIR/libs.${DEVICE_ARCH}"`,
  `SDL_GAMECONTROLLERCONFIG="$sdl_controllerconfig"`, `$GPTOKEYB "exe.${DEVICE_ARCH}" -c file.gptk &`,
  `pm_platform_helper`, run `./exe.${DEVICE_ARCH}`, `pm_finish`. gptokeyb also provides the exit hotkey
  (Select+Start on most devices) and kills the process, even with no mapping file.
- Binary naming: `<exe>.aarch64` / `.armhf` / `.x86_64`; extra libs in `libs.<arch>/`.
- Build environment [P build-environments]: "We build on Ubuntu 22.04 LTS" (glibc 2.35); "Ubuntu 20.04
  (glibc 2.31) is the one to use if you want your port to also run on older CFWs like ArkOS"; `min_glibc`
  hides the port on older devices — leave empty if built on 20.04. Suggested aarch64 chroot (qemu-user) or
  Docker; chroot recipe replaces distro SDL2 with **SDL 2.26.2 built from source** "for better
  compatibility". **Caveat:** device_info lists ArkOS at glibc **2.30**, which is older than Ubuntu 20.04's
  2.31 — a Rust binary must be checked for `GLIBC_2.31` symbol versions (see C.5) rather than assuming.
- Native SDL-based C/C++ builds that "ship their own binary" are a standard category [P what-can-be-ported];
  nothing there excludes Rust. No resolution rule is stated in the packaging guide; supporting all listed
  screen sizes is implied by not setting `reqs`.
- Validation in repo README [V]: `python3 tools/build_release.py --do-check`; PR reviewed by the crew.

### B.7 Requirements this implies for the Rust port
1. Ship `digger.aarch64` (target `aarch64-unknown-linux-gnu`), dynamically linked against **system
   `libSDL2-2.0.so.0`** (use only SDL <= 2.26 API); require glibc <= 2.30 symbols to cover ArkOS with empty
   `min_glibc`, or set `min_glibc` and lose those devices [P]; optionally `x86_64`.
2. Work with zero arguments, fullscreen on KMSDRM, any resolution from 480x320 to 1920x1152, aspect
   ratios 4:3, 3:2, 16:9, 1:1 [P]; scale the 320x200 /
   640x400 framebuffer itself (integer or aspect-preserving) — an improvement over the C port's stretch.
3. Keep config, scores and recordings under the port folder: honour `XDG_DATA_HOME`/`XDG_CONFIG_HOME`
   (the guide's preferred mechanism [P]) with a fallback to the executable's directory — then no source
   patch is needed.
4. Either keep keyboard-only input and reuse `digger.gptk` unchanged, or (better) add native
   SDL GameController input honouring `SDL_GAMECONTROLLERCONFIG`, which the script already exports. Keep
   keyboard defaults identical so gptokeyb still works. Provide a pad-friendly initials entry and exit path.
5. Log to stdout/stderr; exit cleanly on F10 from title so `pm_finish` runs.
6. Include `licenses/` (GPL-2 `COPYING`, notices, SDL2 zlib licence if any `.so` is bundled), a 640x480
   gameplay `screenshot.png`, `gameinfo.xml`, README with Controls + Compile sections [P].
7. Must be playable with D-pad + face buttons only (19 device combos have no analog stick) [P].

---

## Part C — Rust toolchain and crates (as of 2026-11-11)

### C.1 Rust version
- Stable channel manifest: **`1.99.1 (58cf8f9c3 2026-10-13)`**, manifest date 2026-10-15 [V]
  https://static.rust-lang.org/dist/channel-rust-stable.toml . So **1.99 is released** (1.99.0 on the
  six-week cadence would be ~2026-10-01 [U, extrapolated from 1.91.0 = 2025-10-30]) and 1.99.1 is current.
- Beta manifest is already `1.101.0-beta.1 (2026-11-10)` [V]
  https://static.rust-lang.org/dist/channel-rust-beta.toml , which implies **1.100.0 stable is imminent
  (expected ~2026-11-12)** [U]. Pin `rust-toolchain.toml` to `1.99.1` to avoid drifting.
- This sandbox has `rustc 1.97.0 (2026-07-07)` installed [V] — older than the target; the pinned toolchain
  must be installed (static.rust-lang.org is reachable).
- Release notes for 1.92–1.99 were **not readable** (blog.rust-lang.org blocked). Do not rely on any
  1.9x-specific feature without checking locally.
- Language facts from memory [U, well established]: edition 2024 stabilised in 1.85 (Feb 2025);
  let-chains (`if let … && let …`) stable in 1.88 for edition 2024 only; `LazyLock`/`OnceLock` in std
  (1.80/1.70); `unsafe extern` blocks and `unsafe(no_mangle)` attributes required style in edition 2024;
  `static mut` references are denied-by-default lint in 2024 (relevant: the C code is all globals — port to
  an owned `Game` struct, not `static mut`); RPIT lifetime capture rules changed; `gen` reserved keyword.
  Rust 1.90 made LLD the default linker on x86_64 Linux; 1.91 promoted aarch64-pc-windows-msvc to tier 1
  [S: blog titles via search].

### C.2 Crate comparison (versions from crates.io API, fetched today) [V unless noted]

| Crate | Latest | Released | Declared MSRV | Notes |
|---|---|---|---|---|
| `sdl2` (rust-sdl2) | 0.38.0 | 2025-07-12 | none declared ("latest stable"); edition 2015 | Maintained at low intensity: commits through 2026-03-22, no release for 16 months. Features `bundled`, `static-link`, `use-pkgconfig`, `raw-window-handle`. README recommends SDL2 >= 2.0.26. https://crates.io/crates/sdl2 , https://github.com/Rust-SDL2/rust-sdl2 |
| `sdl3` (sdl3-rs) | 0.22.1 | 2026-11-09 | none declared; edition 2021 | Very active (3 releases in Nov 2026) but pre-1.0 and churning; README: "Expect some bugs and missing features". Built on `sdl3-sys` 0.7.1; features `build-from-source`, `build-from-source-static`, `static-link`, `use-pkg-config`. https://github.com/vhspace/sdl3-rs |
| `winit` | 0.30.13 stable (2026-03-02); 0.31.0-beta.4 (2026-10-24) | | 1.70 (0.30) / 1.86 (0.31 beta) | 0.31 still beta after a year; API changes pending. No KMSDRM backend [U]. |
| `pixels` | 0.17.2 | 2026-07-14 | 1.88 | wgpu-based; needs Vulkan/GLES3-class GPU + winit. Poor fit for Mali/PowerVR handhelds without a window system [U]. |
| `softbuffer` | 0.4.8 | 2025-12-13 | 1.71 | CPU presentation for winit; has a DRM/KMS backend but winit itself needs X11/Wayland [U]. |
| `minifb` | 0.29.0 | 2026-10-03 | none declared | Simple u32 framebuffer + keyboard; X11/Wayland/Win/macOS/wasm; no gamepad, no audio, no KMSDRM [U]. Previous release Jan 2025. |
| `macroquad` | 0.4.16 | 2026-07-30 | none declared | Own GL (miniquad) backend, good wasm story, built-in audio is sample-playback rather than a synthesis callback; Linux needs X11/Wayland + GL, no KMSDRM [U]. |
| `cpal` | 0.18.2 | 2026-08-16 | 1.85 | Callback audio, ALSA/WASAPI/CoreAudio/WebAudio. Good, but on handhelds ALSA device selection is less battle-tested than SDL's [U]. |
| `rodio` | 0.23.0 | 2026-10-11 | 1.87 | Mixer on top of cpal; more than needed for a PC-speaker-style synth. |
| `gilrs` | 0.11.2 | 2026-05-30 | 1.84 | Gamepads via evdev/XInput/IOKit/wasm; uses its own bundled SDL mapping DB and reads `SDL_GAMECONTROLLERCONFIG` [U]. |
| `cross` | 0.2.5 on crates.io (2023-02-04) | | — | Crates.io release is stale; project recommends `cargo install cross --git https://github.com/cross-rs/cross`. Its current aarch64 image uses glibc 2.39 — **too new for handheld firmware**; needs a custom image (e.g. Ubuntu 20.04) in `Cross.toml`. https://github.com/cross-rs/cross |
| `cargo-deny` | 0.20.2 | 2026-07-09 | 1.88 | Licence/advisory/ban checks. |

All MSRVs are <= 1.99, so the toolchain is not a constraint for any option.

### C.3 sdl2-compat
`sdl2-compat` implements the SDL2 ABI on top of SDL3 and "needs to be able to find a copy of SDL3" at
runtime; latest release 2.32.74 (2 Oct, year not shown — presumably 2026 [U]) [V]
https://github.com/libsdl-org/sdl2-compat , https://github.com/libsdl-org/sdl2-compat/releases .
Relevance:
- Desktop distros increasingly ship sdl2-compat as `libSDL2-2.0.so.0` [U — widely reported for Fedora/Arch/
  Debian/Ubuntu, not re-verified today]. A Rust binary linked against the SDL2 ABI runs on both real SDL2
  and sdl2-compat, so **targeting SDL2 is the most portable ABI choice** in 2026.
- PortMaster firmwares ship SDL2 (often vendor-patched for KMSDRM/rotation quirks [U]); the existing
  Digger port relies on it with no bundled SDL [V], and PortMaster's build recipe targets SDL 2.26.2 [P]. An SDL3 binary would have to bundle `libSDL3.so` and
  would lose those firmware patches — higher risk for no benefit to a 320x200 game.
- If linked dynamically on a build host that has sdl2-compat, avoid SDL2 APIs newer than the oldest target
  firmware's SDL2 (stay at the 2.26 feature level PortMaster builds against [P]; rust-sdl2 README
  recommends SDL2 >= 2.0.26 [V]).

### C.4 Recommendation
**Use the `sdl2` crate (0.38) as the single platform layer**, behind a small `Platform` trait so the game
core is a pure, dependency-free `no_std`-friendly crate (deterministic logic, indexed framebuffer `[u8;
320*200]`/640x400, palette, sound generator producing samples).

- Video: `Texture` streaming ARGB8888 (palette lookup on CPU), `Canvas::set_logical_size` or manual
  integer/aspect scaling; works on X11/Wayland/KMSDRM/Windows/macOS.
- Audio: `AudioCallback` (mono S16/f32, 44.1 kHz, small buffer) — same model as upstream `sdl_snd.c`, lowest
  porting risk for the timer-chip emulation in `newsnd.c`/`soundgen.c`.
- Input: SDL keyboard + `GameControllerSubsystem` (honours `SDL_GAMECONTROLLERCONFIG` exported by the
  PortMaster script) — no need for `gilrs`.
- Linking: dynamic + `use-pkgconfig` for Linux/handhelds; `bundled` + `static-link` for Windows/macOS
  release artefacts.
- wasm32 (optional): rust-sdl2 on `wasm32-unknown-emscripten` is possible but awkward [U]; cleaner is a
  second thin frontend for the same core (`macroquad`, or `winit`+`softbuffer`+`cpal`) added later. The
  core/frontend split keeps this cheap.
- Do **not** start on `sdl3` now: fast-moving API, and handhelds lack system SDL3. Revisit when sdl3-rs
  reaches a stable line; the `Platform` trait confines the migration.
- Rejected as primary: `pixels` (wgpu GPU requirements), `minifb`/`macroquad`/`winit` (no KMSDRM console
  path, the hard PortMaster requirement).

Risk to record: rust-sdl2 has had no release since July 2025 and is edition 2015; it is stable and widely
used (~800k recent downloads [V]) but if it stalls, the fallback is `sdl2-sys` directly or the `sdl3`
crate with a bundled SDL3.

### C.5 Tooling / CI
- `rust-toolchain.toml`: `channel = "1.99.1"`, components `rustfmt`, `clippy`; targets
  `aarch64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, optionally `wasm32-unknown-unknown`/`-emscripten`.
- `Cargo.toml`: `edition = "2024"`, `rust-version = "1.99"`, workspace `[lints]` table:
  `unsafe_code = "forbid"` in the core crate, `clippy::all` + selected `clippy::pedantic`; allow
  `cast_possible_truncation`/`cast_sign_loss` case by case (the C code leans on 16-bit wraparound — use
  explicit `wrapping_*` and `i16`/`u16` types so behaviour matches recordings).
- CI: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo deny check`
  (cargo-deny 0.20.2; set allowed licences compatible with GPL-2.0), replay tests that run recorded
  `.drf`/`.edrf` files headless (`SDL_VIDEODRIVER=dummy`, `SDL_AUDIODRIVER=dummy`) and compare score /
  state hash with the C build.
- aarch64 handheld build: an `arm64v8/ubuntu:20.04` chroot/Docker under QEMU with `libsdl2-dev`
  (PortMaster's documented method, glibc 2.31 [P]), or `cross` from git with a custom old-glibc image, or
  `cargo-zigbuild --target aarch64-unknown-linux-gnu.2.30` [U] to pin the floor to ArkOS's glibc 2.30
  [P device_info.json]. Always check: `objdump -T digger.aarch64 | grep -o 'GLIBC_[0-9.]*' | sort -Vu`.

---

## Still unverified
1. Rust 1.92-1.99 release notes and the exact 1.100 release date (blog.rust-lang.org blocked).
2. Which desktop distros ship sdl2-compat by default; exact SDL2 version on each handheld firmware.
3. Licence bodies of Sobolyev's `netsim*`/`edrf*` files and the Sippy files (only header lines scanned).
4. Whether a Rust std binary built on Ubuntu 20.04 (glibc 2.31) loads on ArkOS (glibc 2.30) — test with objdump.
5. Legal status of the CGA artwork, sound tables and "Popcorn" beyond what digger.org states (A.5).
6. On-device behaviour of the existing port (save file names, scaling look) — inferred from source only.
7. digger.org pages not read: `windmill.html`, `fame.html`, `future.html`, `java.html`, `links.html`, `instruct.html`.
