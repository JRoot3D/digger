# Compatibility quirks

This file lists every place where the port deliberately does what the
original Digger Remastered (sobomax/digger at commit 638125e) does instead of
what a clean implementation would do, or gives defined behaviour where the
original had none. Each entry names the code that isolates it (a comment
carrying the quirk id sits at that spot) and the test that pins it.

## Policy

Summary of the quirk policy in [`docs/porting-plan.md`](porting-plan.md)
§3. Every deviation belongs to one of four classes:

| Class | Policy |
|---|---|
| Affects the simulation | Reproduce exactly, named in code, covered by a test |
| Undefined behaviour in C | Give it defined, safe behaviour; match what recordings observe where they exercise it; reject invalid mode combinations at configuration time |
| Presentation or usability bug | Fix |
| Dropped feature (CGA, NetSim, X11 embed, pre-1998 recording kludge) | Not ported; rejected with a clear message where a user could ask for it |

The picture is game state (the fireball reads pixels back to find walls),
so anything that changes which pixels are drawn counts as affecting the
simulation even when it looks like presentation.

When a recording disagrees with a research report or with an entry here,
the recording wins and the entry is corrected. Recordings win over reports:
the research reports in `docs/research/` were written by reading the
original's code, so every entry derived from them stands until an upstream
recording says otherwise, and each correction is listed under "Findings from
recordings".

Id prefixes, so that entries from different changes never collide: `R`
rendering, `S` sprite layer, `A` asset data (all from
`add-assets-and-renderer`); `Q` simulation and `U` undefined or accidental
behaviour of the original (`add-simulation-core`); `P` recording and
playback; `SQ` and `SF` sound.

Test names below are given with their module path inside the `digger-core`
crate; run one with `cargo test -p digger-core <name>`.

## Rendering (`add-assets-and-renderer`)

| Id | Quirk | Class | Isolated in | Test |
|---|---|---|---|---|
| R1 | The pixel probe reads an unaligned 8x2 block of real pixels at real (2x, 2y), ORs the two rows, and masks the result with 0xEE; beyond logical x 319 or y 199 it returns an unmasked 0xFF (research 03 §4.6) | Affects the simulation | `Framebuffer::probe` (`crates/digger-core/src/video/framebuffer.rs`) | `video::tests::probe_vectors` |
| R2 | A probe near the right edge runs into the next real row, because the picture is addressed linearly; reads before the start or past the end of the picture are 0, where upstream reads out of bounds. Research 03 §4.6 argues the game never probes there; this result is a choice, to be corrected if a recording shows otherwise | Undefined behaviour in C, given a defined result | `Framebuffer::pixel_at_offset` (`video/framebuffer.rs`) | `video::tests::probe_row_wrap`, `video::tests::probe_outside_buffer_reads_zero` |
| R3 | Selecting a palette also resets the intensity to normal (research 03 §3) | Affects the picture | `Framebuffer::set_palette` (`video/framebuffer.rs`) | `video::tests::palette_select_resets_intensity` |
| R4 | The masked blit treats an image byte of 255 as transparent in addition to the mask: such a pixel is left unchanged, any other sets `(pixel AND mask) OR image` (research 03 §4.5, §13 item 9) | Affects the simulation (probe input) | `Framebuffer::blit_parts` (`video/framebuffer.rs`) | `video::tests::blit_rule_truth_table` |
| R5 | Text is opaque, a full 12x12 logical cell including its black background; inside a string a character without a glyph erases its cell like a space; a lone character without a glyph draws nothing (research 03 §6) | Affects the simulation (probe input) | `glyph_or_space` (used by `Framebuffer::draw_text`) and `Framebuffer::draw_char` (`video/text.rs`) | `video::text::tests::text_cell_is_opaque`, `video::text::tests::text_invalid_char_becomes_space` |
| R6 | Number drawing leaves the leftmost cell untouched, not erased, when its digit is zero (research 03 §6) | Affects the simulation (probe input) | `Framebuffer::draw_number` (`video/text.rs`) | `video::text::tests::number_leftmost_cell_not_erased` |
| R7 | Background tiling starts at logical y 14 and its last row of tiles is clipped at y 199; rows 12–13 are never drawn by it (research 03 §8.2) | Affects the simulation (probe input) | `Framebuffer::draw_background` (`video/backdrop.rs`) | `video::backdrop::tests::background_coverage` |

## Sprite layer (`add-assets-and-renderer`)

| Id | Quirk | Class | Isolated in | Test |
|---|---|---|---|---|
| S1 | x is rounded down to a multiple of 4 (towards minus infinity) by place, move and static draw, but not for the protected area of a static draw, for text or for the probe (research 03 §2, §7.5, §13 item 8) | Affects the simulation | `geom::align_x` (`crates/digger-core/src/geom.rs`) and its call sites in `SpriteLayer::place`, `move_to`, `draw_static` | `sprite::tests::alignment_sites` |
| S2 | Collision lists are produced only by a move and persist until the next move (or until game logic empties them) (research 03 §7.4, §13 item 3) | Affects the simulation | `SpriteLayer::move_to` (`sprite/mod.rs`) | `sprite::tests::collisions_only_on_move` |
| S3 | A move restores the sprite's saved background at its recorded position even when the sprite is disabled; saved backgrounds survive erase and reset, so a move after an erase pastes a stale background (the blinking digger) (research 03 §7.4, §7.6) | Affects the simulation | `SpriteLayer::move_to` (`sprite/mod.rs`) | `sprite::tests::move_after_erase_restores_stale_background` |
| S4 | Placing an already enabled sprite does not erase its old image and, when it overlaps another sprite at the new position, writes its old saved background there before the new capture (research 03 §7.4) | Affects the simulation | `SpriteLayer::place` (`sprite/mod.rs`) | `sprite::tests::place_enabled_sprite_quirk` |
| S5 | Other sprites collide with a sprite's current (last drawn) image size, for example 16x14 for image 80, not with its pending image (research 03 §7.2, §7.4) | Affects the simulation | `Sprite::rect` (`sprite/mod.rs`) | `sprite::tests::collision_uses_current_size` |
| S6 | Resetting an enabled sprite forgets it without erasing it: its image stays on the picture (research 03 §7.3, §13 item 15) | Affects the simulation | `SpriteLayer::reset` (`sprite/mod.rs`) | `sprite::tests::reset_does_not_erase` |
| S7 | Stacking order is slot order (higher slot on top, whichever sprite moved last); sprites that game logic regards as dead but are still enabled collide and are redrawn (research 03 §7.3; research 02 §1) | Affects the simulation | ascending iteration of the slot bit sets (`sprite/id.rs`, `sprite/collisions.rs`) | `sprite::tests::stacking_order`, `sprite::tests::enabled_not_alive` |

## Asset data (`add-assets-and-renderer`)

| Id | Quirk | Class | Isolated in | Test |
|---|---|---|---|---|
| A1 | Image numbers 88–93 duplicate 82–87 and 57–61 duplicate 26–30; the duplicates are kept so that the +31 (second digger) and +32 (second fireball) offsets hold for every frame (research 03 §5.2) | Data layout | the generated index `assets/index.rs` and `ImageId::for_second_digger`, `ImageId::for_second_fireball` (`crates/digger-core/src/assets/image.rs`) | `assets::image::tests::duplicate_image_numbers` |

## Simulation quirks (`add-simulation-core`)

Class: affects the simulation. Each quirk is reproduced exactly, isolated
in one named place carrying a comment with its id, and pinned by one test
named `quirk_<id>` (design D12 of `add-simulation-core`). The last two
columns are filled in as the quirks are implemented.

| Id | Behaviour | Research reference | Implemented in | Test |
|---|---|---|---|---|
| Q01 | Penalty counter: every draw and scoring site of the scoring spec's penalty table raises it, a scoring call three times even for 0 points | research 01 §4, research 02 §6 |  |  |
| Q02 | Penalty overflow delays monster slots 1..n−1 only; slot 0 is exempt, an excess of 1 does nothing, unoccupied slots are included | research 01 §4, research 02 §6, §9 item 1 |  |  |
| Q03 | Random draws are taken even when their result is unused: the noise draw at difficulty 6 and above, the double-move draw for hobbins | research 02 §4.3, §4.4, §8 |  |  |
| Q04 | Wind-down updates bags, then diggers, then monsters; monsters and the generator keep running; it lasts 1 to 20 ticks by the moving-bag and penalty rule | research 01 §3.2, research 02 §6 |  |  |
| Q05 | A full redraw opens every partly dug cell completely | research 01 §5.3, §10 item 8, research 03 §8.2 |  |  |
| Q06 | The second player's level state is initialised as level 0 in modes without alternating players | research 01 §3.1, §10 item 18 |  |  |
| Q07 | With two diggers on the built-in levels, layouts 3 and 4 are read with `H` at cells (6,9) and (8,9) | research 01 §5.5, §10 item 7 |  |  |
| Q08 | Level sequence 1–8, then 6–8, then 5–8 repeating; level capped at 1000; difficulty frozen at 10 | research 01 §5.5, §10 item 4, research 02 §7 |  |  |
| Q09 | The second score's bonus life arrives one point late | research 01 §6.2, §10 item 1 |  |  |
| Q10 | Lives are capped at 5 while the bonus threshold still advances; one threshold step per scoring call | research 01 §6.2 |  |  |
| Q11 | At one million the visible score rolls over into a carried total, and no bonus lives are awarded afterwards | research 01 §6.2, §10 item 3 |  |  |
| Q12 | Score awards are truncated to signed 16 bits; the monster-eating multiplier doubles with 16-bit wrap | research 01 §6.2, §9, research 02 §3.8 |  |  |
| Q13 | The gauntlet clock counts in 1,193,181ths of a second, −80,000 per tick with wrap; bonus time adds 17,897,715 with wrap | research 01 §7.4 |  |  |
| Q14 | A level advances when no emeralds remain even if the digger died on that tick; the dead digger still loses a life | research 01 §3.2, §10 item 11 |  |  |
| Q15 | Movement samples are taken only while a digger is alive, visible and not in a push pause; fire is latched one tick late | research 02 §3.4, research 05 §1.6, §1.7, §4.3 |  |  |
| Q16 | Emerald thresholds per direction: redraw at remainder 8/12/16/6, eat at 12/9/12/9 (right/up/left/down) | research 02 §3.5 |  |  |
| Q17 | A digger pauses after every horizontal push attempt, successful or not | research 02 §3.4 |  |  |
| Q18 | A blocked digger is redrawn (an extra invincibility decrement) and its facing reversed | research 02 §3.4, §3.7 |  |  |
| Q19 | Fireballs find earth by the pixel probe with per-direction sample patterns; the probe is ignored while touching any sprite, the own digger included | research 02 §3.6, research 03 §4.6 |  |  |
| Q20 | Kill rule: a digger can be killed again while carried by a bag or in the death arc, not during the grave stages | research 02 §3.4, §3.7, §9 items 13, 15 |  |  |
| Q21 | A crushed digger is held at bag top + 6; a squashed monster snaps to the bag top without the 6 | research 02 §3.7, §4.7, §5.7, §9 items 6, 7 |  |  |
| Q22 | Monsters on a fresh grave are removed without score | research 02 §3.7 |  |  |
| Q23 | A one-digger death waits 89 ticks from the first tombstone frame | research 02 §3.7, §9 items 14, 23 |  |  |
| Q24 | "All diggers dead" is tested over absolute digger indices; the eat-multiplier reset and the respawn facing likewise | research 02 §3.7, §3.10 |  |  |
| Q25 | Respawn does not restore the fire-ready look | research 02 §3.7, §3.10 |  |  |
| Q26 | Outside bonus mode only a monster's own move kills a digger; in bonus mode either side's move eats the monster | research 02 §3.4, §3.8, §4.4 |  |  |
| Q27 | A monster's side effects (decision, draw, digging, collisions) persist when its move is undone | research 02 §4.4 |  |  |
| Q28 | With no passable priority a monster takes the reverse direction; a hobbin sticks at the edge; an overlapped monster is reversed between cells | research 02 §4.4, §4.6 |  |  |
| Q29 | The transformation test also runs for dead monsters still carried by a bag | research 02 §4.3, §4.5, §9 item 24 |  |  |
| Q30 | A dead left-facing hobbin is 14 pixels tall | research 02 §4.7, §9 |  |  |
| Q31 | Any monster removal in bonus mode raises the round's monster total | research 02 §3.8, §4.7 |  |  |
| Q32 | Bags share one push counter; a failed push sets the bag's direction to the reverse; a failed chain restores twice | research 02 §5.3, §5.5, §9 item 9 |  |  |
| Q33 | Pushing a falling bag sideways succeeds without moving it and crushes the pusher | research 02 §5.5, §5.7, §9 item 10 |  |  |
| Q34 | A landing bag removes the bags it overlaps; gold over a dug cell expires early; a bag breaks after falling two cells | research 02 §5.2, §5.4, §9 item 11 |  |  |
| Q35 | A digger takes gold whenever it overlaps it, whoever triggered the collection | research 02 §5.6, §9 item 12 |  |  |
| Q36 | A bag's wobble is suppressed by a vertically moving digger in the bag's row or the row below | research 02 §5.3 |  |  |
| Q37 | Round-end bag tidy rules, including removal of a bag at cell (7,9) | research 02 §5.1, §5.8 |  |  |
| Q38 | A hobbin's dig step is charged even with no direction; hobbins and falling bags destroy emeralds without score | research 02 §4.4, §5.3 |  |  |
| Q39 | Zero-point scoring calls at round start, after a pause and at game end perform the threshold check | research 01 §3.2, §3.3, §6.2, §6.3 |  |  |

## Undefined and accidental behaviour (`add-simulation-core`)

Cases where the original's behaviour is undefined in C or plainly
accidental, and the decision taken for each (design D13 of
`add-simulation-core`).

| Id | Behaviour | Research reference | Decision | Class | Implemented in | Test |
|---|---|---|---|---|---|---|
| U01 | Firing with no facing aborts the program | research 02 §3.4, §9 item 18 | The fire request is ignored and the fireball stays ready (digger spec, "Digger left without a facing") | Undefined behaviour in C, given a defined result |  |  |
| U02 | The pixel probe beyond the screen reads outside the picture | research 02 §3.6, research 03 §4.6 | The phase-2 probe's defined result is used as is (quirks R1, R2): earth beyond x 319 or y 199, row wrap near the right edge, 0 outside the buffer | Undefined behaviour in C, given a defined result |  |  |
| U03 | Two alternating players with two diggers index past the digger array | research 01 §1.1, §1.2, §9 | Unrepresentable as a game mode; rejected when settings arrive as separate numbers | Undefined behaviour in C, rejected at configuration time |  |  |
| U04 | A start level of 0 or below gives blank levels and a division by zero in the generator | research 01 §1.1, §9 | The start level accepts 1..=1000 only; the generator's modulus is non-zero by construction | Undefined behaviour in C, rejected at configuration time |  |  |
| U05 | A dig step indexes outside the field | research 01 §5.3, §9 | No cell changes; the penalty is still charged; a debug assertion fires | Undefined behaviour in C, given a defined result |  |  |
| U06 | Signed overflow in the generator and in SplitMix64 | research 01 §7.1, §7.2, §9 | Wrapping arithmetic, as every recording assumes | Undefined behaviour in C, given a defined result |  |  |
| U07 | Reads of unoccupied monster slots, removed bags and out-of-play diggers | research 01 §8, research 02 §9 | Arrays are always fully initialised and keep their last values | Undefined behaviour in C, given a defined result |  |  |
| U08 | The on-screen monster limit has no value for a level below 1 | research 02 §4.2, §7 | Unreachable: the start level is validated | Undefined behaviour in C, unreachable |  |  |
| U09 | The second player's level is left over from the previous game | research 01 §3.1, §10 item 18 | Always reset to level 0 at game start (Q06 covers the fresh-process case recordings exercise) | Accidental behaviour, fixed |  |  |
| U10 | A custom level file is read into 16-byte rows | research 01 §5.5 | Rows are 15 bytes, as evidently intended | Accidental behaviour, fixed |  |  |
| U11 | Quitting during the player announcement skips the end-of-game bookkeeping | research 01 §3.4 | Quit always finishes the game through the same path | Accidental behaviour, fixed |  |  |
| U12 | Timing kludge for recordings made on or before 1998-11-25 | research 01 §7.3, §10 item 17, research 05 §4.1 | Not ported; `add-recording-and-conformance` rejects such files | Dropped feature |  |  |

## Presentation deviations (`add-simulation-core`)

Class: presentation. These change when or how long something is shown, never
the state of the game.

- **Level-complete pause.** A fixed 40 frames instead of "until the jingle
  ends": the simulation never waits on sound (porting plan §4, Sound). 40
  frames at the default 80 ms cover the 3.17 s jingle.
- **Dirge.** No real-time hold beyond its 89 ticks: the in-game wait of the
  grave sequence already covers the 7.05 s tune.
- **Level draw pacing.** The level draw reveals three items per frame instead
  of pacing each item by a third of a frame, so that a draw takes whole
  frames; no tick can observe the difference.
- **Pause requests.** A pause request is honoured only on a main-phase tick,
  because the pause phase is simulation state entered between ticks.
- **Game over notice.** `GAME OVER` is always shown for 50 frames; entering
  initials is left to the desktop app, which acts on the player result the
  step reports, so the simulation never blocks on input.

## Not ported

Class: dropped feature. Each is rejected with a clear message where a user
could ask for it.

| Feature | Rejected by |
|---|---|
| CGA graphics mode | `add-desktop-app` (command line and configuration) |
| Networked play (NetSim) | `add-desktop-app` (command line and configuration) |
| X11 window embedding | `add-desktop-app` (command line) |
| Timing kludge for recordings made on or before 1998-11-25 (U12) | `add-recording-and-conformance` (recording reader) |

## Related tests

Beyond the per-quirk tests, `crates/digger-core/tests/sprite_model.rs`
checks long disciplined operation sequences against a composite-picture
oracle and a brute-force collision scan, and
`crates/digger-core/tests/golden_frames.rs` pins the CRC-32 of the title,
backgrounds, text, numbers and a scripted sprite scene; `cargo xtask
render-sheet` writes those scenes as PPM files for review.

## Findings from recordings

