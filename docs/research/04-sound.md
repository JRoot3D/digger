# 04 — Sound subsystem (porting reference)

Source tree: `https://github.com/sobomax/digger` (HEAD 638125e). All `file:line` refs are relative to it.

Files read in full: `sound.c` (735), `sound.h`, `sound_int.h`, `sound_backend.c` (1157), `soundgen.c/.h`, `newsnd.c/.h`, `sdl_snd.c/.h`, `fbsd_snd.c/.h`, `spinlock.c/.h`, `hardware.h`, `device.h`, `sdl_timer.c`, `digger_math.c/.h` (filters), plus the sound-related parts of `main.c`, `digger.c`, `bags.c`, `bullet_obj.c`, `monster.c`, `input.c`, `scores.c`.

IMPORTANT: this fork differs substantially from Andrew Jenner's classic source. There is no `soundint` running off a timer ISR, no `sound level 0/1/2` PWM sample tables, no 8-bit path, and no circular sample buffer. What exists now:

- a game-thread **command queue** (`sound.c`),
- an audio-thread **effect/music state machine** that still thinks in PIT timer 0 / timer 2 divisors (`sound_backend.c`),
- a **PIT-to-band adapter** (`newsnd.c`),
- a 2-band **floating-point square-wave generator** (`soundgen.c`),
- an **SDL2 callback** backend with optional filtering (`sdl_snd.c`).

---

## 1. Architecture and data flow

```
GAME THREAD                                  AUDIO THREAD (SDL callback)
-----------                                  ---------------------------
bags.c / digger.c / monster.c / main.c
   soundfall(), music(), soundem() ...
        |  sound.c: sound_queue_post()
        v
  [sound_cmdq: 4096-entry ring, spinlock] --> fill_audio()            sdl_snd.c:92
                                               for each of len/2 samples:
                                                 getsample()          newsnd.c:59
                                                   every 606th sample: soundint()   sound_backend.c:183
                                                      sound_queue_drain(sound_backend_apply)
                                                      musicupdate() + 12 effect updaters
                                                         -> choose t0val / t2val / musvol
                                                      timer0()/timer2()/setspkrt2()  (fn pointers -> s1*)
                                                         -> sgen_setband / setband_mod / setmuteband
                                                   sgen_getsample()   soundgen.c:272
                                                 [HP 1 kHz -> LP 4 kHz, round, clamp]  (unless NO_SND_FILTER)
                                               SDL_MixAudioFormat into stream
  [sound_ackq: set of <=64 ids, spinlock] <-- sound_ack_push()  (level-done jingle end, dirge end)
   soundackready()/sound_ack_poll()
```

### 1.1 Layers

| Layer | File | Thread | Role |
|---|---|---|---|
| Public API | `sound.h`, `sound.c:304-546` | game | One function per event; each just enqueues a `struct sound_cmd {type, argi, argd, done_ack_id}` (`sound_int.h:10-44`). |
| Command queue | `sound.c:39-264` | both | Ring of 4096 (`SOUND_CMD_QUEUE_LEN`), drained in batches of 64 (`SOUND_CMD_DRAIN_MAX`) under a spinlock. |
| Ack set | `sound.c:57-187` | both | Unordered set of up to 64 completed ack ids plus a "canceled" set of 64; 16-bit ids, 0 = "no ack". |
| Backend state machine | `sound_backend.c` | audio (see 1.3 for exceptions) | All effect/music state, `soundint()` tick. |
| HW abstraction | fn pointers `setupsound, killsound, soundoff, setspkrt2, timer0, timer2, soundkillglob` (`sound.c:24-30`, `sound.h:56-62`) | — | Default `s0*` no-op stubs (`sdl_timer.c:96-128`); replaced with `s1*` from `newsnd.c` in `initsounddev()` (`main.c:331-347`) unless `/Q`. |
| PIT emulation | `newsnd.c` | audio | Maps timer divisors to generator bands. |
| Generator | `soundgen.c` | audio | 2 bands, square wave / amplitude modulator. Own spinlock. |
| Device | `sdl_snd.c` (`fbsd_snd.c` = FIXME stubs, `wave_device_available` = `false` macro) | audio | SDL2 callback. |

### 1.2 The emulated PC speaker model

Original hardware semantic retained in the backend:

- **Timer 2** = the speaker tone generator for sound effects. Backend variable `t2val` (`sound_backend.c:16`), divisor of `PIT_FREQ = 0x1234dd = 1193181 Hz` (`newsnd.c:14`). `t2val == 40` is the idle/"no effect" sentinel (inaudible ~29.8 kHz on real hardware).
- **Timer 0** = originally the system timer interrupt reprogrammed to produce music by PWM of the speaker. Backend variable `t0val`; `0x7d00` (32000) is the "no music note / rest" sentinel.
- `spkrmode` (`sound.c:18`): 0 = speaker driven by timer 2, 1 = timer 0 (music), 2 = off. `wavetype` (`sound_backend.c:15`) is the mode to use when timer 0 drives; initialised to 2, set to 1 forever by the first `musicupdate()` call that gets past the `musicplaying` check (`sound_backend.c:1024`).
- `pulsewidth = musvol * volume` (`sound_backend.c:1095`): original PWM duty; now turned into an amplitude.
- `soundt0flag` (`sound_backend.c:1101`) is an edge-trigger latch: `setsoundmode()` (1113-1121) sets `spkrmode=wavetype` and calls `setspkrt2()` only on the false->true edge; `setsoundt2()` (1103-1111) sets `spkrmode=0` and calls `setspkrt2()` only on the true->false edge.

Mapping to generator bands (`newsnd.c:101-161`), band 0 = `T0_BND`, band 1 = `T2_BND`:

- `s1timer0(t0)` (148-161): if `40 < t0 < 0x4000`: capture band 0 phase, `sgen_setband(0, PIT_FREQ / t0, (pulsewidth - 1) / 49.0)`, restore phase. Else disable band 0 (`freq=0, amp=0`). Always `t0rate = t0`.
  - **`PIT_FREQ / t0` is integer division** (`unsigned long / uint16_t`), then converted to `double`. All frequencies are whole Hz. Keep this truncation.
  - amplitude: `musvol` 1..50 with `volume==1` gives 0.0..1.0. `musvol==1` gives amp 0 which disables the band (`soundgen.c:141`).
- `s1timer2(t2, mode)` (104-124): if `40 < t2 < 0x4000`:
  - `mode == false`: `sgen_setband(1, PIT_FREQ / t2, 1.0)` — full-amplitude square wave (integer division again).
  - `mode == true` (level-done jingle only): `frq = (double)(PIT_FREQ / t2) - (double)(PIT_FREQ / t0rate)`; `sgen_setband_mod(1, frq, 1.0, 0.0)`; `sgen_setmuteband(1, 0)` and if the band *was* muted the phase to restore is forced to 0.0. Band 1 becomes an on/off **amplitude gate** on band 0 at the beat frequency (emulates the original two-timer "polyphonic" beating).
  - phase is captured before and restored after (`sgen_getphase`/`sgen_setphase`) for click-free frequency changes.
  - else: band 1 disabled.
  - Hazard: `PIT_FREQ / t0rate` divides by zero if `t0rate == 0` (it is 0 after `timer0(0)` in `initsound`). In practice mode==true is only reached after `sett0(true)` set `t0rate = t2val+35`. Guard it in Rust.
- `s1setspkrt2()` (133-146): `spkrmode==0` -> mute band 0, unmute band 1; `==1` -> unmute band 0, mute band 1; else mute both.
- `s1soundoff()` (126-131): mute both.

### 1.3 "Interrupt" tick

`getsample()` (`newsnd.c:59-65`): `if ((sgen_getstep(ssp) + 1) % intmod == 0) soundint();` then `sgen_getsample()`. `intmod = round(samprate / 72.8)` (`newsnd.c:72`) = **606 samples at 44100 Hz -> 72.772 Hz** (13.741 ms). First tick fires before sample index 605, then 1211, ... This replaces the original ~72.8 Hz timer-0 ISR. All effect durations are in units of this tick.

`soundint()` (`sound_backend.c:182-223`), in order:

1. `sound_queue_drain(sound_backend_apply)` — apply all pending commands.
2. `if (soundflag && !sndflag) sndflag = musicflag = true;` — turning sound back on also forces music on.
3. `if (!soundflag && sndflag) { sndflag=false; timer2(40,false); setsoundt2(); soundoff(); }`.
4. `if (sndflag && !soundpausedflag)`:
   - `t0val = 0x7d00; t2val = 40;`
   - `if (musicflag) musicupdate();`
   - effect updaters in this exact order (later ones overwrite `t2val`, i.e. **last writer wins = priority**): emerald, wobble, ddie, break, gold, em, explode, fire, eatm, fall, 1up, bonus (lines 200-211; compiled out by `NO_SND_EFFECTS`).
   - `if (t0val == 0x7d00 || t2val != 40) setsoundt2(); else { setsoundmode(); sett0(false); }` — any active effect pre-empts music; music only sounds when no effect wrote `t2val`.
   - `sett2val(t2val, false);` (always; `timer2` if `sndflag`).
5. `if (soundlevdoneactive) soundlevdoneupdate();`

`sett0(mode)` (1083-1099): if `sndflag`: when `!mode` call `timer2(t2val, false)`; clamp `t0val` to >= 1000 if `wavetype` is 1 or 2; clamp `musvol` to 1..50; `pulsewidth = musvol*volume; timer0(t0val); setsoundmode();`.

`initsound()` (1123-1141, game thread at startup, `main.c:362`): `timer2(40,false); setspkrt2(); timer0(0); wavetype=2; t0val=12000; musvol=8; t2val=40; soundt0flag=true; sndflag=true; spkrmode=0; setsoundt2(); soundstop_apply(); setupsound(); timer0(0x4000); randvs=0;`

### 1.4 Threads and locking

- **Game thread** runs: everything in `sound.c`; `initsound()`; `soundinitglob()`; `s1setupsound()` (which calls `soundint()` once directly, `newsnd.c:82`); `s1killsound()` -> `setsoundt2(); timer2(40,false); pausesounddevice(true)` (`newsnd.c:87-94`); `wakesounddevice()` (called inline from `sound_queue_push_done` when the command is `SOUND_CMD_WAKEUP`, `sound.c:225-226`).
- **Audio thread** runs: `fill_audio` -> `getsample` -> `soundint` -> all `*_apply`/`*update`, `sgen_*`.
- Locks: one spinlock for the command queue, one for the ack set (`sound.c:79-97`), one inside `sgen_state` taken on *every* `sgen_*` call including per-sample `sgen_getsample` and `sgen_getstep` (`soundgen.c:115,128,157,196,218,241,279`). `spinlock.c` is a bare `atomic_flag` test-and-set busy loop (79-87), no yield except `SDL_Delay(0)` on Emscripten.
- Atomics: `soundflag`, `musicflag` (`sound.c:19`), `sndflag` (`sound_backend.c:69`), `wave_device_available` (`sdl_snd.c:19`).
- **Unsynchronised state (data races in the C)**: `spkrmode`, `pulsewidth`, `volume`, and all backend statics are touched from the game thread during `initsound`, `s1setupsound` (direct `soundint()` call while the device is already unpaused — `setsounddevice` unpauses at `sdl_snd.c:87` before `initsound` runs) and `killsound`/`setupsound` in `scores.c:373,391`. Only the generator is protected. The Rust port should route these through the queue too.
- `sound_cmdq_enabled` (`sound.c:69`): false until a device is opened (`soundqueueenable()` from `sdl_snd.c:86`). While false, `sound_queue_push_done` drops the command and completes its ack immediately (`sound.c:195-200`). Queue full -> command dropped, ack completed (`sound.c:208-217`) — intended for `/S:0` test runs.

### 1.5 Ack protocol

- `sound_ack_alloc()` (99-108): `++next_id`, skipping 0. Game thread only.
- `sound_ack_push(id)` (125-139, audio thread): if id is in the canceled set, consume the cancel; else add to completed set.
- `sound_ack_poll(id)` (141-154): removes and returns true if completed.
- `sound_ack_cancel(id)` (156-171): if already completed remove it, else remember the cancel. `id==0` no-op.
- Public wrappers `soundackready(id)` (id 0 -> true), `soundackcancel(id)` (173-187).
- Removal is swap-with-last (`sound_ack_take`, 111-123).

Only two things are ack'd: the level-done jingle and the dirge (`TUNE_DIRGE`).

---

## 2. Sound effects

All state is `static` in `sound_backend.c`. "tick" = one `soundint()` (~13.74 ms). Frequency in Hz = `1193181 / t2val` (integer division). Every updater only writes `t2val`; the square wave is full amplitude on band 1. `FIREBALLS == DIGGERS` (`def.h:34`), i.e. 2.

| Effect | Game call sites | Start (`*_apply`) | Stop | Per-tick update |
|---|---|---|---|---|
| **Fall** (bag falling) | `bags.c:180,207` on; `bags.c:80,157` off | `soundfall_apply` 314-319: `soundfallvalue=1000; soundfallflag=true` | `soundfalloff_apply` 321-326: flag false, `soundfalln=0` | 328-347. State `soundfallflag, soundfallf, soundfallvalue, soundfalln`. If `soundfalln<1`: `soundfalln++`; if `soundfallf` `t2val=soundfallvalue`. Else: `soundfalln=0`; if `soundfallf` {`soundfallvalue+=50; soundfallf=false`} else `soundfallf=true`. Net: 4-tick cycle, tone sounds 1 tick in 4, divisor +50 per cycle (falling pitch from 1193 Hz). Note `soundfallf` is not reset by start/stop. |
| **Break** (bag breaks) | `bags.c:127` | 352-359: `soundbreakduration=3; if (soundbreakvalue<15000) soundbreakvalue=15000; flag=true` | `soundbreakoff` 361-365 | 367-378: while duration!=0: `duration--; t2val=soundbreakvalue` (15000 -> 79 Hz, 3 ticks); else flag=false. Value is effectively constant 15000 (initial 0). |
| **Wobble** (bag about to fall) | `bags.c:188` on, `bags.c:159` off | 383-387: flag=true | 389-394: flag false, `soundwobblen=0` | 396-416: `soundwobblen++` wrap `>63 -> 0`; `t2val` set only on ticks where n is 0 (`0x7d0`=2000, 596 Hz), 16 or 48 (`0x9c4`=2500, 477 Hz), 32 (`0xbb8`=3000, 397 Hz). 1-tick blips every 16 ticks. |
| **Fire** (fireball flying), per fireball `n` | `bullet_obj.c:45` on (called from `put`, i.e. every time it is (re)drawn — restarts the sweep), `bullet_obj.c:71` off | 422-427: `soundfirevalue[n]=500; soundfireflag[n]=true` | 429-434: flag false, `soundfiren[n]=0` | 436-465. For each n: `sff[n]=false`; if flag: if `soundfiren[n]==1` {`soundfiren[n]=0; soundfirevalue[n]+=soundfirevalue[n]/55; sff[n]=true; f=true; if (value>30000) off(n)`} else `soundfiren[n]++`. If any `f`: round-robin pick via `soundfirew` (do { n=soundfirew++; wrap at FIREBALLS } while !sff[n]) then `t2val = soundfirevalue[n] + randnos(soundfirevalue[n]>>3)`. Sounds every other tick, divisor grows ~1.8 %/2 ticks, plus up to 12.5 % random jitter (noise). |
| **Explode**, per fireball | `bullet_obj.c:57` | 471-478: `value[n]=1500; duration[n]=10; flag=true; soundfireoff_apply(n)` | `soundexplodeoff(n)` 480-484 | 486-513: if flag: if duration!=0 {`value -= value>>3; duration--; sef[n]=true; f=true`} else flag=false. If any f: round-robin via `soundexplodew`; `t2val=soundexplodevalue[n]`. 10 ticks, rising pitch (divisor x7/8 each tick: 1313, 1149, ...). |
| **Bonus** (pre-bonus-mode flash siren) | `digger.c:373,377` on, `digger.c:381,388,394` off | 518-522: flag=true | 524-529: flag false, n=0 | 531-543: `soundbonusn++` wrap `>15 -> 0`; n in 0..5 -> `t2val=0x4ce` (1230, 970 Hz); n in 8..13 -> `t2val=0x5e9` (1513, 788 Hz); else untouched. Two-tone 16-tick cycle. |
| **Em** (emerald click) | `digger.c:634` | 547-551: flag=true | 553-557 | 559-566: `t2val=1000` for exactly one tick, then off. (Overridden by later updaters; itself overrides emerald/wobble/ddie/break/gold for that tick.) |
| **Emerald** (rising scale) | `digger.c:635` `soundemerald(digdat[n].emn)`, `emn` 0..7 | 573-580: `emerfreq=emfreqs[n]; duration=7; soundemeraldn=0; flag=true`. Table `emfreqs[8]={0x8e8,0x7f0,0x712,0x6ac,0x5f2,0x54c,0x4b8,0x474}` (571) = 523,587,659,698,783,879,987,1046 Hz (C major scale C5..C6). **No bounds check on n.** | `soundemeraldoff` 582-586 | 588-604: if duration!=0: if `soundemeraldn` is 0 or 1 `t2val=emerfreq`; `soundemeraldn++`; if `>7` {n=0; duration--}. Else off. 7 bursts of 2-on/6-off ticks = 56 ticks. The consecutive-emerald counter `emn` and its `emocttime` timeout live in `digger.c:631-640` (game logic, not here). |
| **Gold** (collect gold) | `bags.c:457` | 609-617: `value1=500; value2=4000; duration=30; soundgoldf=false; flag=true` | `soundgoldoff` 619-623 | 625-644: if duration!=0 duration-- else flag=false (but still runs this tick); if `soundgoldf` {f=false; `t2val=value1`} else {f=true; `t2val=value2`}; `value1 += value1>>4; value2 -= value2>>4`. Two interleaved sweeps (one falling, one rising in pitch), 31 ticks. int16 — value1 reaches ~3.2k, no overflow. |
| **Eat monster** | `digger.c:686`, `monster.c:494` | 649-656: `duration=20; soundeatmn=3; value=2000; flag=true` | `soundeatmoff` 658-662 | 664-686: if `soundeatmn!=0`: if duration!=0 { if `duration%4==1` `t2val=value`; if `duration%4==3` `t2val=value-(value>>4)`; `duration--; value -= value>>4` } else {`duration=20; soundeatmn--; value=2000`}. Else flag=false. Three 20-tick rising chirps. |
| **Digger death "ddie"** | `digger.c:721,778`, `main.c:542` (netsim connect failure) | 691-697: `soundddien=0; value=20000; flag=true` | `soundddieoff` 699-703 (also called by `music_apply` for the dirge, 825) | 705-720: `soundddien++`; if `==1` `musicoff_apply()`; if 1..10 `value=20000-n*1000`; if `>10` `value+=500`; if `value>30000` off (**still writes t2val that tick**); `t2val=value`. Divisor falls 19000 -> 10000 over ticks 1..10 (pitch rising 62 -> 119 Hz), then +500 per tick until it exceeds 30000 at tick 51 (pitch falling). However `s1timer2` only accepts `40 < t2 < 0x4000` (`newsnd.c:108`) and **disables the band whenever `t2val >= 16384`**, so by my calculation ddie is audible only on ticks 4..22 (16000 down to 10000, back up to 16000) and silent, though still pre-empting music, on ticks 1..3 and 23..51. Nothing else is affected by that gate (break = 15000 is below it). Derived by reading, not by running; confirm with a trace. |
| **1-up** | `digger.c:1053` | 725-730: `duration=96; flag=true` | `sound1upoff` 732-736 | 738-748: if `(duration/3)%2 != 0` `t2val=(duration<<2)+600`; `duration--`; if `<1` flag=false. 3-on/3-off ticks, pitch rising as divisor drops 984 -> 612. |
| **Level done jingle** | `main.c:279` `soundlevdone()` | `soundlevdone_start_apply(ack)` 266-281: if `sndflag`: `nljpointer=0; nljnoteduration=20; soundlevdoneactive=true; soundlevdone_done_ack_id=ack; soundpausedflag=true` (pauses everything else). Else push ack immediately. | `soundlevdoneoff` 227-237: active=false, `soundpausedflag=false`, push ack | 286-309, runs *after* the normal block in `soundint`: `if (nljpointer<11) t2val=newlevjingle[nljpointer]; t0val=t2val+35; musvol=50; setsoundmode(); sett0(true); sett2val(t2val,true);` then `if (nljnoteduration>0) nljnoteduration-- else { nljnoteduration=20; nljpointer++; if (>10) soundlevdoneoff(); }`. Table `newlevjingle[11]={0x8e8,0x712,0x5f2,0x7f0,0x6ac,0x54c,0x712,0x5f2,0x4b8,0x474,0x474}` (283-284). 11 notes x 21 ticks = 231 ticks = 3.17 s. Band 0 = square at `PIT/(t2+35)` at amp 1.0, gated by band 1 at `PIT/t2 - PIT/(t2+35)` Hz (e.g. 523-515 = 8 Hz tremolo). |
| **Pause** | `main.c:832` on; `main.c:821,869` off (`testpause` calls `soundpauseoff()` every unpaused frame) | `soundpause_apply(true)` 1055-1067: idempotent; sets `soundpausedflag`; if `sndflag`: `timer2(40,false); setsoundt2(); soundoff();` | `soundpause_apply(false)`: just clears flag | All state machines freeze while paused (step 4 of `soundint` skipped); they resume where they were. |
| **Stop all** | `main.c:264,427,541`, `soundlevdone` | `soundstop_apply` 239-264: levdoneoff, flush music ack, fall/wobble off, fire off x2, `musicoff_apply`, bonus off, explode off x2, break/em/emerald/gold/eatm/ddie/1up off | — | Also clears game-side `cur_music_*` (`sound.c:304-309`). |

Level-done quirk (documented at `digger.txt:216-217`): `setspkrt2` with `spkrmode = wavetype`. If no music has ever played since start (`wavetype` still 2, e.g. `/M`), `s1setspkrt2` mutes band 0; then `s1timer2(..., true)` unmutes band 1 (a modulator) only -> **the jingle is silent** in this fork, where the original was "polyphonic". If music has played (`wavetype==1`), band 0 unmuted by `setspkrt2` (only on the `soundt0flag` rising edge) and band 1 unmuted by `s1timer2`. Decide deliberately whether to replicate.

Sound-only RNG: `randnos(n)` (`sound_backend.c:76-81`): `randvs = randvs*0x15a4e35 + 1; return (randvs & 0x7fffffff) % n;` on `int32_t randvs` (signed overflow UB in C; use `wrapping_mul`/`wrapping_add`). Seeded to 0 in `initsound`. **Separate from the game RNG** `dgstate.randv` (`main.c:1226-1230`, same LCG constants). Used only by fire. `randnos(0)` would divide by zero; cannot happen (min value 500>>3 = 62).

---

## 3. Music

### 3.1 Requests vs tunes

Game-side enum `music_request` (`sound.h:9-13`): `MUSIC_BONUS=0, MUSIC_MAIN=1, MUSIC_DIRGE=2`. `music_request_to_tune()` (`sound.c:272-289`) maps to backend `enum internal_tune` (`sound_backend.c:750-755`): BONUS->`TUNE_BONUS`(0), DIRGE->`TUNE_DIRGE`(2), MAIN->`TUNE_MAIN`(1) **unless the local player has exactly 1 life left, then `TUNE_BATTLE`(3)** (local player = `netsim_local_player()` in netsim, else `dgstate.curplayer`; reads `getlives()`).

So there are **four tunes**, not three:

| Tune | Table | Shape | Piece |
|---|---|---|---|
| `TUNE_BONUS` | `bonusjingle[321]` 851-872 | 160 (divisor,duration) pairs + `0x7d64` | William Tell Overture |
| `TUNE_MAIN` | `backgjingle[291]` 874-893 | 145 pairs + `0x7d64` | Popcorn |
| `TUNE_DIRGE` | `dirge[]` 895-899 (49 entries) | 24 pairs + `0x7d64` | Chopin Funeral March |
| `TUNE_BATTLE` | `battlejingle[]` 902-955 | long, + `0x7d64` | "I vnov prodolzhaetsya boi" (fork addition), played on last life |

Encoding: flat `int16_t` array of `(PIT divisor, duration units)` pairs. `0x7d00` as divisor = rest. `0x7d64` in the divisor slot = end marker -> loop to index 0. Battle notes are transposed up a major second at load: `battlejingle_notevalue()` (957-967): `>= 0x7d00` unchanged, else `((int32)v * 891 + 500) / 1000`.

Game call sites: `music(MUSIC_MAIN,1.0)` at round start `main.c:207`, after bonus `digger.c:389,395`, on respawn `digger.c:846`; `music(MUSIC_BONUS,1.0)` `digger.c:380,844`; dirge `digger.c:738` (`musicwithack(MUSIC_DIRGE,1.0)` single digger) / `digger.c:740` (`music(MUSIC_DIRGE,0.7)` two diggers, no ack); `musicoff()` `main.c:238`, `digger.c:774`; `musicrefresh()` `digger.c:1054` in `addlife` (re-issues MAIN if the lives-dependent mapping changed; `sound.c:508-515`, tracked via `cur_music_request/tune/dfac`, 292-302).

### 3.2 Per-tune parameters — `music_select_tune()` 763-805

| Tune | maxvol | attack | sustain | decay | release | `musicdfac` |
|---|---|---|---|---|---|---|
| BONUS | 50 | 20 | 20 | 10 | 4 | `3.0*dfac` |
| MAIN | 50 | 50 | 8 | 15 | 1 | `6.0*dfac` |
| DIRGE | 50 | 50 | 25 | 5 | 1 | `10.0*dfac` |
| BATTLE | 50 | 28 | 26 | 8 | 4 | `2.0*dfac` |

`musicdfac` is `int16_t` — the double product is truncated (dfac 1.0 -> 3/6/10/2; dirge at 0.7 -> 7, computed as `(int16_t)(10.0*0.7)`; check it yields 7 in Rust `as i16`, it does in IEEE double). DIRGE additionally: if `!musicflag` then `musicflag=true; restore_musicflag_after_death=true` (the dirge plays even with music off; flag restored in `musicoff_apply` 845-848 or when the dirge reaches its trailing rest, 1004-1007).

`music_apply(tune, dfac, ack)` 807-834: complete any previous music ack; if `!sndflag` complete this ack and return; `musicp=0; noteduration=0; music_select_tune(); musicplaying=true`; for DIRGE: `soundddieoff()`, and store the ack in `soundmusic_done_ack_id` (if `!wave_device_available` complete immediately). Note `musvol` is **not** reset on tune change, and `musicstage` only resets at the next note load (which is the next tick because `noteduration=0`).

`musicoff_apply()` 836-849: `musicplaying=false; musicp=0;` complete music ack; restore musicflag.

### 3.3 `musicupdate()` — 969-1053 (once per tick, only if `musicflag`)

```
if !musicplaying return
if noteduration != 0: noteduration--
else:                              // load next note
    musicstage = musicn = 0
    noteduration   = table[musicp+1] * musicdfac
    musicnotewidth = noteduration - musicdfac      // BONUS, DIRGE, BATTLE
                   = musicdfac * 2                 // MAIN  (fixed staccato gate)
    notevalue      = table[musicp]                 // (BATTLE: transposed)
    [DIRGE only: if musicp > 0 && notevalue == 0x7d00: push music ack; restore musicflag]
    musicp += 2
    if table[musicp] == 0x7d64: musicp = 0
musicn++
wavetype = 1
t0val = notevalue
if musicn >= musicnotewidth: musicstage = 2
switch musicstage:
  0 (attack):  if musvol+attack >= maxvol { stage=1; musvol=maxvol } else musvol += attack
  1 (decay):   if musvol-decay <= sustain { musvol=sustain } else musvol -= decay
  2 (release): if musvol-release <= 1 { musvol=1 } else musvol -= release
if musvol == 1: t0val = 0x7d00
```

Consequences:
- Each note occupies `duration*musicdfac + 1` ticks (one load tick + countdown). MAIN: a "2" note = 13 ticks = 179 ms; BONUS "2" = 7 ticks = 96 ms.
- Dirge: the 12 notes before the first trailing rest sum to 50 units -> 500 + 12 = 512 ticks; the ack fires on tick 513 = 7.05 s (matches the comment and `DIRGE_TICKS 89` at `digger.c:44-48`). The dirge then loops through 12x16-unit rests.
- A rest note (`0x7d00`) still runs the envelope; `t0val=0x7d00` causes `soundint` to pick `setsoundt2()` (silence unless an effect is active).
- Envelope volume -> amplitude `(musvol*volume - 1)/49.0`; `sett0` clamps `t0val >= 1000` (max 1193 Hz).
- When `musicflag` is false `musicupdate` isn't called at all: music position is frozen, not advanced silently.
- Music is only audible on ticks where no effect set `t2val` (see 1.3 step 4).

### 3.4 Toggles (backend side)

`togglesound_apply` 1069-1074: `soundflag = !soundflag` (effect applied by `soundint` steps 2-3). `togglemusic_apply` 1076-1081: `musicflag = !musicflag`. Re-enabling sound sets `musicflag=true` regardless.

---

## 4. Sample synthesis

### 4.1 Generator — `soundgen.c`

`sgen_ctor(srate, nbands=2)` (77-100); all bands start `disabled`. State: `step` (u64 sample counter), per band `b_type` (GEN/MOD), `freq`, `amp`, `phase`, `wrk.prd = 1/freq`, `wrk.lastspos` (u64 whole seconds already folded), `wrk.lut[2]` (i16), `wrk.phi_off`, `wrk.disabled`, `muted`; global `wrk.lastipos`, `wrk.lastnpos`.

- `sgen_setband(band, freq, amp)` 123-150: type GEN; if `freq>0 && amp>0`: `prd=1/freq; lut[0]=(i16)(amp*32767); lut[1]=(i16)(-amp*32767)`; else disabled. (`phase`, `phi_off`, `lastspos`, `muted` untouched.)
- `sgen_setband_mod(band, freq, a0, a1)` 213-233: type MOD; `amp=a1-a0`; if `freq>0`: `lut[0]=a0*32767; lut[1]=a1*32767`; else disabled. (When the beat frequency is 0 the gate is disabled -> ungated tone.)
- `sgen_setmuteband` 235-247 returns previous mute state.
- `sgen_getphase` 188-211: 0.0 if disabled; else `pos = precisediv(step - lastnpos, srate)`, `pos.ires += lastipos - band.lastspos`, `cpos = precisedivf(pos, prd)`, `fmod(band.phase + cpos.frem * band.freq, 1.0)`. Note: it does **not** add `phi_off` to the time, it adds `phase` after.
- `sgen_setphase(band, phase)` 166-186 -> `sgen_addphase(band, delta)` 152-164 with `delta = phase - r1` or `1 - r1 + phase`: `band.phase = fmod(band.phase + delta, 1.0); phi_off = band.phase / band.freq`. (Asserts `0 <= delta < 1`; with NDEBUG off there is also a `1e-15` consistency assert at 184 which can theoretically trip — decide whether to keep as `debug_assert`.) Division by `freq==0` gives inf/NaN if called on a disabled band; `newsnd.c` only calls it right after enabling, but `sgen_setband` with `amp<=0` (musvol==1) leaves the band disabled with `freq>0` so it is finite.
- `sgen_getsample` 271-329:

```
osample: i32 = 0; omod: i32 = 32767
pos = precisediv(step - lastnpos, srate)      // ires = whole secs, nres = ires*srate, frem = rem/srate (f64)
lastnpos += pos.nres; pos.ires += lastipos
for each band not disabled and not muted:
    tpos = pos; tpos.ires -= band.lastspos
    if phi_off != 0.0: tpos.frem += phi_off
    cpos = precisedivf(tpos, prd):
        cpos.ires = trunc(tpos.ires / prd)      // u64 from f64
        nres = cpos.ires * prd                  // f64
        cpos.nres = trunc(nres)                 // u64
        res = tpos.ires - nres
        cpos.frem = fmod(res + tpos.frem, prd)
    if cpos.nres > 0: band.lastspos += cpos.nres
    j = (cpos.frem * 2 < prd) ? 0 : 1
    GEN: osample += lut[j]
    MOD: omod = omod * lut[j] / 32767
osample /= nbands                               // always /2, even with one band active
if omod != 32767: osample = osample * omod / 32767
step += 1; lastipos = pos.ires
return (i16) osample
```

So: pure 50 % duty **square wave**, sample-and-hold at sample instants (no band-limiting, no PWM). Peak output is +/-16383 for a full-amplitude band (the `/nbands`). Since all frequencies are integer Hz (integer PIT division), every band's waveform is exactly periodic over 1 second. Floating point: `f64` throughout (`1/freq`, `fmod`, `trunc`). The output is entirely deterministic given the sequence of tick decisions, the sample rate, and IEEE-754 double semantics (beware FMA contraction in the reference C build when generating golden data: `phase + frem*freq` at line 207).

Built-in self-test harness `sgen_test()` (331-432, `-Dsgen_test`), not wired into any makefile.

### 4.2 Device layer — `sdl_snd.c`

- **SDL2** (`CMakeLists.txt:137`, `-D_SDL -D_SDL_SOUND` at 143). **Callback model** (`wanted.callback = fill_audio`), not queued audio.
- `setsounddevice(samprate, bufsize)` 37-90: `SDL_Init(SDL_INIT_AUDIO)`; `SDL_OpenAudioDevice(NULL, 0, &wanted, &obtained, 0)` with `freq=samprate` (default 44100), `samples=bufsize` (default 512 -> 11.6 ms), `channels=1`, `format=AUDIO_S16` (S16LSB), `allowed_changes=0` (SDL converts). Scratch buffer of `obtained.size` bytes. Filters: `bqd_lp_init(freq, 4000)`, `bqd_hp_init(freq, 1000)`. Then `soundqueueenable()` and unpause.
- `fill_audio` 92-125: sets `wave_device_available=true` on first call; memset silence; for `len/2` samples: `getsample()`; unless `NO_SND_FILTER`: `hp` then `lp`, `round()`, clamp to i16. Then `SDL_MixAudioFormat(..., SDL_MIX_MAXVOLUME)`.
  - **Build difference**: `GNUmakefile:17` defines `-DNO_SND_FILTER` (raw square waves); CMake and `Makefile.fbsd` do not (filtered). Pick one or make it a runtime option.
- Filters (`digger_math.c:163-211`), first-order despite the "bqd" name: `w = tan(pi*Fc/Fs); n = 1/(1+w); a1 = n*(w-1)`; LP: `b0=b1=n*w`; HP: `b0=n, b1=-n`. `bqd_apply`: `z1 = x*b0 + z0*b1 - z1*a1; z0 = x; return z1` (`z0` = previous input, `z1` = previous output). `D_PI` is a `long double` literal (`digger_math.h`), so coefficient computation is done in long double on x86 — tiny last-bit differences vs a pure-f64 Rust port are possible in filtered output.
- `pausesounddevice(p)` 130-137; `wakesounddevice()` 139-154: if the device is not `SDL_AUDIO_PLAYING`, clears `wave_device_available` and unpauses.
- `initsounddevice()` returns true, no-op.
- No close/teardown: `soundkillglob` stays the `s0` no-op; `sgen_dtor` is never called.

### 4.3 Volume / bit depth

- 16-bit signed mono only. No 8-bit path in this fork.
- `volume` (`sound.c:18`): `calibrate()` sets `volume = getkips()/291`, min 1 (`main.c:872-877`); `getkips()` returns 1 on SDL (`sdl_timer.c:91-94`) -> **volume is always 1**; `initsounddev` also sets `volume=1`. Its only audio role is `pulsewidth = musvol*volume`. It is also (ab)used as a loop count in `flashywait` (`scores.c:403-410`) — graphics agent's concern.
- INI `SoundVolume` is read into `dx_sound_volume` (`main.c:1288`) and **never used**. There is no master volume.

---

## 5. Timing, determinism, feedback into gameplay

- **The sound tick does not drive the game clock.** Frame pacing is `gethrt()` in `sdl_timer.c:54-89`: a software PLL on `SDL_GetTicks()` (phase detector + recursive filter + sigmoid, from `digger_math.c`) sleeping to `dgstate.ftime` microseconds per frame (default 80000 = 12.5 Hz, `def.h:56`). `inittimer()` (called from `setupsound`) initialises that PLL — the only reason "sound setup" touches timing (`sound.c:731-735`, `newsnd.c:81`). Speed changes do not change sound tempo (`digger.txt:270`).
- **The audio clock is the sample counter**; game events are applied at the next 606-sample boundary after they are dequeued. Latency = buffer (512) + up to one tick. Not sample-accurate relative to game frames, and output is not reproducible across runs with a real device because of callback/game thread interleaving.
- **RNG**: sound uses its own `randvs`; it never touches `dgstate.randv`. No sound state is read by game logic except via acks and `sound_backend_local_sound_available()`.
- **Feedback paths into game flow (wall-clock only, not game state):**
  1. `soundlevdone()` (`sound.c:361-381`): after `soundstop()`, if `sndflag && wave_device_available`, posts LEVDONE_START with an ack and **blocks the game thread** in `soundlevdone_wait_finish` (317-327) looping `gethrt(false,1); checkkeyb();` until ack, `escape`, or device loss. On abort posts LEVDONE_OFF and cancels the ack. No frames are counted and no RNG is consumed during the wait. With no device it returns immediately. NetSim variant (329-359) exchanges `freezeframe()`s with the peer while waiting (netsim agent's area) — there frames *are* exchanged.
  2. Death dirge (`digger.c:720-800`): game logic waits a **fixed** `DIRGE_TICKS = 89` game ticks (`dirgetime`) regardless of sound; `musicwithack()` returns 0 with no device. Then `dirgehold()` (`digger.c:244-252`) additionally stalls in real time until the ack; `deathffwd` skips `gethrt` when the dirge is already done. During playback (`playing`) the ack is cancelled and never waited for (`digger.c:220-225`). So recorded-game determinism is unaffected; only pacing differs.
  3. `music_request_to_tune` reads lives (game -> sound, one way).
- With `/Q` or a failed device open: queue disabled, every command dropped, acks immediate, `soundint` never runs. Game logic identical.
- Tests run with `SDL_AUDIODRIVER=dummy` (`scripts/do-test-run.sh:24`); there are **no sound unit tests** upstream.

---

## 6. Options and settings

- Command line (`main.c:1138-1141, 1166-1167`): `/Q` sets `soundflag=false` **and** `quiet=true` -> `initsounddev()` (`main.c:331-347`) does not open a device or install `s1*` functions at all, so F9 cannot bring sound back (contradicts `digger.txt:441-442`, which describes the classic behaviour). `/M` sets `musicflag=false` (toggleable with F7; the dirge still plays).
- Keys (`sdl_kbd.c:36-37`, `input.h:50-51`, `input.c:129-134`): `DKEY_MTG` = F7 -> `togglemusic()`; `DKEY_STG` = F9 -> `togglesound()`. Rebindable via the `[Keys]` INI section. Toggles are queued commands, so they do nothing without a device. Toggles are not written back to the INI.
- INI section `[Sound]` (`def.h:63`, `main.c:1283-1288`): `SoundOn` (bool, default true) -> `soundflag`; `MusicOn` (true) -> `musicflag`; `Rate` (44100) -> `sound_rate`; `BufferSize` (`DEFAULT_BUFFER` = 512, `def.h:66-69`, comment: smaller breaks ALSA) -> `sound_length`; `SoundVolume` (0) -> unused. `Rate`/`BufferSize` are stored in `uint16_t` (`main.c:323`) — values above 65535 wrap (48000 is fine, 96000 is not).
- Init order: `inir()` reads INI and calls `soundpreinit()` (creates queue locks; `sound.c:548-554`) -> `parsecmd` -> `maininit()`: `initsounddev()`, `calibrate()`, ..., `initsound()` (`main.c:349-368`). `soundwakeup()` before each `game()` (`main.c:554`) re-starts a device paused by `killsound()` in `getinitials` (`scores.c:373,391`; note `s1setupsound` does not unpause). Shutdown `finish()` (`main.c:607-614`): `killsound(); soundoff(); soundkillglob();`.
- Compile-time: `_SDL_SOUND` (whole subsystem; without it `sound.c:555-722` provides empty stubs), `NO_SND_FILTER`, `NO_SND_EFFECTS` (music only), `sgen_test`, `newsnd_test`, `spinlock_test`.

---

## 7. Suggested Rust design

### 7.1 Module layout

```
sound/
  cmd.rs      // enum SoundCmd { Stop, Wakeup, LevDoneStart{ack}, LevDoneOff, FallOn, FallOff, Break,
              //   WobbleOn, WobbleOff, FireOn(u8), FireOff(u8), Explode(u8), BonusOn, BonusOff, Em,
              //   Emerald(u8), Gold, EatM, DDie, OneUp, Music{tune, dfac: f64, ack}, MusicOff,
              //   SoundToggle, MusicToggle, Pause(bool) }
  tables.rs   // BONUS_JINGLE, BACKG_JINGLE, DIRGE, BATTLE_JINGLE, NEWLEV_JINGLE, EM_FREQS as &[i16]
  synth.rs    // struct Synth: effect + music state machine; fn tick(&mut self, pit: &mut Pit, acks)
  pit.rs      // struct Pit { gen: SquareGen, t0rate: u16, spkrmode, pulsewidth, .. }  (newsnd.c)
  gen.rs      // struct SquareGen (soundgen.c), no lock
  filter.rs   // FirstOrder { a1, b0, b1, z0, z1 }
  engine.rs   // struct Engine { synth, pit, filters, intmod, rx } ; fn render(&mut self, out: &mut [i16])
  handle.rs   // game-side SoundHandle: one method per event, ack alloc/poll/cancel, cur_music tracking
  backend.rs  // trait AudioBackend + SdlBackend, NullBackend
```

- **Pure core**: `Engine::render(&mut [i16])` owns everything the audio thread mutates and has no I/O, clock or globals. It reproduces `getsample()`: `if (step+1) % intmod == 0 { drain commands; synth.tick() }`. Replace the function-pointer indirection (`timer0`/`timer2`/...) with direct calls on `Pit`; the `s0` no-op variant becomes "no engine".
- **Thread safety**: the engine is moved into the audio callback (`Send`, no `Sync` needed). Game -> audio: a bounded SPSC ring (`rtrb`, `ringbuf`, or `crossbeam::ArrayQueue<SoundCmd>` capacity 4096) with drop-on-full + immediate ack, as the C does. Audio -> game acks: second small SPSC queue of `u16` ids; the game-side handle keeps the "completed" and "canceled" sets locally (no lock needed since alloc/poll/cancel are all game-thread). `sound_on`/`music_on`/`device_available` as `AtomicBool` for the game side to read. No spinlocks, no per-sample locking. A `Mutex<Engine>` would also work but the lock-free form matches the real-time constraint and removes the C's data races (do `initsound`, `killsound`/`setupsound` via commands, or before the stream starts).
- **Backend trait**: `trait AudioBackend { fn start(&mut self, engine: Engine, rate: u32, buf: u16) -> Result<..>; fn pause(&mut self, bool); fn wake(&mut self); fn available(&self) -> bool; }`. SDL2 `AudioCallback` (or `cpal`) impl calls `engine.render`. **NullBackend**: either drops commands and completes acks immediately (mirrors `/Q` / `sound_cmdq_enabled=false`) or, for tests, a `ManualBackend` that owns the engine and lets the test call `render(n)` synchronously.
- Keep `volume` as a constant 1 inside `Pit`; expose a real master volume separately if wanted (post-synthesis gain), since `SoundVolume` is dead upstream.

### 7.2 Testing

- **State-machine tests without samples**: have `Synth::tick` return/record `(t0val, t2val, musvol, spkrmode, mode)` per tick; assert sequences for each effect against values traced from the C (add a printf at the end of `soundint`, build with `SDL_AUDIODRIVER=dummy`), e.g. gold = 31 ticks alternating 4000,500,3750,531,...; dirge ack at tick 513; emerald = 7x(2 on, 6 off).
- **Golden sample hashes**: drive `Engine::render` with a scripted command list at fixed sample offsets (e.g. `Music{Main}` at 0, `Gold` at 22050), render N seconds at 44100 with filter off, hash the i16 LE bytes (SHA-256/xxhash). Generate reference data from the C by linking `sound_backend.c + sound.c + newsnd.c + soundgen.c + spinlock.c` with a tiny driver that implements `setsounddevice`/`wakesounddevice`/`gethrt` stubs and calls `getsample()` in a loop (compile with `-ffp-contract=off`). Keep filtered-output goldens separate and compare with a tolerance (or hash only after confirming bit-equality on your platforms) because of the `long double` coefficient maths.
- Generator property tests: for integer `f` and no phase offset, sample `n` is high iff `fmod((n % srate)/srate, 1/f)*2 < 1/f`; transitions/second == 2f; phase continuity across `setband` (no extra edge).
- Run game-logic determinism tests with the null backend to prove sound never perturbs gameplay.

### 7.3 Trickiest parts to port

1. **`soundgen.c` float maths bit-exactness** — `precisediv`/`precisedivf`, `trunc` to u64, `fmod`, the `lastspos`/`lastipos`/`lastnpos` folding, and `phi_off = phase/freq`. Port literally with `f64`, `%` for `fmod`, `.trunc() as u64`; do not "simplify" to a phase accumulator if golden hashes matter. Integer `PIT_FREQ / t` truncation must be kept.
2. **Edge-triggered mode switching** (`soundt0flag`, `setsoundt2`/`setsoundmode`, `wavetype` sticky at 1, mute flags persisting across `setband`) — easy to get subtly wrong; it determines the silent-level-done quirk and music/effect arbitration.
3. **Updater ordering and "write on some ticks only" semantics** — priority is the call order in `soundint`; several effects write `t2val` on the tick they turn themselves off (gold, ddie).
4. **`s1timer2` gate `40 < t2 < 0x4000`** silently mutes large divisors (most of ddie); `s1timer0` same gate plus the `>= 1000` clamp.
5. **int16 arithmetic and truncations**: `musicdfac` (f64 -> i16), `noteduration = dur * musicdfac`, `soundfirevalue += v/55`, shifts on signed values; `randvs` wrapping i32 LCG and `& 0x7fffffff` before `%`.
6. **Ack lifecycle**: music ack completion points (new `music_apply`, `musicoff_apply`, `soundstop_apply`, dirge reaching the rest at `musicp>0`, no device), cancel-before-complete, id wrap skipping 0; and the game-side blocking waits (`soundlevdone`, `dirgehold`) which must stay wall-clock-only and be bypassed during playback / null backend.
7. **`restore_musicflag_after_death`** and the `soundflag`-on-forces-`musicflag`-on rule, with `musicflag` mutated from the audio side.
8. **Device pause/wake dance** (`killsound` in `getinitials` pauses; only `soundwakeup` before the next game resumes; `wave_device_available` cleared on wake until the first callback) — affects whether `soundlevdone` waits.
9. `emfreqs[n]` and fire/explode indices are unchecked in C; Rust will panic on out-of-range — clamp or assert at the handle.
