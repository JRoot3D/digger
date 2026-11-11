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
the recording wins and the entry is corrected.

Id prefixes, so that entries from different changes never collide: `R`
rendering, `S` sprite layer, `A` asset data (all from
`add-assets-and-renderer`); `Q` simulation (`add-simulation-core`); `P`
recording and playback; `SQ` and `SF` sound.

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

## Related tests

Beyond the per-quirk tests, `crates/digger-core/tests/sprite_model.rs`
checks long disciplined operation sequences against a composite-picture
oracle and a brute-force collision scan, and
`crates/digger-core/tests/golden_frames.rs` pins the CRC-32 of the title,
backgrounds, text, numbers and a scripted sprite scene; `cargo xtask
render-sheet` writes those scenes as PPM files for review.
